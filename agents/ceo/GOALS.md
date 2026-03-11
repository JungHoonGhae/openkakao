# OpenKakao — CEO Goals

## Company Mission
OpenKakao를 macOS 카카오톡의 사실상 표준 CLI 클라이언트로 만든다. AI 에이전트와 개발자가 카카오톡을 프로그래밍할 수 있는 인프라를 제공한다.

## Current State (v0.9.2)
- Rust CLI (`openkakao-rs`): LOCO + REST 완전 동작, 25K+ lines
- 240 tests passing, zero clippy warnings
- Homebrew 배포, fumadocs 문서 사이트
- 핵심 기능: send, watch, read, chats, friends, profile, download, analytics

---

## Goals (Priority Order)

### G1: Full Conversation Access — E2E Verified
**Priority: CRITICAL — Do this first**

모든 친구의 대화를 보고, 대화를 시작하고, 이전 메시지 전체를 불러올 수 있어야 한다. E2E 테스트로 검증.

**Requirements:**
- [ ] 모든 친구의 대화방 목록 조회 (`chats`)
- [ ] 특정 친구와 대화 시작 (1:1 DM 생성)
- [ ] 대화 이전 메시지 전체 불러오기 (full history scroll-back)
- [ ] 메시지 전송 + 수신 확인

**E2E Test Plan (순서대로):**
1. **Christine (여자친구)** `382367313744175` — 최근 대화 있어서 가장 쉬움. 여기부터 시작
2. **엄마/오정숙** `384660504914024` — send/delete/mark-read/react 기존 검증됨
3. **누나/Kate** `383248611365287` — send/delete/mark-read/react 기존 검증됨

**E2E Checklist (per contact):**
- [ ] `chats` — 대화방 목록에서 해당 채팅방 확인
- [ ] `read <id> --all` — 전체 메시지 히스토리 로드
- [ ] `send <id> "test" -y` — 메시지 전송
- [ ] `watch --chat-id <id>` — 실시간 수신 확인
- [ ] `mark-read <id> <logId>` — 읽음 표시
- [ ] `react <id> <logId>` — 리액션

### G2: v1.0 Release — Production Ready
**Priority: HIGH — G1 완료 후**

v1.0을 릴리즈하고 프로덕션 레벨 안정성을 달성한다.

- [ ] LOCO reconnect 안정성 — watch 장시간 연결 유지 (24h+ uptime)
- [ ] Error recovery 전 경로 통합 테스트
- [ ] `cargo clippy` + `cargo test` CI 파이프라인 (GitHub Actions)
- [ ] Homebrew formula 자동 업데이트 (release workflow)
- [ ] CHANGELOG, README 최종 정리
- [ ] Semantic versioning 1.0.0 태그

### G3: Chat DB Decryption — SQLCipher
**Priority: HIGH**

로컬 카카오톡 Chat DB (SQLCipher) 복호화로 전체 메시지 히스토리 접근을 확보한다.

- [ ] SQLCipher key derivation 리버스 엔지니어링
- [ ] macOS KakaoTalk의 DB 파일 위치 및 스키마 분석
- [ ] `decrypt-db` 커맨드 구현
- [ ] 복호화된 메시지를 local cache와 병합

### G4: Developer Experience
**Priority: MEDIUM**

개발자와 AI 에이전트가 카카오톡을 쉽게 자동화할 수 있도록 DX를 강화한다.

- [ ] `--json` 출력 모든 커맨드에 일관 적용
- [ ] `--completion-promise` 전 커맨드 지원 (LLM 에이전트 통합)
- [ ] MCP (Model Context Protocol) 서버 구현 — AI 에이전트가 직접 카카오톡 조작
- [ ] webhook/HTTP callback 모드 (`watch --webhook`)
- [ ] 문서 사이트(fumadocs) API 레퍼런스 자동 생성

### G5: Community & Growth
**Priority: LOW**

오픈소스 커뮤니티를 성장시킨다.

- [ ] CONTRIBUTING.md 정비 및 good-first-issue 라벨링
- [ ] GitHub Discussions 활성화
- [ ] 기술 블로그: LOCO 프로토콜 리버스 엔지니어링 기록
- [ ] Show HN / Reddit 런칭

---

## Founding Engineer — First Tasks

Founding Engineer를 hire한 후 첫 번째로 맡길 작업:

1. **G1 E2E 테스트 자동화** — Christine → 엄마 → 누나 순서로 전체 대화 흐름 검증
2. **CI/CD 파이프라인 구축** — GitHub Actions: cargo test + clippy + release workflow
3. **MCP 서버 프로토타입** — AI 에이전트가 OpenKakao CLI를 도구로 사용할 수 있게
