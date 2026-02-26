"""Full LOCO login test: booking -> checkin -> login."""

import asyncio
import struct
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))

from kakaotalk_cli.auth import get_credentials
from kakaotalk_cli.crypto import LocoEncryptor
from kakaotalk_cli.packet import LocoPacket, PacketBuilder


async def loco_login():
    creds = get_credentials()
    if not creds:
        print("[FAIL] No credentials found. Is KakaoTalk running?")
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

    # Read response (might be large)
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
        print("  [FAIL] Token expired. Restart KakaoTalk to refresh.")
    elif status == -300:
        print("  [FAIL] Invalid device (-300). Check duuid field.")
    elif status == -203:
        print("  [FAIL] Authentication error (-203).")
    else:
        print(f"  [FAIL] Unknown error. Body: {str(login_body)[:500]}")

    writer2.close()
    try:
        await writer2.wait_closed()
    except Exception:
        pass


if __name__ == "__main__":
    asyncio.run(loco_login())
