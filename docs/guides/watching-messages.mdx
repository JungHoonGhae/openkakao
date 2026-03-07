---
title: Watching Messages
description: Monitor real-time messages with auto-reconnect and read receipts.
---

## Basic Watch

```bash
# Watch all chats
openkakao-rs watch

# Filter by chat
openkakao-rs watch --chat-id <chat_id>

# Show raw BSON body
openkakao-rs watch --raw
```

## Read Receipts

Send NOTIREAD (read receipts) for incoming messages, so the sender sees their message was read:

```bash
openkakao-rs watch --read-receipt
```

## Auto-Reconnect

The watch command automatically reconnects on network failures with exponential backoff:

```bash
# Default: 5 reconnect attempts
openkakao-rs watch

# Custom max attempts
openkakao-rs watch --max-reconnect 10

# Disable reconnect (exit on first error)
openkakao-rs watch --max-reconnect 0
```

Reconnect behavior:
- **Auth errors** (-950, -999): Immediate exit (reconnect won't help)
- **Network errors**: Exponential backoff (1s, 2s, 4s, 8s, ... max 32s)
- **CHANGESVR**: Automatic reconnect to new server
- **PING failure**: Treated as disconnect, triggers reconnect

## Auto-Download Media

Automatically download media attachments (photos, videos, audio, files) as they arrive:

```bash
openkakao-rs watch --download-media

# Custom download directory
openkakao-rs watch --download-media --download-dir ./media
```

Files are saved as `{download_dir}/{chat_id}/{log_id}_{filename}`.

## Combining Options

```bash
openkakao-rs watch \
  --chat-id <chat_id> \
  --read-receipt \
  --download-media \
  --download-dir ./media \
  --max-reconnect 10
```

## Output Format

Each message is printed as:

```
[ChatTitle] Sender: message content
```

For JSON processing, combine with `--json`:

```bash
openkakao-rs watch --raw 2>/dev/null | jq '.'
```
