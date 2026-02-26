"""Test LOCO server connection with fixed crypto handshake."""

import asyncio
import ssl
import struct
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))

from openkakao.auth import get_credentials
from openkakao.crypto import LocoEncryptor
from openkakao.packet import LocoPacket, PacketBuilder


BOOKING_HOST = "booking-loco.kakao.com"
BOOKING_PORT = 443


async def test_booking():
    """Phase 1: Booking via TLS to get checkin server info."""
    print("=" * 60)
    print("[Phase 1] Booking: GETCONF via TLS")
    print("=" * 60)

    builder = PacketBuilder()
    pkt = builder.build("GETCONF", {
        "MCCMNC": "99999",
        "os": "mac",
        "model": "",
    })

    ctx = ssl.create_default_context()
    reader, writer = await asyncio.open_connection(
        BOOKING_HOST, BOOKING_PORT, ssl=ctx
    )

    raw = pkt.encode()
    print(f"  Sending GETCONF ({len(raw)} bytes)")
    print(f"  Header hex: {raw[:22].hex()}")
    writer.write(raw)
    await writer.drain()

    header_data = await asyncio.wait_for(
        reader.readexactly(LocoPacket.HEADER_SIZE), timeout=10
    )
    _, _, method, _, body_length = LocoPacket.decode_header(header_data)
    print(f"  Response method: {method}, body_length: {body_length}")

    body_data = await reader.readexactly(body_length)
    response = LocoPacket.decode(header_data + body_data)

    writer.close()
    await writer.wait_closed()

    body = response.body
    print(f"  Status: {body.get('status')}")

    ticket = body.get("ticket", {})
    lsl = ticket.get("lsl", [])
    print(f"  Checkin hosts (lsl): {lsl}")

    wifi = body.get("wifi", {})
    ports = wifi.get("ports", [])
    print(f"  Ports: {ports}")

    return body


async def test_checkin_legacy(host: str, port: int, user_id: int):
    """Phase 2: Checkin via legacy RSA handshake + AES-CFB."""
    print()
    print("=" * 60)
    print(f"[Phase 2] Checkin: legacy mode -> {host}:{port}")
    print("=" * 60)

    enc = LocoEncryptor()
    handshake = enc.build_handshake_packet()
    print(f"  Handshake packet: {len(handshake)} bytes")
    print(f"  Handshake header (12 bytes): {handshake[:12].hex()}")
    print(f"  AES key: {enc.aes_key.hex()}")

    reader, writer = await asyncio.open_connection(host, port)
    print(f"  TCP connected to {host}:{port}")

    writer.write(handshake)
    await writer.drain()
    print("  Handshake sent, waiting for server acceptance...")

    # Server doesn't respond to handshake; it just accepts it silently.
    # Now send CHECKIN command encrypted with AES.
    builder = PacketBuilder()
    pkt = builder.build("CHECKIN", {
        "userId": user_id,
        "os": "mac",
        "ntype": 0,
        "appVer": "4.5.0",
        "MCCMNC": "99999",
        "lang": "ko",
        "countryISO": "KR",
        "useSub": True,
    })

    raw = pkt.encode()
    print(f"  CHECKIN packet: {len(raw)} bytes")
    print(f"  CHECKIN raw hex (first 22): {raw[:22].hex()}")

    encrypted = enc.encrypt(raw)
    print(f"  Encrypted frame: {len(encrypted)} bytes")

    writer.write(encrypted)
    await writer.drain()
    print("  Encrypted CHECKIN sent, waiting for response...")

    # Read response: 4-byte size prefix + encrypted frame
    size_data = await asyncio.wait_for(reader.readexactly(4), timeout=15)
    size = struct.unpack("<I", size_data)[0]
    print(f"  Response frame size: {size} bytes")

    frame_data = await asyncio.wait_for(reader.readexactly(size), timeout=15)
    print(f"  Got {len(frame_data)} bytes of encrypted data")

    decrypted = enc.decrypt(frame_data)
    print(f"  Decrypted: {len(decrypted)} bytes")
    print(f"  Decrypted hex (first 40): {decrypted[:40].hex()}")

    response = LocoPacket.decode(decrypted)
    print(f"  Response method: {response.method}")
    print(f"  Response status: {response.status_code}")
    print(f"  Response body: {response.body}")

    writer.close()
    await writer.wait_closed()

    return response.body


