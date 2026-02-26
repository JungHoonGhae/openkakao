"""Authentication: extract tokens from KakaoTalk desktop app's cached data."""

import plistlib
import sqlite3
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass
class KakaoCredentials:
    oauth_token: str
    user_id: int
    device_uuid: str
    device_name: str
    app_version: str = "3.7.0"
    user_agent: str = ""
    a_header: str = ""


CONTAINER = Path.home() / "Library/Containers/com.kakao.KakaoTalkMac/Data"
CACHE_DB = CONTAINER / "Library/Caches/Cache.db"


def _extract_from_cache_db() -> KakaoCredentials | None:
    """Extract OAuth token and user info from KakaoTalk's URL cache database.

    The Mac KakaoTalk app makes REST API calls to katalk.kakao.com,
    talk-pilsner.kakao.com, and bzm-capi.kakao.com with Authorization
    headers. These are cached in the standard macOS NSURLCache SQLite database.
    """
    if not CACHE_DB.exists():
        print("[auth] Cache.db not found", file=sys.stderr)
        return None

    # Copy DB to temp to include WAL data
    import shutil
    import tempfile

    tmp_dir = tempfile.mkdtemp()
    tmp_db = Path(tmp_dir) / "Cache.db"
    shutil.copy2(CACHE_DB, tmp_db)
    wal = CACHE_DB.with_suffix(".db-wal")
    shm = CACHE_DB.with_suffix(".db-shm")
    if wal.exists():
        shutil.copy2(wal, tmp_db.with_suffix(".db-wal"))
    if shm.exists():
        shutil.copy2(shm, tmp_db.with_suffix(".db-shm"))

    try:
        conn = sqlite3.connect(str(tmp_db))
        cursor = conn.cursor()

        # Find ALL cached requests with Authorization headers, newest first
        cursor.execute("""
            SELECT b.request_object, r.request_key, r.time_stamp
            FROM cfurl_cache_blob_data b
            JOIN cfurl_cache_response r ON b.entry_ID = r.entry_ID
            WHERE b.request_object IS NOT NULL
            ORDER BY r.time_stamp DESC
            LIMIT 50
        """)

        best_token = None
        best_timestamp = None
        best_user_id = 0
        best_user_agent = ""
        best_a_header = ""

        for row in cursor.fetchall():
            req_obj, url, timestamp = row
            if not req_obj:
                continue

            try:
                parsed = plistlib.loads(req_obj)
            except Exception:
                continue

            arr = parsed.get("Array", [])
            headers = None
            for item in arr:
                if isinstance(item, dict) and "Authorization" in item:
                    headers = item
                    break

            if not headers:
                continue

            auth_token = headers.get("Authorization", "")
            if not auth_token:
                continue

            user_id_str = headers.get("talk-user-id", "")
            user_agent = headers.get("User-Agent", "")
            a_header = headers.get("A", "")

            # Track user_id from any request
            if user_id_str and not best_user_id:
                best_user_id = int(user_id_str)

            # Keep the most recent token
            if best_token is None:
                best_token = auth_token
                best_timestamp = timestamp
                best_user_agent = user_agent or best_user_agent
                best_a_header = a_header or best_a_header

        conn.close()

        # Cleanup temp files
        shutil.rmtree(tmp_dir, ignore_errors=True)

        if not best_token:
            return None

        # Extract device_uuid from token (part after the dash)
        device_uuid = ""
        if "-" in best_token:
            device_uuid = best_token.split("-", 1)[1]

        # Extract app version from A header (e.g., "mac/3.7.0/ko")
        app_version = "3.7.0"
        if best_a_header:
            parts = best_a_header.split("/")
            if len(parts) >= 2:
                app_version = parts[1]

        print(f"[auth] Token extracted from cache (timestamp: {best_timestamp})", file=sys.stderr)
        print(f"[auth] User ID: {best_user_id}", file=sys.stderr)
        print(f"[auth] Token: {best_token[:30]}...", file=sys.stderr)

        return KakaoCredentials(
            oauth_token=best_token,
            user_id=best_user_id,
            device_uuid=device_uuid,
            device_name="openkakao",
            app_version=app_version,
            user_agent=best_user_agent,
            a_header=best_a_header,
        )

    except Exception as e:
        print(f"[auth] Cache extraction error: {e}", file=sys.stderr)
        shutil.rmtree(tmp_dir, ignore_errors=True)

    return None


def get_credentials() -> KakaoCredentials | None:
    """Extract KakaoTalk credentials from the desktop app's cache."""
    return _extract_from_cache_db()


def get_credentials_interactive() -> KakaoCredentials:
    """Prompt the user for credentials if auto-extraction fails."""
    print("Could not auto-extract KakaoTalk credentials.")
    print("Please provide them manually.\n")
    print("You can find the OAuth token by inspecting KakaoTalk traffic")
    print("with mitmproxy or from the app's cache database.\n")

    oauth_token = input("OAuth Token (Authorization header value): ").strip()
    user_id_str = input("User ID (numeric, from talk-user-id header): ").strip()

    return KakaoCredentials(
        oauth_token=oauth_token,
        user_id=int(user_id_str) if user_id_str else 0,
        device_uuid="",
        device_name="openkakao",
    )
