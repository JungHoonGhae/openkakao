# OpenKakao

[![GitHub stars](https://img.shields.io/github/stars/JungHoonGhae/openkakao)](https://github.com/JungHoonGhae/openkakao/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/JungHoonGhae/openkakao/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Status: Beta](https://img.shields.io/badge/status-beta-FEE500)](https://openkakao.vercel.app/)
[![Docs](https://img.shields.io/badge/docs-fumadocs-black)](https://openkakao.vercel.app/)

**한국어** | [English](README.en.md)

카카오톡 macOS 데스크탑 앱을 위한 비공식 CLI입니다. 채팅, 친구, 프로필을 조회하고 LOCO 프로토콜로 메시지를 주고받을 수 있습니다. 현재 beta 단계입니다.

> **Disclaimer**: 이 프로젝트는 기술 연구 목적의 CLI 도구입니다. 카카오(Kakao Corp.)와 무관하며, 카카오의 승인이나 보증을 받지 않았습니다.

<p align="center">
  <img src="openkakao-rs/assets/thumbnail-ko.png" alt="openkakao" width="600" />
</p>

## 핵심

- macOS 카카오톡 앱에서 인증 정보 추출
- 채팅방/메시지/친구/프로필 조회
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
openkakao-rs loco-chats

# 3. 메시지 읽기
openkakao-rs loco-read <chat_id> -n 20

# 4. 메시지 보내기
openkakao-rs send <chat_id> "Hello from CLI!"
```

## 문서

- 문서 사이트: https://openkakao.vercel.app/
- 빠른 시작: https://openkakao.vercel.app/docs/getting-started/quickstart/
- CLI 레퍼런스: https://openkakao.vercel.app/docs/cli/overview/
- 프로토콜 문서: https://openkakao.vercel.app/docs/protocol/overview/

## Claude Code Skill

```bash
npx skills add JungHoonGhae/skills@openkakao-cli
```

## 개발

```bash
cd openkakao-rs
cargo build --release
```

자세한 사용법과 프로토콜 설명은 문서 사이트를 참고해 주세요.

## Contributing

버그 제보나 PR 환영합니다.

## License

MIT
