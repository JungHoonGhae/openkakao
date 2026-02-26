#!/usr/bin/env python3
"""Capture LOCO tokens by acting as a transparent TLS proxy.

Instead of using mitmproxy's complex setup, this script:
1. Resolves the actual KakaoTalk LOCO server IP
2. Creates a local TLS proxy that relays traffic
3. Uses /etc/hosts or pf to redirect KakaoTalk traffic to our proxy
4. Decodes LOCO packets passing through

Usage:
    sudo python3 scripts/capture_loco.py

The script modifies /etc/hosts to redirect ticket-loco.kakao.com
to 127.0.0.1, runs the proxy, then restores /etc/hosts on exit.
After starting, restart KakaoTalk to capture the CHECKIN/LOGINLIST flow.
"""

import asyncio
import json
import signal
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
from datetime import datetime
from pathlib import Path

# Add parent dir to path
sys.path.insert(0, str(Path(__file__).parent.parent))

from kakaotalk_cli.packet import LocoPacket

OUTPUT_FILE = Path("/tmp/kakao_captured_tokens.json")
PROXY_PORT = 5223

captured_data = {
    "timestamp": "",
    "tokens": {},
    "checkin": {},
    "packets": [],
}


def save_captured():
    captured_data["timestamp"] = datetime.now().isoformat()
    with open(OUTPUT_FILE, "w") as f:
        json.dump(captured_data, f, indent=2, default=str)


def parse_loco_packet(data: bytes, direction: str):
    """Try to parse data as a LOCO packet and extract interesting fields."""
    if len(data) < 22:
        return

    try:
        pkt = LocoPacket.decode(data)
        print(f"  [{direction}] {pkt.method} (id={pkt.packet_id}, status={pkt.status_code})")

        body = pkt.body
        if body:
            print(f"    Keys: {list(body.keys())}")

        # Save interesting packets
        if pkt.method in ("LOGINLIST", "CHECKIN", "GETCONF", "LOGIN"):
            packet_info = {
                "direction": direction,
                "method": pkt.method,
                "status": pkt.status_code,
                "body": {k: str(v)[:200] for k, v in body.items()},
            }
            captured_data["packets"].append(packet_info)

            # Extract tokens
            for key in ("oauthToken", "accessToken", "userId", "duuid", "host", "port"):
                if key in body:
                    captured_data["tokens"][key] = body[key]
                    print(f"    *** {key}: {str(body[key])[:80]}")

            if pkt.method == "CHECKIN" and "host" in body:
                captured_data["checkin"] = {
                    "host": body.get("host"),
                    "port": body.get("port"),
                }

            save_captured()

    except Exception:
        if len(data) > 4:
            print(f"  [{direction}] Raw data ({len(data)} bytes): {data[:40].hex()}")


async def relay(reader, writer, direction, other_writer):
    """Relay data between client and server, inspecting LOCO packets."""
    try:
        while True:
            data = await reader.read(65536)
            if not data:
                break
            parse_loco_packet(data, direction)
            other_writer.write(data)
            await other_writer.drain()
    except Exception:
        pass
    finally:
        try:
            other_writer.close()
        except Exception:
            pass


async def handle_connection(client_reader, client_writer, real_host, real_port):
    """Handle a proxied connection: connect to real server and relay."""
    client_addr = client_writer.get_extra_info("peername")
    print(f"\n[proxy] New connection from {client_addr}")
    print(f"[proxy] Connecting to real server {real_host}:{real_port}")

    try:
        ctx = ssl.create_default_context()
        server_reader, server_writer = await asyncio.open_connection(
            real_host, real_port, ssl=ctx
        )
        print("[proxy] Connected to real server")

        await asyncio.gather(
            relay(client_reader, client_writer, "CLIENT>>>", server_writer),
            relay(server_reader, server_writer, "<<<SERVER", client_writer),
        )
    except Exception as e:
        print(f"[proxy] Error: {e}")
    finally:
        client_writer.close()
        print("[proxy] Connection closed")


def generate_self_signed_cert() -> tuple[str, str]:
    """Generate a self-signed certificate for TLS interception."""
    cert_dir = Path(tempfile.mkdtemp(prefix="kakao_cert_"))
    cert_path = str(cert_dir / "cert.pem")
    key_path = str(cert_dir / "key.pem")

    subprocess.run(
        [
            "openssl", "req", "-x509", "-newkey", "rsa:2048",
            "-keyout", key_path, "-out", cert_path,
            "-days", "1", "-nodes",
            "-subj", "/CN=ticket-loco.kakao.com",
            "-addext", "subjectAltName=DNS:ticket-loco.kakao.com,DNS:*.kakao.com",
        ],
        capture_output=True,
        check=True,
    )
    return cert_path, key_path


async def run_proxy(real_host: str, real_port: int, listen_port: int):
    """Run a TCP proxy that relays and inspects LOCO traffic."""

    cert_path, key_path = generate_self_signed_cert()

    ssl_ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ssl_ctx.load_cert_chain(cert_path, key_path)

    async def on_connect(reader, writer):
        await handle_connection(reader, writer, real_host, real_port)

    server = await asyncio.start_server(
        on_connect, "127.0.0.1", listen_port, ssl=ssl_ctx
    )
    print(f"[proxy] Listening on 127.0.0.1:{listen_port} (TLS)")
    print(f"[proxy] Forwarding to {real_host}:{real_port}")

    async with server:
        await server.serve_forever()


def main():
    import os

    if os.geteuid() != 0:
        print("This script needs root to modify /etc/hosts.")
        print("Run with: sudo python3 scripts/capture_loco.py")
        print()
        print("Alternative: manually add to /etc/hosts:")
        print("  127.0.0.1 ticket-loco.kakao.com")
        print()
        print("Then run without sudo:")
        print("  python3 scripts/capture_loco.py --no-hosts")
        sys.exit(1)

    real_host = "ticket-loco.kakao.com"
    real_ip = socket.gethostbyname(real_host)
    print(f"[*] {real_host} -> {real_ip}")

    hosts_path = Path("/etc/hosts")
    hosts_backup = hosts_path.read_text()
    hosts_entry = f"\n127.0.0.1 {real_host}\n"

    def restore_hosts(*_args):
        hosts_path.write_text(hosts_backup)
        print("[*] Restored /etc/hosts")
        sys.exit(0)

    signal.signal(signal.SIGINT, restore_hosts)
    signal.signal(signal.SIGTERM, restore_hosts)

    try:
        hosts_path.write_text(hosts_backup + hosts_entry)
        print(f"[*] Modified /etc/hosts: {real_host} -> 127.0.0.1")
        print()
        print(">>> Restart KakaoTalk now to capture login traffic <<<")
        print(">>> Press Ctrl+C to stop and restore /etc/hosts <<<")
        print()

        asyncio.run(run_proxy(real_ip, 443, PROXY_PORT))
    finally:
        hosts_path.write_text(hosts_backup)
        print("[*] Restored /etc/hosts")


def main_no_hosts():
    """Run proxy without modifying /etc/hosts."""
    real_host = "ticket-loco.kakao.com"
    real_ip = socket.gethostbyname(real_host)
    print(f"[*] {real_host} -> {real_ip}")
    print()
    print("Make sure /etc/hosts has: 127.0.0.1 ticket-loco.kakao.com")
    print()
    asyncio.run(run_proxy(real_ip, 443, PROXY_PORT))


if __name__ == "__main__":
    if "--no-hosts" in sys.argv:
        main_no_hosts()
    else:
        main()
