---
title: LOCO Commands
description: Known LOCO protocol commands and their BSON fields.
---

## Messaging

### WRITE — Send Message

```json
{
  "chatId": 900000000000001,
  "msg": "Hello!",
  "type": 1,
  "noSeen": false
}
```

Message types: `1` = text, `2` = photo, `3` = video, `12` = audio, `14` = GIF, `26` = file.

### MSG — Incoming Message (Push)

Pushed by the server when a message is received:

```json
{
  "chatId": 900000000000001,
  "logId": 9000000000000000001,
  "authorId": 100000001,
  "message": "Hello!",
  "type": 1,
  "sendAt": 1741355188
}
```

### SYNCMSG — Fetch History

```json
{
  "chatId": 900000000000001,
  "cur": 0,
  "cnt": 50,
  "max": 9000000000000000001
}
```

Returns a `chatLogs` array of messages. Paginate by updating `cur` to the smallest `logId` from the previous response.

### NOTIREAD — Read Receipt

```json
{
  "chatId": 900000000000001,
  "watermark": 9000000000000000001
}
```

Marks messages up to the given `logId` as read.

## Chat Rooms

### LCHATLIST — List Chats

```json
{
  "chatIds": [],
  "maxIds": [],
  "lastTokenId": 0,
  "lastChatId": 0
}
```

### CHATONROOM — Chat Info

```json
{
  "chatId": 900000000000001
}
```

### GETMEM — Get Members

```json
{
  "chatId": 900000000000001
}
```

## Media

### SHIP — Request Upload Slot

```json
{
  "c": 900000000000001,
  "t": 2,
  "s": 145238
}
```

Fields: `c` = chatId, `t` = message type, `s` = file size.

Returns: `vh` (vhost), `p` (port), `k` (upload key).

### POST — Upload Metadata (on upload server)

```json
{
  "u": 100000001,
  "k": "<upload_key>",
  "t": 2,
  "s": 145238,
  "c": 900000000000001,
  "w": 1920,
  "h": 1080,
  "ns": false,
  "os": "mac"
}
```

After POST, the encrypted file data is sent through the AES-GCM channel.

## System

### PING — Keep-Alive

Empty body. Server responds with PING.

### CHANGESVR — Server Migration (Push)

Pushed when the client should reconnect to a different server. Triggers automatic reconnection.
