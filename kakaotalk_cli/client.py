"""LOCO protocol client: booking -> checkin -> login -> messaging.

Uses V2SL mode: raw LOCO packets over TLS (no RSA handshake needed).
Falls back to legacy mode (RSA handshake + AES-CFB) if V2SL is unavailable.
"""

import asyncio
import ssl
import struct
import sys
from dataclasses import dataclass, field
from typing import Any, Callable

from .auth import KakaoCredentials
from .crypto import LocoEncryptor
from .packet import LocoPacket, PacketBuilder

# Server configuration
BOOKING_HOST = "booking-loco.kakao.com"
BOOKING_PORT = 443

LOCO_PORTS = [5223, 5228, 9282, 5242, 10009, 995, 8080]
DEFAULT_LOCO_PORT = 5223


@dataclass
class ChatRoom:
    chat_id: int
    type: str = ""
    name: str = ""
    members: list[dict] = field(default_factory=list)
    last_log_id: int = 0
    last_seen_log_id: int = 0
    unread_count: int = 0


@dataclass
class ChatMessage:
    log_id: int
    chat_id: int
    author_id: int
    author_nickname: str
    message: str
    msg_type: int = 1
    send_at: int = 0


def _make_ssl_context() -> ssl.SSLContext:
    ctx = ssl.create_default_context()
    return ctx


async def _loco_oneshot(host: str, port: int, packet: LocoPacket, use_tls: bool = True) -> LocoPacket:
    """Open a connection, send one LOCO packet, read one response, close.

    In V2SL mode (use_tls=True): raw LOCO over TLS.
    In legacy mode (use_tls=False): RSA handshake + AES-CFB encrypted LOCO.
    """
    if use_tls:
        ctx = _make_ssl_context()
        reader, writer = await asyncio.open_connection(host, port, ssl=ctx)
        writer.write(packet.encode())
        await writer.drain()

        # Read response: LOCO header (22 bytes) to get body length, then body
        header_data = await reader.readexactly(LocoPacket.HEADER_SIZE)
        _, _, _, _, body_length = LocoPacket.decode_header(header_data)
        body_data = await reader.readexactly(body_length)
        response = LocoPacket.decode(header_data + body_data)
    else:
        enc = LocoEncryptor()
        reader, writer = await asyncio.open_connection(host, port)
        writer.write(enc.build_handshake_packet())
        await writer.drain()

        encrypted = enc.encrypt(packet.encode())
        writer.write(encrypted)
        await writer.drain()

        size_data = await reader.readexactly(4)
        size = struct.unpack("<I", size_data)[0]
        frame_data = await reader.readexactly(size)
        decrypted = enc.decrypt(frame_data)
        response = LocoPacket.decode(decrypted)

    writer.close()
    await writer.wait_closed()
    return response


