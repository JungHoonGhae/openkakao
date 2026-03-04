# openkakao-rs

Unofficial KakaoTalk CLI client built by reverse engineering the macOS KakaoTalk desktop app. Provides both REST API and LOCO protocol access for read-only operations and message sending.

## About

This project reverse engineers KakaoTalk's proprietary LOCO binary protocol (TCP + BSON) and REST API to build a fully functional CLI client. Key achievements:

- **LOCO Protocol**: Full implementation of booking → checkin → login flow with RSA-2048/AES-128 encryption
- **X-VC Authentication**: Cracked the Mac X-VC header algorithm via static binary analysis of the KakaoTalk Mach-O binary
- **Real-time Messaging**: Watch incoming messages via persistent LOCO connection with PING keepalive
- **Full History Access**: Read complete chat history via LOCO protocol, bypassing Pilsner REST API cache limitations

## Build

```bash
cargo build --release
```

## Setup

```bash
# Extract and save credentials from running KakaoTalk app
cargo run -- login --save

# Verify token
cargo run -- auth
```

## Commands

### REST API (read-only, uses cached token)

| Command | Description |
|---------|-------------|
| `auth` | Check token validity |
| `login --save` | Extract credentials from KakaoTalk's Cache.db |
| `me` | Show your profile |
| `friends [-f] [-s query]` | List friends (with favorites/search filter) |
| `settings` | Show account settings |
| `chats` | List chat rooms (Pilsner REST API) |
| `messages <chat_id> [-n count]` | Read messages (Pilsner, limited cache) |
| `members <chat_id>` | List chat room members (Pilsner) |

### LOCO Protocol (full access, real-time)

| Command | Description |
|---------|-------------|
| `loco-test` | Test full LOCO connection flow |
| `send <chat_id> <message>` | Send a message via LOCO |
| `watch [--chat-id ID] [--raw]` | Watch real-time incoming messages |
| `loco-chats [--all]` | List all chat rooms (no cache limit) |
| `loco-read <chat_id> [-n count] [--all]` | Read messages with full history |
| `loco-members <chat_id>` | List chat room members |
| `loco-chatinfo <chat_id>` | Show raw chat room info |

### Token Management

| Command | Description |
|---------|-------------|
| `relogin [--fresh-xvc]` | Refresh token via login.json |
| `renew` | Attempt token renewal via refresh_token |
| `watch-token [--interval N]` | Poll Cache.db for fresh tokens |

## Architecture

```
src/
├── main.rs          # CLI commands and LOCO command implementations
├── rest.rs          # REST API client + X-VC generation
├── auth.rs          # Token/credential extraction from macOS Cache.db
├── model.rs         # Data models (credentials, profiles, messages)
├── error.rs         # Error types
└── loco/
    ├── client.rs    # LOCO protocol client (booking, checkin, login, commands)
    ├── crypto.rs    # RSA-2048 OAEP + AES-128-CFB encryption
    └── packet.rs    # LOCO packet codec (22-byte header + BSON body)
```

## LOCO Protocol

The LOCO protocol is KakaoTalk's proprietary binary messaging protocol:

1. **Booking** (TLS) → `booking-loco.kakao.com:443` → Get checkin server info
2. **Checkin** (RSA+AES) → Get assigned LOCO chat server IP
3. **Login** (LOGINLIST) → Authenticate with fresh access_token
4. **Commands** → LCHATLIST, GETMSGS, WRITE, etc.

### Encryption

- Handshake: RSA-2048 (e=3, OAEP/SHA-1) to exchange AES key
- Data: AES-128-CFB with per-frame random IV
- Packet: 22-byte LE header (packet_id, status, method, body_type, body_length) + BSON body

## License

MIT
