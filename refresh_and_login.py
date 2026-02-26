"""Restart KakaoTalk, wait for fresh token, then run LOCO login."""

import asyncio
import os
import struct
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, os.path.dirname(__file__))

from kakaotalk_cli.auth import get_credentials
from kakaotalk_cli.crypto import LocoEncryptor
from kakaotalk_cli.packet import LocoPacket, PacketBuilder

CACHE_DB = Path.home() / "Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Caches/Cache.db"


def wait_for_fresh_token(old_timestamp, timeout=60):
    """Wait until a token newer than old_timestamp appears in cache."""
    import plistlib
    import shutil
    import sqlite3
    import tempfile

    start = time.time()
    while time.time() - start < timeout:
        time.sleep(2)
        tmp_dir = tempfile.mkdtemp()
        try:
            tmp_db = Path(tmp_dir) / "Cache.db"
            shutil.copy2(CACHE_DB, tmp_db)
            wal = CACHE_DB.with_suffix(".db-wal")
            shm = CACHE_DB.with_suffix(".db-shm")
            if wal.exists():
                shutil.copy2(wal, tmp_db.with_suffix(".db-wal"))
            if shm.exists():
                shutil.copy2(shm, tmp_db.with_suffix(".db-shm"))

            conn = sqlite3.connect(str(tmp_db))
            c = conn.cursor()
            c.execute("""
                SELECT r.time_stamp, b.request_object
                FROM cfurl_cache_blob_data b
                JOIN cfurl_cache_response r ON b.entry_ID = r.entry_ID
                WHERE b.request_object IS NOT NULL
                ORDER BY r.time_stamp DESC
                LIMIT 10
            """)
            for ts, req_obj in c.fetchall():
                if not req_obj:
                    continue
                try:
                    parsed = plistlib.loads(req_obj)
                    arr = parsed.get("Array", [])
                    for item in arr:
                        if isinstance(item, dict) and "Authorization" in item:
                            if ts > old_timestamp:
                                conn.close()
                                print(f"  Fresh token found! (ts={ts})")
                                return True
                except Exception:
                    continue
            conn.close()
        finally:
            shutil.rmtree(tmp_dir, ignore_errors=True)

        elapsed = int(time.time() - start)
        print(f"  Waiting for fresh token... ({elapsed}s)", end="\r")

    return False


