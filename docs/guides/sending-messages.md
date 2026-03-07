---
title: Sending Messages
description: Send text, photos, videos, and files via LOCO protocol.
---

## Text Messages

```bash
# With confirmation prompt
openkakao-rs send <chat_id> "Hello!"

# Skip confirmation
openkakao-rs send <chat_id> "Hello!" -y
```

::: info
Messages are automatically prefixed with `🤖 [Sent via openkakao]` for traceability. Disable with `--no-prefix`.
:::

```bash
# Without prefix
openkakao-rs --no-prefix send <chat_id> "Hello!" -y
```

## Photos

```bash
openkakao-rs send-photo <chat_id> photo.jpg -y
# or
openkakao-rs send-file <chat_id> photo.jpg -y
```

Supported formats: JPEG, PNG, GIF. The CLI auto-detects the image type from magic bytes and extracts dimensions for the thumbnail metadata.

## Videos

```bash
openkakao-rs send-file <chat_id> video.mp4 -y
```

Supported formats: MP4, MOV, WebM.

## Files

```bash
openkakao-rs send-file <chat_id> document.pdf -y
```

Any file type is supported. The CLI detects media types automatically and falls back to generic file upload.

## How It Works

Media upload uses the LOCO SHIP+POST protocol:

1. **SHIP** — Request an upload slot from the main LOCO server (returns CDN host, port, key)
2. **POST** — Connect to the upload server, send file metadata (size, type, dimensions)
3. **Data** — Send the encrypted file data through the AES-GCM channel
4. **WRITE** — Send the chat message with the attachment URL

```mermaid
sequenceDiagram
    participant CLI as openkakao-rs
    participant LOCO as LOCO Server
    participant CDN as Upload Server (Kage CDN)

    CLI->>LOCO: SHIP (chatId, type, size)
    LOCO-->>CLI: vhost, port, key
    CLI->>CDN: RSA Handshake + POST (metadata)
    CLI->>CDN: Encrypted file data
    CDN-->>CLI: Upload complete (status=0)
    CLI->>LOCO: WRITE (msg with attachment URL)
```

## Open Chat Safety

Sending to open chats (public chat rooms) has a higher risk of account restrictions. The CLI blocks this by default:

```bash
# This will be blocked
openkakao-rs send <open_chat_id> "message" -y
# Error: Open chat send blocked (use --force to override)

# Override with --force
openkakao-rs send <open_chat_id> "message" -y --force
```
