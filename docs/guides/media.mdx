---
title: Media Download
description: Download photos, videos, and files from chat messages.
---

## Download a Specific Message

```bash
# Download attachment from a message
openkakao-rs download <chat_id> <log_id>

# Custom output directory
openkakao-rs download <chat_id> <log_id> -o ./media
```

The `log_id` can be found in message output or JSON export.

## Auto-Download in Watch Mode

Download media automatically as messages arrive:

```bash
openkakao-rs watch --download-media --download-dir ./media
```

## Supported Media Types

| Type | LOCO Code | Extensions |
|------|-----------|------------|
| Photo | 2 | JPEG, PNG |
| Video | 3 | MP4, MOV, WebM |
| Audio | 12 | M4A, AAC |
| GIF/Animated | 14 | GIF |
| File | 26 | Any |

## File Naming

Downloaded files are saved as:

```
{download_dir}/{chat_id}/{log_id}_{original_filename}
```

Filenames are sanitized to prevent path traversal attacks — directory components (`../`), null bytes, and path separators are stripped.

## Security

- Download URLs are validated against a Kakao domain allowlist (`.kakao.com`, `.kakaocdn.net`)
- Credentials are never sent to non-Kakao domains
- Filenames from the server are sanitized before writing to disk
