# SQLCipher DB key derivation on KakaoTalk macOS 26.5.0+

Status: research note (internal). Static analysis only — no running/hooking the
app, no LOCO/REST traffic, no account risk. Inspected KakaoTalk
**26.6.1** (build 1190) — the newest build in the affected 26.5.0+ range
(`/Applications/KakaoTalk.app/Contents/Info.plist` → `CFBundleShortVersionString`).
Related issue: [#39](https://github.com/JungHoonGhae/openkakao-cli/issues/39).

## Question

Why does `src/local_db.rs`'s `derive_secure_key` / `derive_database_name` (the
old formula) no longer open the message DB on KakaoTalk 26.5.0+, even though
userId and device UUID are still recovered correctly?

## Headline verdict

**Decisive change, not structural sealing → GO path (recoverable in principle).**

- The key is still **device-derived via PBKDF2-HMAC**, same primitive as today.
  No Secure Enclave, no Keychain item, no `LAContext` in the DB path.
- What changed: KakaoTalk 26.5.0+ **rederives the DB filename and the SQLCipher
  key from new input templates** and **migrates the old DB to the new
  name+key**. The old formula's derived name and key therefore no longer match
  what is on disk — exactly the reported symptom (userId/uuid fine, decrypt fails).
- The **new input template strings are recoverable statically** (found — below).
  The remaining numeric parameters (iteration count, key length, salt source,
  which template maps to name vs key, any reverse/substring transform) sit in an
  ARC-heavy (apparently Swift) helper that this static pass could **not** cleanly
  resolve with `otool`/`radare2`. Pinning them exactly is a small next step:
  either deeper disassembly of that one helper, or a **one-time dynamic hook on
  `pbkdf2WithSalt:iterCount:keyLength:`** to dump password/salt/iter/keylen —
  the latter is out of scope for this static-only ticket.

## The old recipe (what's in the codebase today)

`src/local_db.rs`:

- `derive_database_name` (line 291): password template
  `"..F.{userId}.A.F.{reversedUuid}..|"` → literal `..F.%lld.A.F.%@..|`;
  salt = reversed `base64(SHA1(uuid) ‖ SHA256(uuid))`; `pbkdf2_sha256(pw, salt,
  100_000, 128)`; hex; substring `[28..106]` (78 hex chars).
- `derive_secure_key` (line 307): password = parts
  `["A", hashed, "|", "F", uuid[..5], "H", userId, "|", uuid[7..]]` joined by
  `"F"`, then the whole string **reversed**; salt = `uuid[30%..]`;
  `pbkdf2_sha256(pw, salt, 100_000, 128)`; full hex.
- `pbkdf2_sha256` (line 336): PBKDF2-HMAC-SHA256, iter 100_000, keylen 128.
- Open path (line 448): `PRAGMA cipher_compatibility = 3` then `PRAGMA key`.

The distinctive literal markers of this recipe are `..F.`, `.A.F.`, `..|`
(the `derive_database_name` template).

## What the binary shows (primary evidence)

Binary: `/Applications/KakaoTalk.app/Contents/MacOS/KakaoTalk` (universal
x86_64+arm64, 69 MB; not FairPlay-encrypted). Findings from the arm64 slice.

### 1. The old template markers are GONE

```
strings -a KakaoTalk | grep -E '\.\.F\.|\.A\.F\.|\.\.\|'   →  (no matches, both slices)
```

The `..F.%lld.A.F.%@..|` database-name template no longer exists in the binary.

### 2. New KDF templates are present (`__cstring`, source `/MacTalk/NT/NTDataStore.m`)

```
0x1019a03b2   J|%lld|O|%@|SH      (cfstring wrapper 0x101d90630)
0x1019a03c1   KY|%lld|%@          (cfstring wrapper 0x101d90650)
0x1019a0449   Detected old database. (old databaseName:%@, new databaseName:%@, userId:%@, uuid:%@)
```

