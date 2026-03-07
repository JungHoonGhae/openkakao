# OpenKakao-rs Improvement Plan

Based on analysis of [KakaoTalk is making me LOCO](https://jusung.dev/posts/kakao-talk-is-making-me-local/) by Jusung,
a full codebase audit, and review of public LOCO protocol implementations.

---

## 1. Blog Post Key Takeaways

| Finding | Detail | Impact on openkakao-rs |
|---------|--------|----------------------|
| **RSA key rotated** | `0xF3188...` (node-kakao era) -> `0xA3B076...` (current) | Our key matches current (`A3B076...`) - OK |
| **Handshake type 12 -> 16** | `key_type` field in handshake packet | We use 16 - OK. KiwiTalk/loco-protocol-rs uses **15** - differs! |
| **ticket.lsl changed** | Was string, now `string[]` (array) | Our code handles array - OK |
| **Port field moved** | `ticket.lslp` -> `wifi.ports[0]` | Our code reads `wifi.ports` - OK |
| **Status field moved** | Was in packet header, now in BSON body | Our code checks both - OK |
| **Mac secondary device auth** | Login without logging out phone; uses `/mac/account/login.json` with X-VC | We have `generate_xvc()` and `login_with_xvc()` - implemented |
| **-999 "Upgrade required"** | Version string must match recent KakaoTalk | We use version from Cache.db user-agent - OK if KakaoTalk is updated |
| **Ban risk** | Matrix bridge showed ban warnings; unclear trigger | No mitigation currently |

### Critical difference: `key_type` 15 vs 16

- **KiwiTalk/loco-protocol-rs** (`storycraft`): Uses `key_type: 15` in handshake
- **Our implementation**: Uses `key_type: 16` (from Mach-O binary analysis)
- **Blog post**: Mentions handshake changed from 12 to 16
- **Hypothesis**: `key_type` may be server-version-dependent; both 15 and 16 might work depending on server

---

## 2. Why LOCO Login Fails (-950)

The `-950` error occurs at LOGINLIST, *after* successful BOOKING and CHECKIN. Our investigation shows:

1. **Token is valid for REST API** (status=0 on `more_settings.json`)
2. **Same token fails on LOCO LOGINLIST** with status -950
3. **Possible causes** (ordered by likelihood):
   a. **Token type mismatch**: LOCO may require a *different* token from REST. The `renew_token.json` endpoint may issue a LOCO-specific access_token, but we can't call it (returns -400, missing params).
   b. **Missing `rp` field**: KiwiTalk sends a 6-byte `rp` field (`[0x??, 0x??, 0xFF, 0xFF, 0x??, 0x??]`). We send `null`. This could be a required authentication nonce.
   c. **Protocol version string**: KiwiTalk uses `"1.0"` for PC clients, we use `"1"`. Different interpretations may cause rejection.
   d. **`pcst` (PC status) field**: KiwiTalk sends `pcst` for PC login. We omit it entirely.
   e. **`useSub` in CHECKIN**: We send `"useSub": true`, which signals secondary-device. The LOCO server may then expect Mac-specific auth fields.

### CRITICAL FINDING: AES-GCM Migration (from loco-wrapper, Dec 2025)

[NetRiceCake/loco-wrapper](https://github.com/NetRiceCake/loco-wrapper) (Java, last commit 2025-12-10, **confirmed working** with KakaoTalk 25.9.2) reveals the LOCO encryption has changed:

| Field | Old (KiwiTalk/node-kakao) | New (loco-wrapper) | **Ours** |
|-------|--------------------------|---------------------|----------|
| `key_type` | 15 | **16** | 16 (OK) |
| `encrypt_type` | 2 (AES-128-CFB) | **3 (AES-128-GCM)** | **2 (WRONG!)** |
| AES mode | CFB-128, 16-byte IV | **GCM, 12-byte nonce** | CFB (WRONG!) |
| Secure frame | `[size(4)][iv(16)][ciphertext]` | `[size(4)][nonce(12)+ciphertext+tag]` | Old format |
| X-VC seeds | `YLLAS`, `GRAEB` | `BARD`, `DANTE`, `SIAN` | `YLLAS`, `GRAEB` |

**This is almost certainly the root cause of -950.** We correctly identify as `key_type=16` (new), but still encrypt with AES-CFB (`encrypt_type=2`). The server likely decodes the handshake, sees type 16, expects GCM, but receives CFB-encrypted data — resulting in garbage and a session rejection.

### Recommended next steps (in priority order)
1. **Switch from AES-128-CFB to AES-128-GCM** (`encrypt_type=3`, 12-byte nonce)
2. Try `prtVer: "1.0"` instead of `"1"`
3. Add `pcst: 1` field to LOGINLIST
4. Generate proper `rp` bytes (try `[0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00]`)
5. Investigate whether Mac client uses different X-VC seeds from Android
6. Investigate whether `oauth2_token.json` returns a separate LOCO token

---

## 3. Reference Implementations

| Project | Language | Status | Key Techniques | Link |
|---------|----------|--------|---------------|------|
| **loco-wrapper** | Java (Netty) | **Active (Dec 2025)** | **Working!** `key_type=16`, `encrypt_type=3` (AES-GCM), new X-VC seeds | [github.com/NetRiceCake/loco-wrapper](https://github.com/NetRiceCake/loco-wrapper) |
| **KiwiTalk** | Rust+TS (Tauri) | Archived (2023) | Full LOCO client, `key_type=15`, `prtVer="1.0"`, `rp` field, `pcst` | [github.com/KiwiTalk/KiwiTalk](https://github.com/KiwiTalk/KiwiTalk) |
| **loco-protocol-rs** | Rust | Archived (2023) | IO-free secure layer, clean handshake impl | [github.com/storycraft/loco-protocol-rs](https://github.com/storycraft/loco-protocol-rs) |
| **node-kakao** | TypeScript | Unmaintained (4yr) | Original LOCO RE work, old RSA key | [github.com/storycraft/node-kakao](https://github.com/storycraft/node-kakao) |
| **kakaotalk_analysis** | Python (mitmproxy) | Active (2024) | MITM scripts, CFB analysis, secret chat | [github.com/stulle123/kakaotalk_analysis](https://github.com/stulle123/kakaotalk_analysis) |
| **matrix-appservice-kakaotalk** | Python+JS | Semi-maintained | Matrix bridge, ban warnings | [src.miscworks.net/.../matrix-appservice-kakaotalk](https://src.miscworks.net/fair/matrix-appservice-kakaotalk.git) |
| **pykakao** | Python | Unmaintained | Simple LOCO/HTTP wrapper | [github.com/hallazzang/pykakao](https://github.com/hallazzang/pykakao) |

### Specific field differences (KiwiTalk vs ours)

```
KiwiTalk LOGINLIST:                     Ours:
  os: "win32"                             os: "mac"
  prtVer: "1.0"                           prtVer: "1"       <-- DIFFERS
  dtype: 2                                dtype: 2
  pcst: Some(1)                           (missing)          <-- MISSING
  rp: [6 bytes]                           rp: null           <-- MISSING
  lbk: 0                                  lbk: 0
  revision: None                          revision: 0
```

---

## 4. Proposed Hardening Features

### 4.1 `doctor` Command (THIS PR)

A diagnostic command that checks environment health without making any changes:

```
openkakao-rs doctor
```

Output:
- KakaoTalk.app installed version (from Info.plist)
- KakaoTalk process running status
- Cache.db existence and freshness
- Token validity (REST API check)
- LOCO booking connectivity (GETCONF)
- LOCO checkin connectivity (CHECKIN)
- Credential file status
- Protocol constants (RSA key fingerprint, handshake type, etc.)

### 4.2 Protocol Version Management (FUTURE)

Make LOGINLIST fields configurable/updatable without recompiling:
- `prtVer` ("1" vs "1.0")
- `pcst` field
- `rp` bytes
- App version override

### 4.3 Safer Auth: Mac Secondary Device Flow (FUTURE)

- Detect if user is logged in on phone before attempting LOCO
- Warn about single-device logout risk
- Implement proper token renewal chain

### 4.4 Rate Limiting and Safety Warnings (FUTURE)

- Add configurable rate limits to LOCO commands
- Display ban risk warning on first use
- Track request frequency per session
- Implement exponential backoff on errors

### 4.5 Improved Error Reporting (THIS PR)

- Structured error codes with explanations
- Actionable hints for common failures (-950, TLS EOF, timeout)
- `--verbose` flag for detailed protocol tracing

### 4.6 Full Chat History Access (THIS PR)

#### How message reading works

| Method | Command | Direction | Max Range | Limitation |
|--------|---------|-----------|-----------|------------|
| **REST** (`read`) | `GET /messaging/chats/{id}/messages` | Backward from cursor | Recent only | Pilsner proxy only caches chats recently opened in KakaoTalk Mac app. Most chats return empty. |
| **LOCO** (`loco-read`) | `SYNCMSG {chatId, cur, cnt, max}` | Forward: `cur` -> `max` | **Full history** | Requires working LOCO connection (currently blocked by -950/AES-GCM). |

#### SYNCMSG pagination details

SYNCMSG scans **forward** from `cur` (exclusive) to `max` (inclusive), returning up to `cnt` messages per batch.
- `cur=0, max=lastLogId` → gets oldest messages first, paginating forward
- `isOK=false` in response → more messages available, continue with `cur=max_log_in_batch`
- `isOK=true` → reached the end, no more messages

#### What blocks full history

1. **LOCO -950 (AES-GCM migration)**: Primary blocker. Fix `encrypt_type` to unblock.
2. **Ban risk**: Aggressive SYNCMSG requests may trigger Kakao's abuse detection. Mitigated by:
   - `--delay-ms 100` (default) between batches
   - 50 messages per batch (conservative)
3. **Server-side limits**: Unknown. KakaoTalk app itself fetches full history on device migration, so the server does support it.
4. **REST API limits**: Pilsner only serves recently cached chats. Not suitable for full history.

#### CLI options implemented (THIS PR)

```
# REST (read) - limited to pilsner cache
openkakao-rs read <chat_id> --all              # Fetch all cached messages
openkakao-rs read <chat_id> --cursor <logId>   # Resume from logId
openkakao-rs read <chat_id> --since 2025-01-01 # Filter by date
openkakao-rs read <chat_id> -n 50              # Last N messages

# LOCO (loco-read) - full history when LOCO works
openkakao-rs loco-read <chat_id> --all                     # Full history
openkakao-rs loco-read <chat_id> --all --cursor <logId>    # Resume from logId
openkakao-rs loco-read <chat_id> --all --since 2024-06-01  # Filter by date
openkakao-rs loco-read <chat_id> --all --delay-ms 200      # Slower rate limit
openkakao-rs loco-read <chat_id> -n 100                    # Last N messages
```

#### Resumable fetch strategy

On disconnect or error during `--all`, the CLI prints:
```
[loco-read] Connection lost: ...
[loco-read] Resume with: openkakao-rs loco-read <chat_id> --all --cursor <last_logId>
```
This allows resuming from the last successful batch without re-fetching.

#### Max-reach strategy (when full history is blocked)

If LOCO remains broken, the fallback hierarchy is:
1. **REST `read --all`**: Gets whatever pilsner has cached (usually recent few hundred messages)
2. **Export from KakaoTalk app**: KakaoTalk Mac has a "내보내기" (export) feature that dumps chat as txt
3. **SQLCipher DB**: `~/Library/Application Support/com.kakao.KakaoTalkMac/chat_data/*.db` — encrypted with SQLCipher, key derivation unknown
4. **MITM proxy**: Use mitmproxy to intercept LOCO traffic from the real KakaoTalk app (see kakaotalk_analysis repo)

---

## 5. Implementation Plan (This PR)

### Phase 1: `doctor` command
- [x] Add `Doctor` subcommand to CLI
- [x] Check KakaoTalk.app version from `/Applications/KakaoTalk.app/Contents/Info.plist`
- [x] Check KakaoTalk process status via `pgrep`
- [x] Check Cache.db existence and modification time
- [x] Check saved credentials file
- [x] Verify token via REST API
- [x] Test LOCO booking (GETCONF) connectivity
- [x] Display protocol constants for debugging

### Phase 2: Improved LOGINLIST fields (experimental)
- [x] Add `pcst: 1` to LOGINLIST
- [x] Add proper `rp` bytes (6-byte BSON binary)
- [ ] Test with `--experimental-login` flag

### Phase 3: Error reporting improvements
- [x] Add actionable messages for -950 errors
- [x] Add hints for TLS handshake failures
- [x] Print protocol version info on failure

### Phase 4: Full history access
- [x] Add `--cursor` option to `read` and `loco-read`
- [x] Add `--since YYYY-MM-DD` date filter to both commands
- [x] Add `--delay-ms` rate limiting to `loco-read`
- [x] Add batch progress reporting during `--all`
- [x] Print resume cursor on disconnect for `loco-read --all`
- [x] Implement AES-128-GCM to unblock LOCO (prerequisite for full history)

### Phase 5: Connection resilience
- [x] Add exponential backoff retry to `full_connect`
- [ ] Add token expiry detection + auto-refresh
- [ ] Add reconnect logic for long-running SYNCMSG operations

---

## References

- Blog: [KakaoTalk is making me LOCO](https://jusung.dev/posts/kakao-talk-is-making-me-local/)
- Security analysis: [stulle123 - Not so Secret](https://stulle123.github.io/posts/kakaotalk/secret-chat/)
- KiwiTalk login.rs: LOGINLIST field reference with `rp`, `pcst`, `prtVer`
- loco-protocol-rs secure/client.rs: Handshake with `key_type=15`