async def test_checkin_tls(host: str, port: int, user_id: int):
    """Phase 2 alt: Checkin via TLS (V2SL mode)."""
    print()
    print("=" * 60)
    print(f"[Phase 2-TLS] Checkin: TLS mode -> {host}:{port}")
    print("=" * 60)

    ctx = ssl.create_default_context()
    reader, writer = await asyncio.open_connection(host, port, ssl=ctx)
    print(f"  TLS connected to {host}:{port}")

    builder = PacketBuilder()
    pkt = builder.build("CHECKIN", {
        "userId": user_id,
        "os": "mac",
        "ntype": 0,
        "appVer": "4.5.0",
        "MCCMNC": "99999",
        "lang": "ko",
        "countryISO": "KR",
        "useSub": True,
    })

    raw = pkt.encode()
    print(f"  CHECKIN packet: {len(raw)} bytes")

    writer.write(raw)
    await writer.drain()
    print("  CHECKIN sent, waiting for response...")

    header_data = await asyncio.wait_for(
        reader.readexactly(LocoPacket.HEADER_SIZE), timeout=15
    )
    _, _, method, _, body_length = LocoPacket.decode_header(header_data)
    print(f"  Response method: {method}, body_length: {body_length}")

    body_data = await reader.readexactly(body_length)
    response = LocoPacket.decode(header_data + body_data)
    print(f"  Response body: {response.body}")

    writer.close()
    await writer.wait_closed()

    return response.body


async def test_full_login(loco_host: str, loco_port: int, credentials, use_tls: bool):
    """Phase 3: Connect to LOCO server and login."""
    print()
    print("=" * 60)
    print(f"[Phase 3] Login: {'TLS' if use_tls else 'Legacy'} -> {loco_host}:{loco_port}")
    print("=" * 60)

    enc = None
    if use_tls:
        ctx = ssl.create_default_context()
        reader, writer = await asyncio.open_connection(loco_host, loco_port, ssl=ctx)
    else:
        reader, writer = await asyncio.open_connection(loco_host, loco_port)
        enc = LocoEncryptor()
        writer.write(enc.build_handshake_packet())
        await writer.drain()
        print("  Handshake sent")

    builder = PacketBuilder()
    pkt = builder.build("LOGINLIST", {
        "os": "mac",
        "ntype": 0,
        "appVer": "4.5.0",
        "MCCMNC": "99999",
        "prtVer": "1",
        "duuid": credentials.device_uuid or "",
        "oauthToken": credentials.oauth_token,
        "lang": "ko",
        "dtype": 2,
        "revision": 0,
        "chatIds": [],
        "maxIds": [],
        "lastTokenId": 0,
        "lbk": 0,
        "bg": False,
    })

    raw = pkt.encode()
    print(f"  LOGINLIST packet: {len(raw)} bytes")

    if use_tls:
        writer.write(raw)
    else:
        writer.write(enc.encrypt(raw))
    await writer.drain()
    print("  LOGINLIST sent, waiting for response...")

    if use_tls:
        header_data = await asyncio.wait_for(
            reader.readexactly(LocoPacket.HEADER_SIZE), timeout=15
        )
        _, _, method, _, body_length = LocoPacket.decode_header(header_data)
        body_data = await reader.readexactly(body_length)
        response = LocoPacket.decode(header_data + body_data)
    else:
        size_data = await asyncio.wait_for(reader.readexactly(4), timeout=15)
        size = struct.unpack("<I", size_data)[0]
        frame_data = await reader.readexactly(size)
        decrypted = enc.decrypt(frame_data)
        response = LocoPacket.decode(decrypted)

    print(f"  Response method: {response.method}")
    print(f"  Response status: {response.status_code}")
    body = response.body
    print(f"  userId: {body.get('userId')}")
    print(f"  status in body: {body.get('status')}")

    if "chatDatas" in body:
        chats = body["chatDatas"]
        print(f"  Chat rooms: {len(chats)}")
        for c in chats[:5]:
            print(f"    - chatId={c.get('chatId')}, type={c.get('type')}")
    else:
        # Print keys to understand response structure
        print(f"  Response keys: {list(body.keys())}")
        # Print first 500 chars of body
        body_str = str(body)
        if len(body_str) > 500:
            body_str = body_str[:500] + "..."
        print(f"  Body preview: {body_str}")

    writer.close()
    await writer.wait_closed()

    return body


