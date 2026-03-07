---
title: send / send-file / send-photo
description: Send text messages and media files.
---

## send

Send a text message via LOCO WRITE command.

```bash
openkakao-rs send <chat_id> "message" [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-y, --yes` | Skip confirmation prompt |
| `--force` | Allow sending to open chats |

Messages are prefixed with `🤖 [Sent via openkakao]` by default. Use the global `--no-prefix` flag to disable:

```bash
openkakao-rs --no-prefix send <chat_id> "raw message" -y
```

---

## send-file

Send a file (photo, video, or document) via LOCO SHIP+POST.

```bash
openkakao-rs send-file <chat_id> <file> [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `-y, --yes` | Skip confirmation prompt |
| `--force` | Allow sending to open chats |

The media type is auto-detected from file magic bytes:

| Magic Bytes | Type | LOCO Code |
|-------------|------|-----------|
| `FF D8` | JPEG photo | 2 |
| `89 50 4E 47` | PNG photo | 2 |
| `GIF8` | GIF animation | 14 |
| `ftyp` (offset 4) | MP4/MOV video | 3 |
| `1A 45 DF A3` | WebM video | 3 |
| Other | Generic file | 26 |

---

## send-photo

Alias for `send-file`. Identical behavior.

```bash
openkakao-rs send-photo <chat_id> photo.jpg -y
```
