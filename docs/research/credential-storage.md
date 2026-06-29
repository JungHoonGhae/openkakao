# Credential storage on recent KakaoTalk macOS builds

Status: research note (internal). Last updated for KakaoTalk **26.4.1** on macOS 26.x.
Related issue: [#15](https://github.com/JungHoonGhae/openkakao-cli/issues/15).

## Why this exists

`login --save` historically read the bearer token out of the app's `NSURLCache`
(`Cache.db`). On recent KakaoTalk builds that path is empty: the cache still has
hundreds of rows, but **zero rows carry an `Authorization` header** — they are all
CDN image responses (`kakaocdn.net`, `daumcdn.net`). Authenticated REST responses
are no longer written to `NSURLCache`. This is structural, not a permissions or
"app not running" problem, so the cache-scrape approach cannot be repaired.

This note records where the credentials actually live and what it would take to
recover them, so the work doesn't have to be re-derived from scratch.

## Where the token lives now

`~/Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Preferences/com.kakao.KakaoTalkMac.plist`
holds two obfuscated keys whose values are **96-byte AES ciphertext** (base64, 128 chars):

- `Dfpr93S FDS zXCV`
- `MtQcL0x822tb`

Both decode to 96 bytes of high-entropy data (no plaintext, no `bplist`/SQLite magic).
A separate `Application Support/<hex>` file (~3 MB) is likewise self-encrypted.

## Binary analysis (go/no-go for decrypting the plist values)

Binary: `/Applications/KakaoTalk.app/Contents/MacOS/KakaoTalk` (universal, 69 MB).

Findings — all from **static, read-only** inspection (`strings`, `otool`, `lipo`,
`radare2`); no LOCO/REST traffic, no account risk:

- **Not FairPlay/DRM encrypted** (no `LC_ENCRYPTION_INFO`) → disassemblable.
- The three obfuscated key names (`Dfpr93S`, `MtQcL0x822tb`, `FDS zXCV`) are present
  as static strings in the binary → they are fixed keys, not per-run random.
- **No Secure Enclave / hardware-key indicators** in the token path
  (`kSecAttrTokenIDSecureEnclave`, `LAContext`, `SecKeyCreateRandomKey` absent).
- Crypto is **CommonCrypto** (`CCCryptorCreateWithMode`, `CCCryptorUpdate`) with a
  key derived via `pbkdf2WithSalt:iterCount:keyLength:` and `IOPlatformUUID`.
- Storage path: `+/-[NTSetting setDataValue:forKey:mtSecureKey:]`, plus
  `init_aesKey:iv:` / `aesKeyAndIVData` ObjC symbols.
- Keychain dump (`security dump-keychain login.keychain-db`) has **no** kakao item.

**Verdict: GO (recoverable in principle).** The AES key is device-derived
(`IOPlatformUUID` → PBKDF2), not hardware-bound — the same shape we already
implement in `src/local_db.rs` (`derive_secure_key`, `pbkdf2_sha256`) to open the
SQLCipher message DB. If/when we pursue it, the SQLCipher key-derivation code is the
starting point.

### Remaining work to actually decrypt (not yet done)

1. Disassemble `NTSetting setDataValue:forKey:mtSecureKey:` and `init_aesKey:iv:`
   to pin: exact PBKDF2 input string, iteration count, key length; AES mode
   (the 96-byte layout is likely `IV ‖ ciphertext ‖ tag`); IV source.
2. Reimplement the decryption in Rust (reuse `pbkdf2_sha256`).
3. Wire it behind a **version guard + graceful fallback** so a KakaoTalk update that
   changes the recipe degrades to manual login instead of breaking the tool.

This is deliberately **not** the primary path — see below.

## Preferred direction: explicit login, not extraction

Chasing the obfuscated-storage recipe across every KakaoTalk release is a permanent
maintenance debt (the cache approach already died this way). The robust path is the
self-contained email+password login that is already in the codebase and does **not**
depend on `Cache.db`:

- `KakaoRestClient::login_with_xvc(email, password, device_uuid, device_name)`
  posts to `katalk.kakao.com` and returns a fresh `access_token`.
- X-VC is computed locally: `SHA512("YLLAS|{email}|{device_uuid}|GRAEB|{user_agent}")[..16]`.
- `device_uuid` is the host `IOPlatformUUID` — we already read it in
  `local_db::get_platform_uuid()`, so it can be generated without a prior
  `login --save`.

**Gap to close for a true manual path:** `resolve_login_params` currently sources
`device_uuid` only from `credentials.json`, so a brand-new user (who never had a
working `login --save`) has no UUID and is blocked. Generating it from
`IOPlatformUUID` closes the gap and makes email+password login fully self-sufficient.

**New-device verification (UNRESOLVED — attempted then reverted):** logging in with a
fresh `device_uuid` makes `login.json` return `status=-100` (DEVICE_NOT_REGISTERED).
v1.3.2 tried to complete a passcode handshake (`request_passcode.json` →
`register_device.json`, mirroring node-kakao's `AuthApiClient`), but those routes
**do not exist on current KakaoTalk macOS builds — they return HTTP 404** (confirmed by
two users, one via binary analysis: #20, #22). The flow was reverted in v1.3.3.
**Critically, repeated `login.json` attempts from an unregistered device have gotten
real users' accounts' "sub-device login" blocked** — so this must not be brute-forced.
There is currently no known macOS REST path to register a new device from a
third-party client; the project is deprecated as of v1.3.3.

## TODO

- [x] Add `login --manual` (email/password prompt) that derives `device_uuid` from
      `IOPlatformUUID` and calls `login_with_xvc`, saving `credentials.json`. _(v1.3.0)_
- [x] Document the `auth.password_cmd` / `email_cmd` config path as the unattended
      equivalent. _(troubleshooting + authentication docs)_
- [ ] ~~Handle KakaoTalk new-device verification (passcode / 2FA) in the `--manual`
      flow~~ — attempted in v1.3.2, reverted in v1.3.3: the macOS app no longer exposes
      the passcode/register_device endpoints (404), and retrying risks an account block
      (#20, #22). No known path; project deprecated.
- [ ] (Optional, best-effort) Decrypt the obfuscated plist values per the recipe
      above, behind a version guard.