class LocoClient:
    """Async LOCO protocol client with V2SL (TLS) support."""

    def __init__(self, credentials: KakaoCredentials):
        self.credentials = credentials
        self.packet_builder = PacketBuilder()
        self._reader: asyncio.StreamReader | None = None
        self._writer: asyncio.StreamWriter | None = None
        self._pending: dict[int, asyncio.Future] = {}
        self._push_handlers: dict[str, list[Callable]] = {}
        self._receive_task: asyncio.Task | None = None
        self._connected = False
        self._use_tls = True
        self._encryptor: LocoEncryptor | None = None
        self.user_id: int = 0
        self.chat_rooms: dict[int, ChatRoom] = {}

    async def connect(self, host: str, port: int, use_tls: bool = True):
        """Connect to a LOCO server."""
        self._use_tls = use_tls
        print(f"[loco] Connecting to {host}:{port} (TLS={use_tls})...", file=sys.stderr)

        if use_tls:
            ctx = _make_ssl_context()
            self._reader, self._writer = await asyncio.open_connection(host, port, ssl=ctx)
        else:
            self._reader, self._writer = await asyncio.open_connection(host, port)
            self._encryptor = LocoEncryptor()
            self._writer.write(self._encryptor.build_handshake_packet())
            await self._writer.drain()

        self._connected = True
        self._receive_task = asyncio.create_task(self._receive_loop())
        print("[loco] Connected", file=sys.stderr)

    async def disconnect(self):
        self._connected = False
        if self._receive_task:
            self._receive_task.cancel()
            try:
                await self._receive_task
            except asyncio.CancelledError:
                pass
        if self._writer:
            self._writer.close()
            try:
                await self._writer.wait_closed()
            except Exception:
                pass

    async def _send_packet(self, packet: LocoPacket) -> LocoPacket:
        raw = packet.encode()

        if self._use_tls:
            data_to_send = raw
        else:
            data_to_send = self._encryptor.encrypt(raw)

        future = asyncio.get_event_loop().create_future()
        self._pending[packet.packet_id] = future

        self._writer.write(data_to_send)
        await self._writer.drain()

        return await asyncio.wait_for(future, timeout=30.0)

    async def _receive_loop(self):
        try:
            while self._connected:
                if self._use_tls:
                    # V2SL: raw LOCO packets over TLS
                    header_data = await self._reader.readexactly(LocoPacket.HEADER_SIZE)
                    _, _, _, _, body_length = LocoPacket.decode_header(header_data)
                    body_data = await self._reader.readexactly(body_length)
                    packet = LocoPacket.decode(header_data + body_data)
                else:
                    # Legacy: AES-encrypted frames
                    size_data = await self._reader.readexactly(4)
                    size = struct.unpack("<I", size_data)[0]
                    frame_data = await self._reader.readexactly(size)
                    decrypted = self._encryptor.decrypt(frame_data)
                    packet = LocoPacket.decode(decrypted)

                if packet.packet_id in self._pending:
                    self._pending.pop(packet.packet_id).set_result(packet)
                else:
                    await self._handle_push(packet)

        except asyncio.IncompleteReadError:
            print("[loco] Connection closed by server", file=sys.stderr)
            self._connected = False
        except asyncio.CancelledError:
            pass
        except Exception as e:
            print(f"[loco] Receive error: {e}", file=sys.stderr)
            self._connected = False

    async def _handle_push(self, packet: LocoPacket):
        method = packet.method
        handlers = self._push_handlers.get(method, [])
        for handler in handlers:
            try:
                await handler(packet)
            except Exception as e:
                print(f"[loco] Push handler error ({method}): {e}", file=sys.stderr)

    def on_push(self, method: str, handler: Callable):
        self._push_handlers.setdefault(method, []).append(handler)

    async def send_command(self, method: str, body: dict[str, Any] | None = None) -> LocoPacket:
        packet = self.packet_builder.build(method, body)
        return await self._send_packet(packet)

    # ── Connection flow ─────────────────────────────────────────────────

    async def booking(self) -> dict:
        """Phase 1: Get configuration and checkin server."""
        builder = PacketBuilder()
        pkt = builder.build("GETCONF", {
            "MCCMNC": "99999",
            "os": "mac",
            "model": "",
        })
        response = await _loco_oneshot(BOOKING_HOST, BOOKING_PORT, pkt, use_tls=True)
        print(f"[booking] Got config (status={response.body.get('status')})", file=sys.stderr)
        return response.body

    async def checkin(self, checkin_host: str, checkin_port: int) -> dict:
        """Phase 2: Get assigned LOCO chat server."""
        builder = PacketBuilder()
        pkt = builder.build("CHECKIN", {
            "userId": self.credentials.user_id,
            "os": "mac",
            "ntype": 0,
            "appVer": "4.5.0",
            "MCCMNC": "99999",
            "lang": "ko",
            "countryISO": "KR",
            "useSub": True,
        })

        # Try TLS first, then fallback to legacy
        for use_tls, port in [(True, 443), (True, checkin_port), (False, checkin_port)]:
            try:
                response = await asyncio.wait_for(
                    _loco_oneshot(checkin_host, port, pkt, use_tls=use_tls),
                    timeout=10,
                )
                host = response.body.get("host")
                if host:
                    print(f"[checkin] Server: {host}:{response.body.get('port')} (TLS={use_tls}, port={port})", file=sys.stderr)
                    response.body["_use_tls"] = use_tls
                    return response.body
            except Exception as e:
                print(f"[checkin] TLS={use_tls} port={port} failed: {e}", file=sys.stderr)

        raise ConnectionError("All checkin attempts failed")

    async def login(self) -> dict:
        """Phase 3: Authenticate with LOGINLIST."""
        response = await self.send_command("LOGINLIST", {
            "os": "mac",
            "ntype": 0,
            "appVer": "4.5.0",
            "MCCMNC": "99999",
            "prtVer": "1",
            "duuid": self.credentials.device_uuid,
            "oauthToken": self.credentials.oauth_token,
            "lang": "ko",
            "dtype": 2,  # PC
            "revision": 0,
            "chatIds": [],
            "maxIds": [],
            "lastTokenId": 0,
            "lbk": 0,
            "bg": False,
        })

        body = response.body
        self.user_id = body.get("userId", 0)
        self.credentials.user_id = self.user_id
        print(f"[login] Status: {response.status_code}, userId: {self.user_id}", file=sys.stderr)

        if "chatDatas" in body:
            for chat_data in body["chatDatas"]:
                chat_id = chat_data.get("chatId", 0)
                self.chat_rooms[chat_id] = ChatRoom(
                    chat_id=chat_id,
                    type=str(chat_data.get("type", "")),
                    last_log_id=chat_data.get("lastLogId", 0),
                    last_seen_log_id=chat_data.get("lastSeenLogId", 0),
                )

        return body

    async def full_connect(self) -> bool:
        """Execute full connection flow: booking -> checkin -> connect -> login."""
        try:
            # Phase 1: Booking
            config = await self.booking()

            # Extract checkin hosts
            ticket = config.get("ticket", {})
            checkin_hosts = ticket.get("lsl", [])
            if not checkin_hosts:
                print("[error] No checkin hosts in booking response", file=sys.stderr)
                return False

            # Get available ports from wifi config
            wifi = config.get("wifi", {})
            ports = wifi.get("ports", LOCO_PORTS)

            checkin_host = checkin_hosts[0]
            checkin_port = ports[0] if ports else DEFAULT_LOCO_PORT

            # Phase 2: Checkin
            checkin_data = await self.checkin(checkin_host, checkin_port)
            loco_host = checkin_data.get("host")
            loco_port = checkin_data.get("port", DEFAULT_LOCO_PORT)
            use_tls = checkin_data.get("_use_tls", True)

            if not loco_host:
                print("[error] No LOCO host from checkin", file=sys.stderr)
                return False

            # Phase 3: Connect and login
            await self.connect(loco_host, loco_port, use_tls=use_tls)
            login_data = await self.login()

            if self.user_id:
                print(f"[connected] User {self.user_id}, {len(self.chat_rooms)} rooms", file=sys.stderr)
                return True
            else:
                status = login_data.get("status", "unknown")
                print(f"[error] Login failed (status={status})", file=sys.stderr)
                return False

        except Exception as e:
            print(f"[error] Connection failed: {e}", file=sys.stderr)
            import traceback
            traceback.print_exc(file=sys.stderr)
            return False

    # ── High-level API ──────────────────────────────────────────────────

    async def get_chat_list(self) -> list[ChatRoom]:
        response = await self.send_command("LCHATLIST", {
            "chatIds": [],
            "maxIds": [],
            "lastTokenId": 0,
            "lastChatId": 0,
        })
        rooms = []
        for chat_data in response.body.get("chatDatas", []):
            chat_id = chat_data.get("chatId", 0)
            room = ChatRoom(
                chat_id=chat_id,
                type=str(chat_data.get("type", "")),
                last_log_id=chat_data.get("lastLogId", 0),
            )
            self.chat_rooms[chat_id] = room
            rooms.append(room)
        return rooms

    async def get_messages(self, chat_id: int, count: int = 30, from_log_id: int = 0) -> list[ChatMessage]:
        body: dict[str, Any] = {"chatId": chat_id, "count": count}
        if from_log_id:
            body["logId"] = from_log_id

        response = await self.send_command("GETMSGS", body)
        messages = []
        for log in response.body.get("chatLogs", []):
            messages.append(ChatMessage(
                log_id=log.get("logId", 0),
                chat_id=log.get("chatId", chat_id),
                author_id=log.get("authorId", 0),
                author_nickname=log.get("authorNickname", ""),
                message=log.get("message", ""),
                msg_type=log.get("type", 1),
                send_at=log.get("sendAt", 0),
            ))
        return messages

    async def send_message(self, chat_id: int, text: str, msg_type: int = 1) -> dict:
        response = await self.send_command("WRITE", {
            "chatId": chat_id,
            "msg": text,
            "type": msg_type,
            "noSeen": False,
        })
        return response.body

    async def get_members(self, chat_id: int) -> list[dict]:
        response = await self.send_command("GETMEM", {"chatId": chat_id})
        return response.body.get("members", [])

    async def get_chat_info(self, chat_id: int) -> dict:
        response = await self.send_command("CHATINFO", {"chatId": chat_id})
        return response.body

    async def mark_read(self, chat_id: int, log_id: int) -> dict:
        response = await self.send_command("NOTIREAD", {
            "chatId": chat_id,
            "watermark": log_id,
        })
        return response.body

    async def ping(self):
        await self.send_command("PING", {})
