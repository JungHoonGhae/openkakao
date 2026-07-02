<div align="center">
  <h1>OpenKakao</h1>
  <p>macOS용 카카오톡 데스크탑 앱을 위한 비공식 CLI입니다.</p>
  <p>터미널에서 직접 쓰기 좋고, JSON 출력, watch, hook, webhook 흐름으로 AI나 agent가 호출하기에도 적합합니다.</p>
  <p>실행 바이너리는 <code>openkakao-cli</code>입니다.</p>
</div>

<p align="center">
  <a href="#quick-start"><strong>Quick Start</strong></a> ·
  <a href="#핵심"><strong>핵심</strong></a> ·
  <a href="#문서"><strong>문서</strong></a> ·
  <a href="#claude-code-skill"><strong>Claude Code Skill</strong></a>
</p>

<p align="center">
  <a href="https://github.com/JungHoonGhae/openkakao-cli/stargazers"><img src="https://img.shields.io/github/stars/JungHoonGhae/openkakao-cli" alt="GitHub stars" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="MIT License" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.75+-orange.svg" alt="Rust" /></a>
  <a href="https://openkakao.vercel.app/"><img src="https://img.shields.io/badge/status-active-brightgreen" alt="Status Active" /></a>
  <a href="https://openkakao.vercel.app/"><img src="https://img.shields.io/badge/docs-fumadocs-black" alt="Docs" /></a>
</p>

**한국어** | [English](README.en.md)

