# OpenKakao

[![GitHub stars](https://img.shields.io/github/stars/JungHoonGhae/openkakao)](https://github.com/JungHoonGhae/openkakao/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/JungHoonGhae/openkakao/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Status: Stable](https://img.shields.io/badge/status-v1.0.0%20stable-brightgreen)](https://openkakao.vercel.app/)
[![Docs](https://img.shields.io/badge/docs-fumadocs-black)](https://openkakao.vercel.app/)

| [<img alt="GitHub Follow" src="https://img.shields.io/github/followers/JungHoonGhae?style=flat-square&logo=github&labelColor=black&color=24292f" width="156px" />](https://github.com/JungHoonGhae) | Follow [@JungHoonGhae](https://github.com/JungHoonGhae) on GitHub for more projects. |
| :-----| :----- |
| [<img alt="X link" src="https://img.shields.io/badge/Follow-%40lucas_ghae-000000?style=flat-square&logo=x&labelColor=black" width="156px" />](https://x.com/lucas_ghae) | Follow [@lucas_ghae](https://x.com/lucas_ghae) on X for updates. |

**한국어** | [English](README.en.md)

카카오톡 macOS 데스크탑 앱을 위한 비공식 CLI입니다. 채팅, 친구, 프로필을 조회하고 LOCO 기반 메시지 워크플로를 다룰 수 있습니다.

> **Disclaimer**: 이 프로젝트는 기술 연구 목적의 CLI 도구입니다. 카카오(Kakao Corp.)와 무관하며, 카카오의 승인이나 보증을 받지 않았습니다.

<p align="center">
  <img src="openkakao-rs/assets/thumbnail-ko.png" alt="openkakao" width="600" />
</p>

## 핵심

- macOS 카카오톡 앱에서 인증 정보 추출
- 채팅방/메시지 조회는 기본적으로 LOCO 우선
- `friends --local`, `profile --local`, `profile --chat-id`로 REST 장애 시에도 일부 조회 가능
- LOCO 기반 메시지 전송, 실시간 watch, 미디어 처리
- `--json` 출력으로 `jq`, `cron`, `LLM`과 조합 가능

## 요구 사항

| Requirement | Notes |
|-------------|-------|
| macOS | 카카오톡 데스크탑 앱 설치 및 로그인 필요 |
| Rust >= 1.75 | 소스 빌드 시 |

## 설치

```bash
# Homebrew
brew tap JungHoonGhae/openkakao
brew install openkakao-rs

# 또는 소스 빌드
git clone https://github.com/JungHoonGhae/openkakao.git
cd openkakao/openkakao-rs
cargo install --path .
```

## 빠른 시작

```bash
# 1. 인증
openkakao-rs login --save

# 2. 채팅방 목록
openkakao-rs chats

# 3. 메시지 읽기
openkakao-rs read <chat_id> -n 20

# 4. 메시지 보내기
openkakao-rs send <chat_id> "Hello from CLI!"
```

필요할 때만 예전 cache-backed 경로를 강제합니다.

```bash
openkakao-rs chats --rest
openkakao-rs read <chat_id> --rest
openkakao-rs members <chat_id> --rest
```

로컬 그래프 기반 조회:

```bash
openkakao-rs friends --local
openkakao-rs profile <user_id> --local
openkakao-rs profile <user_id> --chat-id <chat_id>
```

진단:

```bash
openkakao-rs auth-status
openkakao-rs doctor --loco
```

## 문서

- 문서 사이트: https://openkakao.vercel.app/
- 빠른 시작: https://openkakao.vercel.app/docs/getting-started/quickstart/
- CLI 레퍼런스: https://openkakao.vercel.app/docs/cli/overview/
- 프로토콜 문서: https://openkakao.vercel.app/docs/protocol/overview/

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

## 개발

```bash
cd openkakao-rs
cargo build --release
```

자세한 사용법과 운영/프로토콜 설명은 문서 사이트를 참고해 주세요.

## Support

이 프로젝트가 도움이 되셨다면 응원해 주세요:

<a href="https://www.buymeacoffee.com/lucas.ghae">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="50">
</a>

## Contributing

버그 제보나 PR 환영합니다.

## License

MIT
