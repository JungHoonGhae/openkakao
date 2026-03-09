# OpenKakao

[![GitHub stars](https://img.shields.io/github/stars/JungHoonGhae/openkakao)](https://github.com/JungHoonGhae/openkakao/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/JungHoonGhae/openkakao/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Status: Beta](https://img.shields.io/badge/status-beta-FEE500)](https://openkakao.vercel.app/)
[![Docs](https://img.shields.io/badge/docs-fumadocs-black)](https://openkakao.vercel.app/)

[한국어](README.md) | **English**

Unofficial CLI for KakaoTalk on macOS, currently in beta. Reads chats, profiles, and friends, and drives LOCO-based messaging workflows.

> **Disclaimer**: This is a technical research CLI tool. It is not affiliated with or endorsed by Kakao Corp.

<p align="center">
  <img src="openkakao-rs/assets/thumbnail-en.png" alt="openkakao" width="600" />
</p>

## Highlights

- Extracts auth data from the macOS KakaoTalk app
- Uses LOCO-first paths for chats and message reads
- Can recover some friend/profile reads with `friends --local`, `profile --local`, and `profile --chat-id`
- Sends messages, watches real-time events, and handles media over LOCO
- Works well with `jq`, `cron`, and LLM tooling through `--json`

## Requirements

| Requirement | Notes |
|-------------|-------|
| macOS | KakaoTalk desktop app must be installed and logged in |
| Rust >= 1.75 | Only for source builds |

## Installation

```bash
# Homebrew
brew tap JungHoonGhae/openkakao
brew install openkakao-rs

# Or build from source
git clone https://github.com/JungHoonGhae/openkakao.git
cd openkakao/openkakao-rs
cargo install --path .
```

## Quick Start

```bash
# 1. Authenticate
openkakao-rs login --save

# 2. List chat rooms
openkakao-rs chats

# 3. Read messages
openkakao-rs read <chat_id> -n 20

# 4. Send a message
openkakao-rs send <chat_id> "Hello from CLI!"
```

Only force the older cache-backed path when you need it:

```bash
openkakao-rs chats --rest
openkakao-rs read <chat_id> --rest
openkakao-rs members <chat_id> --rest
```

Local graph-based reads:

```bash
openkakao-rs friends --local
openkakao-rs profile <user_id> --local
openkakao-rs profile <user_id> --chat-id <chat_id>
```

Diagnostics:

```bash
openkakao-rs auth-status
openkakao-rs doctor --loco
```

## Docs

- Documentation site: https://openkakao.vercel.app/
- Quick start: https://openkakao.vercel.app/docs/getting-started/quickstart/
- CLI reference: https://openkakao.vercel.app/docs/cli/overview/
- Protocol docs: https://openkakao.vercel.app/docs/protocol/overview/

Reverse engineering / local app-state diff:

```bash
openkakao-rs profile-hints --local-graph --json
openkakao-rs profile-hints --app-state --json > /tmp/profile-before.json
openkakao-rs profile-hints --app-state --app-state-diff /tmp/profile-before.json --json
```

## Claude Code Skill

```bash
npx skills add JungHoonGhae/skills@openkakao-cli
```

## Development

```bash
cd openkakao-rs
cargo build --release
```

Detailed usage, operations notes, and protocol details now live in the docs site.

## Contributing

Bug reports and PRs welcome.

## License

MIT