`%lld` = userId, `%@` = uuid (or a hash of it). These three strings are
**consecutive in `__cstring`** and, decisively, all three cfstring wrappers are
**loaded inside one function** at `0x1011dffe8`
(adrp `0x101d90000` + add `#0x630`/`#0x650`/`#0x710`) — i.e. the new name/key
templates are used together with the old→new migration log. That co-location is
what ties `J|…` / `KY|…` to DB name/key derivation.

Tentative mapping (not yet pinned by disassembly): `KY|%lld|%@` → secure**KY**
(key), `J|%lld|O|%@|SH` → databaseName. Treat as a hypothesis until confirmed.

### 3. Old→new migration exists (why the on-disk DB moved)

- `+[NTDataStore convertOldDatabaseNamed:withOldKey:toDatabaseNamed:withKey:]`
- `+[NTDataStore removeDatabaseNamed:]`
- log: `Detected old database. (old databaseName:%@, new databaseName:%@, userId:%@, uuid:%@)`

The app detects a DB created by the old recipe, converts it to a **new name with
a new key**, and can remove the old one. So a logged-in 26.5.0+ user's DB is
under the new name/key; the old-formula name/key match nothing on disk.

### 4. Primitive unchanged — no hardware sealing

- ObjC selector `pbkdf2WithSalt:iterCount:keyLength:` still present.
- Imports `_CCKeyDerivationPBKDF`, `_PKCS5_PBKDF2_HMAC_SHA1` (CommonCrypto).
- `ivar _secureKey`, `_databaseName`; `-[NTSetting setDataValue:forKey:mtSecureKey:]`.
- `strings | grep -E 'kSecAttrTokenIDSecureEnclave|LAContext|SecKeyCreateRandomKey'`
  → **no matches** (consistent with `credential-storage.md`). The key is not
  bound to Secure Enclave or Keychain.

So the crypto **structure** is intact (PBKDF2-HMAC, device-derived). Only the
**inputs** (name/key templates, and by extension salt/iter/keylen/transform)
changed, plus a migration step.

## What is NOT statically determinable in this pass

The derivation helper reached from `0x1011dffe8` is ARC-heavy and shows no direct
`objc_msgSend` in r2's linear view (looks Swift/outlined); `otool` and `radare2`
both mislabel the `__cfstring` pointers here. From static bytes alone this pass
could **not** reliably extract:

- iteration count (old = 100_000 — may or may not have changed),
- key length (old = 128 bytes),
- salt source for each new template,
- which template is name vs key,
- any byte-reversal / substring slicing applied to the PBKDF2 output,
- whether `cipher_compatibility` / KDF-iter SQLCipher pragmas changed
  (`cipher_compatibility` does not appear as a literal string).

These are the missing pieces for a working reimplementation. They are best
obtained by disassembling the single helper more carefully, or — decisively and
quickly — a **one-time dynamic hook** on `pbkdf2WithSalt:iterCount:keyLength:`
that logs its four arguments (explicitly out of scope for this static ticket).

## Concrete diff (old → new)

| | Old (in `local_db.rs`) | New (26.5.0+, from binary) |
|---|---|---|
| DB-name password template | `..F.%lld.A.F.%@..|` | replaced — old markers absent; new: `J|%lld|O|%@|SH` (tentative) |
| Secure-key password template | parts joined by `F`, reversed | replaced — new: `KY|%lld|%@` (tentative) |
| PBKDF2 primitive | HMAC-SHA256, 100_000, 128 | same primitive (`pbkdf2WithSalt:iterCount:keyLength:`); exact iter/keylen not pinned |
| Salt / reversal / substring | see above | **unknown** (needs disasm or dynamic dump) |
| Storage/DB lifecycle | single DB | old→new **migration** (`convertOldDatabaseNamed:withOldKey:toDatabaseNamed:withKey:`) |
| Hardware sealing | none | none (no SE/Keychain/LAContext) |

## Recommendation

Pursue the GO path but do not attempt to guess the numeric params. One dynamic
`pbkdf2WithSalt:iterCount:keyLength:` capture (on a throwaway/consenting account)
would pin the full new recipe in minutes; wire the result behind a
KakaoTalk-version guard with graceful fallback, since this can change again on
future builds (as it just did).

---

## Update — dynamic-hook attempt (KakaoTalk 26.6.1, issue #43)

