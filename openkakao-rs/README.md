# openkakao-rs

Unofficial KakaoTalk CLI client built by reverse engineering the macOS KakaoTalk desktop app. Provides both REST API and LOCO protocol access for read-only operations and message sending.

## About

This project reverse engineers KakaoTalk's proprietary LOCO binary protocol (TCP + BSON) and REST API to build a fully functional CLI client. Key achievements:

- **LOCO Protocol**: Full implementation of booking → checkin → login flow with RSA-2048/AES-128 encryption
- **X-VC Authentication**: Cracked the Mac X-VC header algorithm via static binary analysis of the KakaoTalk Mach-O binary
- **Message Send/Receive**: Send messages via LOCO WRITE, watch real-time incoming messages via persistent connection
- **Full History Access**: Read complete chat history via LOCO SYNCMSG, bypassing Pilsner REST API cache limitations

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
| `send <chat_id> <message>` | Send a message via LOCO WRITE |
| `watch [--chat-id ID] [--raw]` | Watch real-time incoming messages |
| `loco-chats [--all]` | List all chat rooms (no cache limit) |
| `loco-read <chat_id> [-n count] [--all]` | Read message history via SYNCMSG |
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

1. **Booking** (TLS) → `booking-loco.kakao.com:443` → GETCONF → get checkin hosts/ports
2. **Checkin** (RSA+AES) → CHECKIN → get assigned LOCO chat server IP
3. **Login** → LOGINLIST with fresh access_token → authenticate + receive chat list
4. **Commands** → SYNCMSG, WRITE, CHATONROOM, LCHATLIST, etc.

### Connection Flow

```
booking-loco.kakao.com:443 (TLS)
  └─ GETCONF → checkin hosts, ports
       └─ ticket-loco.kakao.com (RSA+AES handshake)
            └─ CHECKIN → LOCO server IP:port
                 └─ LOCO server (RSA+AES handshake)
                      └─ LOGINLIST → status=0, chatDatas[]
                           └─ ready for commands
```

### Encryption

- Handshake: RSA-2048 (e=3, OAEP/SHA-1) to exchange AES key
- Data: AES-128-CFB with per-frame random IV
- Packet: 22-byte LE header (packet_id, status, method, body_type, body_length) + BSON body

### X-VC Authentication

The X-VC header is required for `login.json` to obtain fresh access tokens. Algorithm cracked via static analysis of the KakaoTalk Mach-O binary:

```
SHA-512("YLLAS|{loginId}|{deviceUUID}|GRAEB|{userAgent}")[0:16]
```

- Seeds: `YLLAS` (reversed "SALLY") and `GRAEB` (reversed "BEARG")
- User-Agent must be the short format: `KT/{version} Mc/{osVersion} ko`
- The same User-Agent is used in both the hash input and the HTTP request header

### LOCO Commands

| Command | Purpose | Key Parameters |
|---------|---------|----------------|
| GETCONF | Get server configuration | `MCCMNC`, `os` |
| CHECKIN | Get LOCO server assignment | `userId`, `os`, `appVer` |
| LOGINLIST | Authenticate + get chat list | `oauthToken`, `duuid`, `dtype=2` |
| LCHATLIST | List chat rooms (paginated) | `chatIds`, `maxIds`, `lastTokenId` |
| CHATONROOM | Get chat room details | `chatId` → members, lastLogId |
| SYNCMSG | Read message history | `chatId`, `cur`, `cnt`, `max` |
| WRITE | Send a message | `chatId`, `msg`, `msgId`, `type=1` |
| PING | Keepalive | (empty body) |

### SYNCMSG — Message History

SYNCMSG is the working command for reading message history on Mac (dtype=2). GETMSGS returns -300 for all chats on this device type.

**Required parameters:**
- `chatId` (Int64) — target chat room
- `cur` (Int64) — cursor position (0 = start from oldest available)
- `cnt` (Int32) — messages per page (max ~100, use 50 for reliability)
- `max` (Int64) — upper bound logId (required, use lastLogId from CHATONROOM)

**Pagination:**
```
1. CHATONROOM {chatId} → get lastLogId (field "l") and member names (field "m")
2. SYNCMSG {chatId, cur=0, cnt=50, max=lastLogId} → first batch
3. If isOK=false → advance cur to max(logId) in batch, repeat
4. If isOK=true → done, all messages fetched
```

**Response fields** (chatLogs array):
- `logId`, `chatId`, `authorId`, `type`, `sendAt` (Int32 unix timestamp), `message`, `attachment`

**Caveats:**
- `cnt` > ~100 causes server to return null chatLogs — use cnt=50
- `max` parameter is mandatory — without it, returns -203
- Server retains limited message history per chat room (varies by activity)

### LOCO BSON Field Abbreviations

LOGINLIST/LCHATLIST chatDatas use abbreviated field names:

| Field | Meaning |
|-------|---------|
| `c` | chatId |
| `t` | type |
| `a` | activeMembersCount |
| `s` | lastLogId |
| `ll` | lastSeenLogId |
| `o` | timestamp |
| `i` | member IDs (array) |
| `k` | member names (string array) |
| `l` | chatLogs |

### Token Flow

The LOCO LOGINLIST requires a fresh 65-char access_token (not the 138-char combined format from Cache.db):

```
1. Extract credentials from Cache.db (email, password, device_uuid)
2. Generate X-VC: SHA-512("YLLAS|{email}|{uuid}|GRAEB|{shortUA}")[0:16]
3. POST login.json with X-VC header → fresh access_token (65 chars)
4. LOGINLIST with oauthToken=access_token → status=0
```

Cache.db token (138 chars) = `{access_token}-{device_uuid}` — the combined format works for REST API but LOCO needs only the 65-char access_token portion.

### Error Codes

| Code | Context | Meaning |
|------|---------|---------|
| 0 | All | Success |
| -203 | SYNCMSG | Missing required parameter (e.g., `max`) |
| -300 | GETMSGS | Not supported on Mac dtype=2 |
| -500 | login.json | Invalid X-VC header |
| -910 | login.json | Invalid credentials or `auto_login` param |
| -950 | LOGINLIST | Token expired (need fresh token via login.json) |

## License

MIT
