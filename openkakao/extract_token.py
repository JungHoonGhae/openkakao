"""Extract OAuth token from the running KakaoTalk process.

Uses macOS-specific techniques to read token data from the KakaoTalk
desktop app that is already logged in.

Methods attempted:
1. Read from KakaoTalk's encrypted SQLite database
2. Parse binary cookie files
3. Search process memory via lldb (requires SIP bypass or entitlements)
4. Intercept LOCO re-authentication traffic via mitmproxy
"""

import json
import os
import plistlib
import sqlite3
import struct
import subprocess
import sys
import tempfile
from pathlib import Path


CONTAINER = Path.home() / "Library/Containers/com.kakao.KakaoTalkMac/Data"
APP_SUPPORT = CONTAINER / "Library/Application Support/com.kakao.KakaoTalkMac"


def extract_from_binary_cookies() -> dict | None:
    """Try to extract auth cookies from KakaoTalk's binarycookies file."""
    cookie_path = CONTAINER / "Library/Cookies/Cookies.binarycookies"
    if not cookie_path.exists():
        return None

    try:
        data = cookie_path.read_bytes()
        # Binary cookies format: "cook" magic, then pages
        if data[:4] != b"cook":
            return None

        # Simple extraction of cookie strings
        result = {}
        # Look for oauth/token related strings in the raw binary
        for keyword in [b"_karauth", b"_kaauth", b"oauthToken", b"accessToken", b"TIARA"]:
            idx = data.find(keyword)
            if idx >= 0:
                # Extract surrounding context
                start = max(0, idx - 50)
                end = min(len(data), idx + 200)
                context = data[start:end]
                result[keyword.decode(errors="ignore")] = f"Found at offset {idx}"
        return result if result else None
    except Exception as e:
        print(f"[extract] Cookie parse error: {e}", file=sys.stderr)
        return None


def extract_from_sqlite_db() -> dict | None:
    """Try to read KakaoTalk's local SQLite databases."""
    results = {}

    # Check all potential SQLite files
    for db_file in APP_SUPPORT.glob("*"):
        if not db_file.is_file():
            continue
        # Try to open as SQLite
        try:
            conn = sqlite3.connect(f"file:{db_file}?mode=ro", uri=True)
            cursor = conn.cursor()

            # Get table list
            cursor.execute("SELECT name FROM sqlite_master WHERE type='table'")
            tables = [row[0] for row in cursor.fetchall()]

            if tables:
                results[db_file.name] = {
                    "tables": tables,
                    "path": str(db_file),
                }

                # Look for token-related tables
                for table in tables:
                    table_lower = table.lower()
                    if any(k in table_lower for k in ("token", "auth", "user", "account", "session", "config")):
                        try:
                            cursor.execute(f"SELECT * FROM [{table}] LIMIT 5")
                            rows = cursor.fetchall()
                            cols = [d[0] for d in cursor.description]
                            results[db_file.name][f"table_{table}"] = {
                                "columns": cols,
                                "rows": [list(r) for r in rows],
                            }
                        except Exception:
                            pass

            conn.close()
        except Exception:
            pass

    return results if results else None


def try_nscoder_decode(data: bytes) -> str | None:
    """Try to decode NSCoded/plist binary data."""
    try:
        decoded = plistlib.loads(data)
        return str(decoded)
    except Exception:
        return None


