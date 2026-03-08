# Doppler Password Command Design

## Goal

Allow unattended relogin to fetch the Kakao password from an external command such as Doppler instead of requiring a cached plaintext password inside local request state.

## Approach

Use a single new config field:

```toml
[auth]
password_cmd = "doppler secrets get KAKAO_PASSWORD -p openkakao -c dev --plain"
```

Relogin password precedence becomes:

1. explicit CLI `--password`
2. `auth.password_cmd`
3. cached `login.json` password

If `password_cmd` fails or returns an empty string, recovery falls back to the cached `login.json` password and logs a warning.

## Why This Approach

- It avoids storing the Kakao password in `config.toml`.
- It keeps Doppler optional rather than hard-wiring a dependency into the CLI.
- It improves unattended recovery without changing the user-facing auth flow.

## Scope

- Add `auth.password_cmd` to config parsing.
- Allow `login.json` extraction to succeed when the email exists but the password is missing.
- Use `password_cmd` only during relogin.
- Document the Doppler example in auth/configuration docs.