> [!TIP]
> **로그인 없이 바로 동작합니다.** `local-send`/`ax-read`는 macOS Accessibility API로 카카오톡 UI를 직접 읽고 조작해서, 서버 세션 없이도 실제 메시지 전송과 최근 대화 읽기를 지원합니다. KakaoTalk 앱이 실행 중이고 로그인만 되어 있으면 됩니다 — 아래 [Quick Start](#quick-start) 참고.

> [!NOTE]
> 서버 로그인(`login --save`/`login --manual`)은 최근 KakaoTalk macOS 빌드에서 대부분 동작하지 않습니다 ([#15](https://github.com/JungHoonGhae/openkakao-cli/issues/15), [#20](https://github.com/JungHoonGhae/openkakao-cli/issues/20), [#22](https://github.com/JungHoonGhae/openkakao-cli/issues/22)). **미등록 기기로 로그인을 반복 시도하지 마세요** — 카카오가 계정의 "서브 디바이스 로그인"을 차단하거나 계정을 제재할 수 있습니다(실제 피해 사례가 보고되었습니다). 로컬 SQLCipher DB(`local-chats`/`local-read`/`local-search`)도 최신 빌드에서 키 유도 공식이 어긋나 신뢰할 수 없습니다 — 대신 `ax-read`를 쓰세요.

> [!WARNING]
> 이 프로젝트는 카카오(Kakao Corp.)와 무관한 비공식 CLI입니다. 연구, 자동화, 로컬 워크플로 용도로 만들었고, 카카오의 승인이나 보증을 받지 않았습니다.
> 사용 방식에 따라 카카오 이용약관 또는 운영정책 위반으로 해석될 수 있으며, 그 경우 사용자 계정이 정지되거나 영구 삭제될 수 있습니다.
> 사용 전에 관련 정책을 직접 확인하고, 모든 책임은 사용자 본인에게 있음을 전제로 신중히 사용하세요.

<div align="center">
<table>
  <tr>
    <td align="center"><strong>Works with</strong></td>
    <td align="center"><img src="docs/assets/logos/openclaw.svg" width="32" alt="OpenClaw" /><br /><sub>OpenClaw</sub></td>
    <td align="center"><img src="docs/assets/logos/claude.svg" width="32" alt="Claude Code" /><br /><sub>Claude Code</sub></td>
    <td align="center"><img src="docs/assets/logos/codex.svg" width="32" alt="Codex" /><br /><sub>Codex</sub></td>
    <td align="center"><img src="docs/assets/logos/cursor.svg" width="32" alt="Cursor" /><br /><sub>Cursor</sub></td>
    <td align="center"><img src="docs/assets/logos/bash.svg" width="32" alt="Bash" /><br /><sub>Bash</sub></td>
    <td align="center"><img src="docs/assets/logos/http.svg" width="32" alt="HTTP" /><br /><sub>HTTP</sub></td>
  </tr>
</table>
</div>

<p align="center">
  <a href="https://www.star-history.com/?repos=JungHoonGhae%2Fopenkakao&type=date&legend=top-left">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/image?repos=JungHoonGhae/openkakao-cli&type=date&theme=dark&legend=top-left" />
      <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/image?repos=JungHoonGhae/openkakao-cli&type=date&legend=top-left" />
      <img alt="Star History Chart" src="https://api.star-history.com/image?repos=JungHoonGhae/openkakao-cli&type=date&legend=top-left" width="600" />
    </picture>
  </a>
</p>

<p align="center">
  <img src="assets/thumbnail-ko.png" alt="openkakao" width="720" />
</p>

## Quick Start

### 로그인 없이 쓰기 (권장)

서버 로그인이 필요 없는 경로입니다. KakaoTalk 앱이 실행 중이고 로그인되어 있기만 하면 됩니다.

```bash
# Homebrew
brew tap JungHoonGhae/openkakao
brew install openkakao-cli

# 1. 실제 전송 전 화이트리스트에 채팅방을 등록 (필수 — 아무 채팅에나 보내지 않도록)
#    ~/.config/openkakao/config.toml
#    [safety]
#    allow_ax_send = true
#    allowed_send_chats = ["나와의 채팅에 표시되는 이름"]

# 2. 메시지 보내기 — 서버 접촉 없음, 실제 카톡 UI를 직접 조작
openkakao-cli local-send "채팅방 표시 이름" "Hello from CLI!" --dry-run   # 미리보기
openkakao-cli local-send "채팅방 표시 이름" "Hello from CLI!" -y         # 실제 전송

# 3. 최근 메시지 읽기 — 같은 방식(AX)으로 화면에 보이는 메시지를 스크랩
openkakao-cli ax-read "채팅방 표시 이름" -n 20
```

### 서버 로그인 기반 (현재 대부분 깨짐)

```bash
# 1. 인증 정보 저장 — 최신 빌드에서는 대부분 실패합니다 (#15, #20, #22)
openkakao-cli login --manual --save
#    (예전 빌드에서 캐시 추출이 되는 경우: openkakao-cli login --save)

# 2. 채팅방 목록
openkakao-cli chats

# 3. 메시지 읽기
openkakao-cli read <chat_id> -n 20

# 4. 메시지 보내기 (LOCO write — opt-in 필요: safety.allow_loco_write = true)
openkakao-cli send <chat_id> "Hello from CLI!"

# 로컬 DB 읽기 (현재 최신 빌드에서 키 유도 실패로 신뢰 불가 — ax-read 권장)
openkakao-cli local-chats
openkakao-cli local-read <chat_id>
```

필요할 때만 예전 cache-backed 경로를 강제합니다.

```bash
openkakao-cli chats --rest
openkakao-cli read <chat_id> --rest
openkakao-cli members <chat_id> --rest
```

### For Agent

```bash
# 로그인 없이 읽고 쓰기 (서버 통신 없음, AX 기반)
openkakao-cli ax-read "채팅방 표시 이름" -n 20 --json
openkakao-cli local-send "채팅방 표시 이름" "message" -y --json

# 실행 전 미리보기
openkakao-cli send <chat_id> "message" --dry-run --json

# 구조화된 출력
openkakao-cli --json chats
openkakao-cli --json read <chat_id> -n 20

# 실시간 이벤트 감시
openkakao-cli watch --json

# 로컬 hook 또는 webhook 흐름으로 연결
openkakao-cli --unattended --allow-watch-side-effects watch \
  --hook-cmd 'jq . > /tmp/openkakao-event.json'
```

Claude Code에서 바로 쓰려면:

```bash
npx skills add JungHoonGhae/skills@openkakao-cli
```

## 핵심

- `local-send`/`ax-read`로 **로그인 없이** 실제 메시지 전송·읽기 (macOS Accessibility API로 카톡 UI를 직접 조작, 서버 통신 없음)
- macOS 카카오톡 앱에서 인증 정보 추출
- 채팅, 메시지, 멤버, 친구, 프로필 조회
- LOCO 기반 메시지 전송, 실시간 watch, 미디어 처리
- `--json` 출력으로 `jq`, `cron`, SQLite, LLM 흐름과 연결 가능
- `watch`, `hook`, `webhook`로 로컬 자동화와 에이전트 워크플로에 연결 가능
- `friends --local`, `profile --local`, `profile --chat-id`로 일부 조회 복구 가능
- `local-chats`, `local-read`, `local-search`로 로컬 DB 읽기 시도 (최신 빌드에서는 키 유도 실패로 신뢰 불가 — `ax-read` 권장)
- `--dry-run`으로 실행 전 미리보기
- `send --me`로 나와의 채팅에 바로 전송 (테스트용)
- LOCO write 기본 비활성 — `safety.allow_loco_write = true`로 opt-in
- `local-send`도 기본 비활성 — `safety.allow_ax_send = true` + `safety.allowed_send_chats` 화이트리스트로 opt-in

## 이런 경우에 잘 맞습니다

- 채팅 기록을 JSON으로 읽어서 다른 도구로 넘기고 싶을 때
- 카카오톡을 로컬 스크립트나 운영 도구의 입력 채널로 쓰고 싶을 때
- watch 이벤트를 hook이나 webhook으로 받아 후속 작업을 실행하고 싶을 때
- 사람이 직접 쓰는 CLI와 AI가 호출하는 로컬 인터페이스를 같이 두고 싶을 때

## 안전 모드

v1.1.0부터 LOCO write 작업(send, delete, edit, react)은 **기본 비활성**입니다.
계정 보호를 위해 서버에 쓰기 요청을 보내는 명령은 명시적 opt-in이 필요합니다.

```toml
# ~/.config/openkakao/config.toml
[safety]
allow_loco_write = true
```

`local-send`(AX 기반 실전송)도 v1.4.0부터 기본 비활성이며, 별도로 opt-in과 **채팅방 화이트리스트**가 필요합니다. `local-send`는 채팅 목록에서 표시 이름이 정확히 일치하는 방을 찾아 전송하는데, 로컬 DB의 chat-id로 대상을 다시 검증할 방법이 없어졌기 때문에 화이트리스트가 유일한 안전장치입니다:

```toml
# ~/.config/openkakao/config.toml
[safety]
allow_ax_send = true
allowed_send_chats = ["나와의 채팅에 표시되는 이름", "다른 허용 채팅방 이름"]
```

읽기 전용 작업은 항상 사용 가능합니다:

| 명령 | 설명 | 서버 통신 |
|------|------|-----------|
| `ax-read <chat_name>` | 화면에 열린 채팅의 최근 메시지 스크랩 (AX) | 없음 |
| `local-chats` | 로컬 DB 채팅 목록 (최신 빌드에서 신뢰 불가) | 없음 |
| `local-read <id>` | 로컬 DB 메시지 읽기 (최신 빌드에서 신뢰 불가) | 없음 |
| `local-search "keyword"` | 로컬 DB 검색 (최신 빌드에서 신뢰 불가) | 없음 |
| `chats --rest` | REST API 채팅 목록 | REST |
| `read <id> --rest` | REST API 메시지 읽기 | REST |
| `send ... --dry-run` | 전송 미리보기 | 없음 |
| `local-send ... --dry-run` | AX 전송 미리보기 | 없음 |

## 요구 사항

| Requirement | Notes |
|-------------|-------|
| macOS | 카카오톡 데스크탑 앱 설치 및 로그인 필요 |
| Rust >= 1.75 | 소스 빌드 시 |

## 설치

### Homebrew

```bash
brew tap JungHoonGhae/openkakao
brew install openkakao-cli
```

### From source

```bash
git clone https://github.com/JungHoonGhae/openkakao-cli.git
cd openkakao/openkakao-cli
cargo install --path .
```

## 문서

- 문서 사이트: https://openkakao.vercel.app/
- 빠른 시작: https://openkakao.vercel.app/docs/getting-started/quickstart/
- CLI 레퍼런스: https://openkakao.vercel.app/docs/cli/overview/
- 자동화 개요: https://openkakao.vercel.app/docs/automation/overview/
- LLM / agent 워크플로: https://openkakao.vercel.app/docs/automation/llm-agent-workflows/
- watch 패턴: https://openkakao.vercel.app/docs/automation/watch-patterns/
- 프로토콜 문서: https://openkakao.vercel.app/docs/protocol/overview/

Reverse engineering / local app-state diff:

```bash
openkakao-cli profile-hints --local-graph --json
openkakao-cli profile-hints --app-state --json > /tmp/profile-before.json
openkakao-cli profile-hints --app-state --app-state-diff /tmp/profile-before.json --json
```

## Claude Code Skill

```bash
npx skills add JungHoonGhae/skills@openkakao-cli
```

## 개발

```bash
cd openkakao-cli
cargo build --release
```

자세한 사용법, 운영 메모, 프로토콜 설명은 문서 사이트에 정리되어 있습니다.

## Support

이 프로젝트가 도움이 되셨다면 응원해 주세요:

<a href="https://www.buymeacoffee.com/lucas.ghae">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="50">
</a>

## Contributing

버그 제보와 PR 환영합니다.

## Acknowledgments

- [kakaocli](https://github.com/silver-flight-group/kakaocli) (MIT) — `local-send`의 macOS Accessibility API 기반 카톡 UI 자동 조작(채팅방 행 선택, 입력창 탐색·전송) 로직을 Rust로 이식했습니다 (`src/ax_send.rs`).
- [Peekaboo](https://github.com/steipete/Peekaboo) (MIT) — `local-send`에서 `CGEventPostToPid`로 이벤트를 대상 프로세스에 직접 전달하는 방식을 참고해, kakaocli가 겪던 포그라운드 활성화 타이밍 레이스([silver-flight-group/kakaocli#9](https://github.com/silver-flight-group/kakaocli/issues/9))를 우회했습니다.

## License

MIT
