---
title: Encryption
description: RSA-2048 handshake + AES-128-GCM transport encryption.
---

## Handshake

The client generates a random 16-byte AES key, encrypts it with the server's RSA public key, and sends a 268-byte handshake packet:

```
[key_size: 4 LE = 256][key_encrypt_type: 4 LE = 16][encrypt_type: 4 LE = 3][encrypted_key: 256]
```

| Field | Value | Description |
|-------|-------|-------------|
| `key_size` | 256 | RSA-2048 output size in bytes |
| `key_encrypt_type` | 16 (0x10) | RSA-OAEP SHA-1 |
| `encrypt_type` | 3 | AES-128-GCM |
| `encrypted_key` | 256 bytes | RSA-encrypted AES key |

::: warning
`key_encrypt_type` must be 16, not 15. This single-bit difference determines whether the server accepts the handshake.
:::

## RSA Parameters

| Parameter | Value |
|-----------|-------|
| Key size | 2048-bit |
| Exponent (e) | 3 |
| Padding | OAEP |
| Hash | SHA-1 (not SHA-256) |
| Key format | DER/PKCS#1 Base64-encoded in the binary |

The RSA public key is extracted from `/Applications/KakaoTalk.app/Contents/MacOS/KakaoTalk`.

## AES-128-GCM Transport

After the handshake, all data is encrypted with AES-128-GCM:

```
[size: 4 LE][nonce: 12][ciphertext + GCM tag: N + 16]
```

| Component | Size | Description |
|-----------|------|-------------|
| Size prefix | 4 bytes | Total size of nonce + ciphertext + tag |
| Nonce | 12 bytes | Random, unique per frame |
| Ciphertext | N bytes | Encrypted LOCO packet data |
| GCM tag | 16 bytes | Authentication tag |

Each packet and each file upload chunk uses a fresh random nonce to prevent nonce reuse.

## Historical Note

Earlier versions of KakaoTalk used `encrypt_type = 2` (AES-128-CFB). The current version uses `encrypt_type = 3` (AES-128-GCM), which provides authenticated encryption.