def search_app_data() -> dict:
    """Search all KakaoTalk app data for potential token storage."""
    findings = {}

    # 1. Binary cookies
    cookies = extract_from_binary_cookies()
    if cookies:
        findings["binary_cookies"] = cookies

    # 2. SQLite databases
    sqlite_data = extract_from_sqlite_db()
    if sqlite_data:
        findings["sqlite"] = sqlite_data

    # 3. Check plist binary blobs
    plist_path = CONTAINER / "Library/Preferences/com.kakao.KakaoTalkMac.9E48A64E85D8EB19AFB3DBF7E12A09E463F4C93B.plist"
    if plist_path.exists():
        try:
            with open(plist_path, "rb") as f:
                plist = plistlib.load(f)

            for key, value in plist.items():
                if isinstance(value, bytes) and len(value) > 20:
                    decoded = try_nscoder_decode(value)
                    if decoded and any(k in decoded.lower() for k in ("token", "oauth", "auth")):
                        findings[f"plist_{key}"] = decoded[:500]
        except Exception as e:
            findings["plist_error"] = str(e)

    # 4. Check Cache.db for API responses containing tokens
    cache_db = CONTAINER / "Library/Caches/Cache.db"
    if cache_db.exists():
        try:
            conn = sqlite3.connect(f"file:{cache_db}?mode=ro", uri=True)
            cursor = conn.cursor()
            cursor.execute("""
                SELECT request_key, time_stamp
                FROM cfurl_cache_response
                WHERE request_key LIKE '%kakao%' OR request_key LIKE '%auth%' OR request_key LIKE '%token%'
                ORDER BY time_stamp DESC LIMIT 20
            """)
            rows = cursor.fetchall()
            if rows:
                findings["cache_urls"] = [(r[0], r[1]) for r in rows]
            conn.close()
        except Exception as e:
            findings["cache_error"] = str(e)

    return findings


def create_mitmproxy_addon_script() -> str:
    """Create a mitmproxy addon script to capture KakaoTalk LOCO traffic."""
    script = '''"""mitmproxy addon to intercept KakaoTalk LOCO auth traffic."""
import json
import struct
import sys
from mitmproxy import tcp, ctx

class KakaoLocoCapture:
    """Capture and decode LOCO protocol packets from KakaoTalk."""

    def __init__(self):
        self.captured_tokens = {}

    def tcp_message(self, flow: tcp.TCPFlow):
        """Intercept TCP messages (LOCO is raw TCP)."""
        message = flow.messages[-1]
        data = message.content

        if len(data) < 22:
            return

        # Try to parse as LOCO packet header
        try:
            packet_id, status_code = struct.unpack_from("<Ih", data, 0)
            method = data[6:17].rstrip(b"\\x00").decode("ascii", errors="ignore")
            body_type = data[17]
            body_length = struct.unpack_from("<I", data, 18)[0]

            if method in ("LOGINLIST", "CHECKIN", "GETCONF"):
                ctx.log.info(f"[LOCO] {method} (id={packet_id}, status={status_code})")

                # Try BSON decode
                body_bytes = data[22:22+body_length]
                if body_bytes:
                    try:
                        import bson
                        body = bson.BSON(body_bytes).decode()

                        # Extract tokens
                        for key in ("oauthToken", "accessToken", "userId", "duuid"):
                            if key in body:
                                self.captured_tokens[key] = body[key]
                                ctx.log.info(f"  {key}: {str(body[key])[:50]}")

                        # Save to file
                        if self.captured_tokens:
                            with open("/tmp/kakao_tokens.json", "w") as f:
                                json.dump(self.captured_tokens, f, indent=2, default=str)
                            ctx.log.info(f"  Tokens saved to /tmp/kakao_tokens.json")
                    except Exception as e:
                        ctx.log.info(f"  BSON decode error: {e}")

        except Exception:
            pass


addons = [KakaoLocoCapture()]
'''
    script_path = "/tmp/kakao_loco_capture.py"
    with open(script_path, "w") as f:
        f.write(script)
    return script_path


if __name__ == "__main__":
    print("Searching KakaoTalk app data for tokens...\n")
    findings = search_app_data()

    if findings:
        print(json.dumps(findings, indent=2, default=str))
    else:
        print("No token data found in app data files.")

    print("\n---")
    print("To capture tokens via traffic interception:")
    script_path = create_mitmproxy_addon_script()
    print(f"1. Run: mitmdump --mode transparent --tcp-hosts '.*kakao.*' -s {script_path}")
    print("2. Restart KakaoTalk to trigger re-authentication")
    print("3. Tokens will be saved to /tmp/kakao_tokens.json")
