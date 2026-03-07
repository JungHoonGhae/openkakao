---
title: Packet Format
description: 22-byte LOCO header + BSON body.
---

## Header

Every LOCO packet starts with a 22-byte little-endian header:

```
Offset  Size  Type    Field
0       4     u32     packet_id
4       2     i16     status_code
6       11    ASCII   method (null-padded)
17      1     u8      body_type
18      4     u32     body_length
```

Total header: **22 bytes**.

## Body

The body is BSON-encoded (Binary JSON). The `body_length` field specifies its exact size.

## Example

A GETCONF request:

```
Header (22 bytes):
  packet_id:   01 00 00 00      (1)
  status_code: 00 00            (0)
  method:      47 45 54 43 4F 4E 46 00 00 00 00  ("GETCONF\0\0\0\0")
  body_type:   00               (0)
  body_length: 1A 00 00 00      (26)

Body (26 bytes):
  BSON document: {"os": "mac", "model": "mac"}
```

## Method Names

Method names are ASCII strings, max 11 characters, null-padded. Common methods:

| Method | Direction | Description |
|--------|-----------|-------------|
| `GETCONF` | Request | Get server config (booking) |
| `CHECKIN` | Request | Get LOCO server assignment |
| `LOGINLIST` | Request | Authenticate |
| `WRITE` | Request | Send message |
| `MSG` | Push | Incoming message |
| `SYNCMSG` | Request | Fetch message history |
| `LCHATLIST` | Request | List chat rooms |
| `CHATONROOM` | Request | Chat room details |
| `NOTIREAD` | Request | Send read receipt |
| `GETMEM` | Request | Get chat members |
| `SHIP` | Request | Request media upload slot |
| `PING` | Request | Keep-alive |
| `CHANGESVR` | Push | Server migration |
| `BLSYNC` | Push | Block list sync |

## Packet ID

Sequential counter starting from 1. Responses carry the same `packet_id` as the request. Push messages (MSG, CHANGESVR) use server-assigned IDs.

## Size Limits

- Maximum body size: **100 MB** (enforced client-side)
- Maximum frame size: **100 MB** (for encrypted frames)