async def loco_login():
    creds = get_credentials()
    if not creds:
        print("[FAIL] No credentials found.")
        return

    print(f"User ID: {creds.user_id}")
    print(f"Token: {creds.oauth_token[:50]}...")
    print(f"Device UUID: {creds.device_uuid[:30]}...")
    print(f"App Version: {creds.app_version}")
    print()

    # Phase 1: Checkin
    print("=" * 50)
    print("[Phase 1] Checkin")
    print("=" * 50)

    enc = LocoEncryptor()
    reader, writer = await asyncio.open_connection("ticket-loco.kakao.com", 995)
    writer.write(enc.build_handshake_packet())
    await writer.drain()

    builder = PacketBuilder()
    pkt = builder.build("CHECKIN", {
        "userId": creds.user_id,
        "os": "mac",
        "ntype": 0,
        "appVer": creds.app_version,
        "MCCMNC": "99999",
        "lang": "ko",
        "countryISO": "KR",
        "useSub": True,
    })
    writer.write(enc.encrypt(pkt.encode()))
    await writer.drain()

    data = await asyncio.wait_for(reader.read(4096), timeout=10)
    size = struct.unpack("<I", data[:4])[0]
    checkin_resp = LocoPacket.decode(enc.decrypt(data[4:4 + size]))
    writer.close()
    await writer.wait_closed()

    body = checkin_resp.body
    if body.get("status") != 0:
        print(f"  [FAIL] Checkin status: {body.get('status')}")
        return

    loco_host = body["host"]
    loco_port = body["port"]
    print(f"  LOCO server: {loco_host}:{loco_port}")

    # Phase 2: Login
    print()
    print("=" * 50)
    print("[Phase 2] LOGINLIST")
    print("=" * 50)

    enc2 = LocoEncryptor()
    reader2, writer2 = await asyncio.open_connection(loco_host, loco_port)
    writer2.write(enc2.build_handshake_packet())
    await writer2.drain()

    builder2 = PacketBuilder()
    login_pkt = builder2.build("LOGINLIST", {
        "appVer": creds.app_version,
        "os": "mac",
        "lang": "ko",
        "duuid": creds.device_uuid,
        "oauthToken": creds.oauth_token,
        "ntype": 0,
        "MCCMNC": "99999",
        "revision": 0,
        "dtype": 2,
        "bg": False,
        "chatIds": [],
        "maxIds": [],
        "lastTokenId": 0,
        "lbk": 0,
    })

    writer2.write(enc2.encrypt(login_pkt.encode()))
    await writer2.drain()
    print("  LOGINLIST sent...")

    # Read response
    all_data = b""
    try:
        while True:
            chunk = await asyncio.wait_for(reader2.read(65536), timeout=15)
            if not chunk:
                break
            all_data += chunk
            if len(all_data) >= 4:
                frame_size = struct.unpack("<I", all_data[:4])[0]
                if len(all_data) >= 4 + frame_size:
                    break
    except asyncio.TimeoutError:
        pass

    if not all_data:
        print("  [FAIL] No response")
        return

    frame_size = struct.unpack("<I", all_data[:4])[0]
    if len(all_data) < 4 + frame_size:
        print(f"  [FAIL] Incomplete response: {len(all_data)}/{4 + frame_size}")
        return

    decrypted = enc2.decrypt(all_data[4:4 + frame_size])
    resp = LocoPacket.decode(decrypted)
    login_body = resp.body
    status = login_body.get("status")
    print(f"  Method: {resp.method}")
    print(f"  Status: {status}")

    if status == 0:
        print(f"  userId: {login_body.get('userId')}")
        if "chatDatas" in login_body:
            chats = login_body["chatDatas"]
            print(f"\n  Chat rooms: {len(chats)}")
            for c in chats[:10]:
                chat_id = c.get("chatId", "?")
                chat_type = c.get("type", "?")
                last_log = c.get("l", {})
                msg = ""
                if isinstance(last_log, dict):
                    msg = str(last_log.get("message", ""))[:50]
                print(f"    chatId={chat_id} type={chat_type} last=\"{msg}\"")
        print("\n  [OK] Login successful!")
    elif status == -950:
        print("  [FAIL] Token expired (-950). Need fresher token.")
    elif status == -300:
        print("  [FAIL] Invalid device (-300).")
    elif status == -203:
        print("  [FAIL] Authentication error (-203).")
    else:
        print(f"  [FAIL] Unknown error. Body keys: {list(login_body.keys())}")
        print(f"  Body: {str(login_body)[:500]}")

    writer2.close()
    try:
        await writer2.wait_closed()
    except Exception:
        pass


def main():
    print("=" * 50)
    print("KakaoTalk LOCO Login - Token Refresh")
    print("=" * 50)

    # Check current token age
    creds = get_credentials()
    if creds:
        print(f"Current token: {creds.oauth_token[:30]}... (may be stale)")
    else:
        print("No cached token found.")

    print()
    answer = input("Restart KakaoTalk to get fresh token? [y/N] ").strip().lower()
    if answer == "y":
        print("\n[1] Quitting KakaoTalk...")
        subprocess.run(["osascript", "-e", 'tell application "KakaoTalk" to quit'], capture_output=True)
        time.sleep(3)

        print("[2] Launching KakaoTalk...")
        subprocess.run(["open", "-a", "KakaoTalk"], capture_output=True)

        print("[3] Waiting for fresh token (up to 60s)...")
        old_ts = "2026-02-26 07:16:13"  # last known timestamp
        if not wait_for_fresh_token(old_ts, timeout=60):
            print("\n  [WARN] Timeout waiting for fresh token. Trying anyway...")
        print()

    print("\n[4] Running LOCO login...\n")
    asyncio.run(loco_login())


if __name__ == "__main__":
    main()
