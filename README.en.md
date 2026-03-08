# OpenKakao

[![GitHub stars](https://img.shields.io/github/stars/JungHoonGhae/openkakao)](https://github.com/JungHoonGhae/openkakao/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/JungHoonGhae/openkakao/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Status: Beta](https://img.shields.io/badge/status-beta-FEE500)](https://openkakao.vercel.app/)
[![Docs](https://img.shields.io/badge/docs-fumadocs-black)](https://openkakao.vercel.app/)

[한국어](README.md) | **English**

Beta-stage unofficial CLI for the KakaoTalk macOS desktop app. It can read personal chats, inspect friends and profiles, and send messages through the LOCO protocol.

> **Disclaimer**: This is a technical research CLI tool. It is not affiliated with or endorsed by Kakao Corp.

<p align="center">
  <img src="openkakao-rs/assets/thumbnail-en.png" alt="openkakao" width="600" />
</p>

## Highlights

- Extracts auth data from the macOS KakaoTalk app
- Reads chats, messages, friends, and profiles
- Sends messages, watches real-time events, and handles media over LOCO
- Works well with `jq`, `cron`, and LLM tooling through `--json`

## Docs

- Documentation site: https://openkakao.vercel.app/
- Quick start: https://openkakao.vercel.app/docs/getting-started/quickstart/
- CLI reference: https://openkakao.vercel.app/docs/cli/overview/
- Protocol docs: https://openkakao.vercel.app/docs/protocol/overview/

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
openkakao-rs loco-chats

# 3. Read messages
openkakao-rs loco-read <chat_id> -n 20

# 4. Send a message
openkakao-rs send <chat_id> "Hello from CLI!"
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

Detailed usage and protocol notes now live in the docs site.

## License

MIT
