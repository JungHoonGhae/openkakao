---
title: watch
description: Real-time message monitoring with auto-reconnect.
---

## Usage

```bash
openkakao-rs watch [OPTIONS]
```

## Options

| Flag | Description | Default |
|------|-------------|---------|
| `--chat-id <ID>` | Filter by specific chat | All chats |
| `--raw` | Show raw BSON body | false |
| `--read-receipt` | Send NOTIREAD for incoming messages | false |
| `--max-reconnect <N>` | Max reconnect attempts (0 = disabled) | 5 |
| `--download-media` | Auto-download media attachments | false |
| `--download-dir <DIR>` | Directory for downloads | `downloads` |

## Examples

```bash
# Watch all chats
openkakao-rs watch

# Watch specific chat with read receipts
openkakao-rs watch --chat-id 382416827148557 --read-receipt

# Auto-download media
openkakao-rs watch --download-media --download-dir ./media

# Maximum reliability
openkakao-rs watch --max-reconnect 20 --read-receipt --download-media
```

## Reconnect Behavior

| Scenario | Action |
|----------|--------|
| Network error | Exponential backoff (1s, 2s, 4s, ... max 32s) |
| Auth error (-950) | Attempt token refresh, then reconnect |
| CHANGESVR received | Immediate reconnect to new server |
| PING timeout | Treated as disconnect |
| Max retries exceeded | Exit |

## Output Format

```
[Chat Title] Author: message text
```

Media messages show the attachment type:

```
[Chat Title] Author: [Photo] (attached)
[Chat Title] Author: [Video] video.mp4
```
