"""LOCO packet encoding/decoding using BSON."""

import struct
from dataclasses import dataclass, field
from typing import Any

import bson


@dataclass
class LocoPacket:
    """A single LOCO protocol packet.

    Header layout (22 bytes, little-endian):
        Offset  Size  Field
        0       4     packet_id (u32)
        4       2     status_code (i16)
        6       11    method (ASCII, null-padded)
        17      1     body_type (0 = BSON)
        18      4     body_length (u32)
    Followed by BSON-encoded body.
    """

    packet_id: int
    status_code: int
    method: str
    body_type: int
    body: dict[str, Any] = field(default_factory=dict)

    HEADER_SIZE = 22

    def encode(self) -> bytes:
        body_bytes = bson.BSON.encode(self.body)
        method_bytes = self.method.encode("ascii").ljust(11, b"\x00")[:11]
        header = struct.pack(
            "<IhB",
            self.packet_id,
            self.status_code,
            0,  # padding byte within the 11-byte method field handled below
        )
        # Repack properly: id(4) + status(2) + method(11) + body_type(1) + body_len(4)
        header = struct.pack("<Ih", self.packet_id, self.status_code)
        header += method_bytes
        header += struct.pack("<BI", self.body_type, len(body_bytes))
        return header + body_bytes

    @classmethod
    def decode(cls, data: bytes) -> "LocoPacket":
        if len(data) < cls.HEADER_SIZE:
            raise ValueError(f"Data too short: {len(data)} < {cls.HEADER_SIZE}")

        packet_id, status_code = struct.unpack_from("<Ih", data, 0)
        method = data[6:17].rstrip(b"\x00").decode("ascii")
        body_type = struct.unpack_from("<B", data, 17)[0]
        body_length = struct.unpack_from("<I", data, 18)[0]
        body_bytes = data[cls.HEADER_SIZE : cls.HEADER_SIZE + body_length]

        body = {}
        if body_bytes:
            body = bson.BSON(body_bytes).decode()

        return cls(
            packet_id=packet_id,
            status_code=status_code,
            method=method,
            body_type=body_type,
            body=body,
        )

    @classmethod
    def decode_header(cls, data: bytes) -> tuple[int, int, str, int, int]:
        """Decode just the header, returning (packet_id, status, method, body_type, body_length)."""
        packet_id, status_code = struct.unpack_from("<Ih", data, 0)
        method = data[6:17].rstrip(b"\x00").decode("ascii")
        body_type = struct.unpack_from("<B", data, 17)[0]
        body_length = struct.unpack_from("<I", data, 18)[0]
        return packet_id, status_code, method, body_type, body_length


class PacketBuilder:
    """Builds LOCO request packets with auto-incrementing IDs."""

    def __init__(self):
        self._next_id = 1

    def build(self, method: str, body: dict[str, Any] | None = None) -> LocoPacket:
        packet = LocoPacket(
            packet_id=self._next_id,
            status_code=0,
            method=method,
            body_type=0,
            body=body or {},
        )
        self._next_id += 1
        return packet