async def main():
    # Extract credentials
    print("[Init] Extracting credentials from Cache.db...")
    creds = get_credentials()
    if not creds:
        print("[FAIL] Could not extract credentials. Is KakaoTalk running?")
        return
    print(f"  Token: {creds.oauth_token[:40]}...")
    print(f"  User ID: {creds.user_id}")
    print()

    # Phase 1: Booking
    try:
        config = await test_booking()
    except Exception as e:
        print(f"  [FAIL] Booking failed: {e}")
        import traceback; traceback.print_exc()
        return

    ticket = config.get("ticket", {})
    lsl = ticket.get("lsl", [])
    wifi = config.get("wifi", {})
    ports = wifi.get("ports", [])

    if not lsl:
        print("[FAIL] No checkin hosts returned")
        return

    checkin_host = lsl[0]
    checkin_port = ports[0] if ports else 5223

    # Phase 2: Try checkin with multiple approaches
    checkin_result = None
    use_tls_for_loco = True

    # Try 1: TLS on port 443
    try:
        checkin_result = await test_checkin_tls(checkin_host, 443, creds.user_id)
        use_tls_for_loco = True
    except Exception as e:
        print(f"  [FAIL] TLS:443 failed: {e}")

    # Try 2: Legacy on checkin_port
    if not checkin_result or not checkin_result.get("host"):
        try:
            checkin_result = await test_checkin_legacy(
                checkin_host, checkin_port, creds.user_id
            )
            use_tls_for_loco = False
        except Exception as e:
            print(f"  [FAIL] Legacy:{checkin_port} failed: {e}")

    # Try 3: TLS on checkin_port
    if not checkin_result or not checkin_result.get("host"):
        try:
            checkin_result = await test_checkin_tls(
                checkin_host, checkin_port, creds.user_id
            )
            use_tls_for_loco = True
        except Exception as e:
            print(f"  [FAIL] TLS:{checkin_port} failed: {e}")

    # Try 4: Legacy on other ports
    if not checkin_result or not checkin_result.get("host"):
        for p in ports[1:5]:
            try:
                checkin_result = await test_checkin_legacy(
                    checkin_host, p, creds.user_id
                )
                use_tls_for_loco = False
                if checkin_result and checkin_result.get("host"):
                    break
            except Exception as e:
                print(f"  [FAIL] Legacy:{p} failed: {e}")

    if not checkin_result or not checkin_result.get("host"):
        print("\n[FAIL] All checkin attempts failed")
        return

    loco_host = checkin_result["host"]
    loco_port = checkin_result.get("port", 5223)
    print(f"\n[OK] Checkin succeeded -> LOCO server: {loco_host}:{loco_port}")

    # Phase 3: Login
    try:
        login_result = await test_full_login(
            loco_host, loco_port, creds, use_tls=use_tls_for_loco
        )
    except Exception as e:
        print(f"  [FAIL] Login failed: {e}")
        import traceback; traceback.print_exc()

        # Retry with opposite TLS setting
        print(f"\n  Retrying with TLS={not use_tls_for_loco}...")
        try:
            login_result = await test_full_login(
                loco_host, loco_port, creds, use_tls=not use_tls_for_loco
            )
        except Exception as e2:
            print(f"  [FAIL] Retry also failed: {e2}")
            import traceback; traceback.print_exc()


if __name__ == "__main__":
    asyncio.run(main())