The static "one dynamic hook and you're done" recommendation above was tested
end-to-end. It ran into a **different, harder wall** than expected. Recording
the full result so a future attempt doesn't re-walk it.

### What the offline brute-force established (before any hooking)

Using the on-disk encrypted DB filename as a free oracle
(`…/Application Support/com.kakao.KakaoTalkMac/<78-hex>`, matching the old
`hex[28:106]` output length), ~13,400 PBKDF2 candidates were tried offline
(templates × uuid-variants × salt-derivations × PRF{sha256,sha512} ×
keylen{64,128} × iter{4096,100000,…}). **No match.**

Binary + plist evidence then reframed the model:

- `KY|%lld|%@` / `J|%lld|O|%@|SH` sit next to `drawerUserInfoSecureKey:%@` and
  `+[NTSetting setDataValue:forKey:mtSecureKey:]` — they are **NTSetting
  storage keys, not PBKDF2 password templates**.
- No `KY|` / `J|` literal keys exist in the plists (NTSetting obfuscates key
  names, cf. `Dfpr93S FDS zXCV`).
- The binary's actual `iteration:4096, keyLen:64` PBKDF2 params belong to the
  **chatBackup / pin** path, not the message DB.

**Conclusion: the DB `secureKey` / `databaseName` are not re-derived at open
time — they are values generated once and stored AES-encrypted (device-derived
key) via NTSetting.** There is no derivation recipe to recover; the final key
must be observed or the stored blob decrypted.

### Dynamic hook — what worked, what blocked it

Target confirmed: KakaoTalk links **`SQLCipher.framework`** (separate dylib);
`sqlite3_key(db, pKey, nKey)` is an exported symbol → hooking it yields the raw
key regardless of derived-vs-stored.

Method: copy the app, ad-hoc re-sign the **copy** (original untouched) keeping
App Sandbox but **removing `network.client/server`** (per-app network block —
the running process had **zero TCP sockets**, all Firebase/Google connects
failed `-1003`), adding `get-task-allow`. Launched under lldb.

- **Anti-debug**: first launch `exit(45)` immediately — cause is
  **`ptrace(PT_DENY_ATTACH=31)`**. Intercepting that call in lldb and returning
  0 (skip) defeats it; the app then runs normally under the debugger.
  (Note: a breakpoint on the name `exit` also matches Security.framework's
  `CountingMutex::exit` — a false positive.)
- **`sqlite3_key` / `sqlite3_key_v2` breakpoints armed** (2 locations each,
  resolved when SQLCipher.framework loads).
- **Blocker — keychain**: ad-hoc re-signing cannot inherit Kakao's
  keychain-access-group, so `SecItemAdd` fails `-34018` and the app **treats
  itself as logged out** (window title = `로그인`). It therefore **never opens
  the encrypted message DB** (`f715…` never appears in `lsof`), so `sqlite3_key`
  never fires. The hook is correct; there is simply nothing to catch offline.

### Where this leaves a future attempt

To capture the key via the running app you need the app in its **logged-in**
state, which the re-sign breaks. Remaining routes:

1. **Debug the original (un-re-signed, logged-in) app** — requires **SIP
   disabled** so lldb can attach to a hardened-runtime binary without
   re-signing. Then the same ptrace-skip + `sqlite3_key` hook capture the key
   offline (zero server contact). Reusable assets: `hook_sqlite3_key.py`,
   `capture.py`, the `ptrace(PT_DENY_ATTACH)` bypass.
2. **Offline NTSetting-AES decryption** — reverse `init_aesKey:iv:` /
   `mtSecureKey` (device-derived AES per this doc's earlier verdict) and decrypt
   the stored `databaseName` / `secureKey` blobs. No app run, no login, no SIP.
   Open question: on 26.6.1 those blobs were **not** found in the two
   `com.kakao.KakaoTalkMac*.plist` files — likely in the ~3 MB self-encrypted
   `Application Support/<hex>` store; locate before committing to this.

As of #40 the DB receive path is **NO-GO** for the current effort (neither route
was pursued); `ax-watch` hardening (Tier 2 bubble diffing, #42) is the fallback.
