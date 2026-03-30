# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.1.0] - 2026-03-30

### Added
- **Local DB reading (SQLCipher)**: `local-chats`, `local-read`, `local-search`, `local-schema` commands read the encrypted KakaoTalk database directly — zero server contact, zero ban risk
- **`--dry-run` flag**: preview send, delete, edit, react actions without executing (supports `--json`)
- **`send --me`**: send to memo chat (나와의 채팅) without specifying chat_id — useful for testing
- **`safety.allow_loco_write` config**: LOCO write operations (send, delete, edit, react) are now disabled by default to protect accounts from bans; opt-in via `~/.config/openkakao/config.toml`
- **Doctor: local DB checks**: `doctor` now verifies SQLCipher database access (UUID, userId, file, decryption) and LOCO write status
- **AGENTS.md**: AI agent integration guide with safe/risky command classification

### Changed
- `rusqlite` switched from `bundled` to `bundled-sqlcipher` for SQLCipher support
- LOCO write commands now require explicit `safety.allow_loco_write = true` in config (breaking change for existing automation — add the config field to restore previous behavior)

## [1.0.0] - 2026-03-11

### Added
- Stable release of openkakao-rs — all LOCO and REST features production-ready

### Changed
- Version bumped from 0.9.4 to 1.0.0 (stable)

---

## [0.9.4] - 2026-03-11

### Added
- `watch --reconnect-delay <sec>`: configurable initial backoff delay (default 2s, doubles each attempt)
- `watch --reconnect-max-delay <sec>`: configurable max backoff cap (default 60s)
- `--json` watch mode emits NDJSON `{"type":"reconnecting","attempt":N,"delay_secs":D,"reason":"..."}` events on reconnect
- `rest_token` field support for pilsner REST endpoint authentication

### Changed
- `watch --max-reconnect` default changed from 5 to 10

### Tests
- JSON output parsing tests for `doctor`, `auth-status`, `cache-stats` commands

## [0.9.3] - 2026-03-11

### Added
- Local SQLite message cache (`~/.config/openkakao/messages.db`) integrated into `watch` and `read`
- `watch` persists incoming MSG/SYNCMSG payloads to local cache on receive
- `read` merges local cache with LOCO-fetched messages (deduped by logId), back-fills cache with new results
- `MessageDb::get_messages(chat_id, limit)` for ordered, paginated local retrieval

## [0.9.2] - 2026-03-10

### Added
- `edit <chat_id> <log_id> <message>` — edit messages via LOCO REWRITE (returns -203 on macOS dtype=2, Android dtype=1 only)
- `--completion-promise` global flag — prints `[DONE]` to stdout after successful command completion (LLM agent integration)

## [0.9.1] - 2026-03-10

### Added
- `react <chat_id> <log_id>` — add reaction via LOCO ACTION (type=1 = like; only type supported on macOS dtype=2)
- SYNCACTION push handler in `watch` for real-time reaction events from other users

## [0.9.0] - 2026-03-10

### Fixed
- cargo fmt formatting fixes for CI lint compliance

## [0.8.0] - 2026-03-10

### Added
- `delete <chat_id> <log_id>` — delete a message via LOCO DELETEMSG (creates feedType:14 deletion marker; `-y` to skip confirm, `--force` for open chats)
- `mark-read <chat_id> <log_id>` — mark messages as read via LOCO NOTIREAD (fire-and-forget)
- `watch --capture` flag for protocol packet capture to `capture.jsonl` (reverse engineering tool)
- SYNCDLMSG and SYNCREWR push handlers in `watch` for protocol reverse engineering
- `--json` output flag for `chats`, `read`, `members`, `friends`, `me`, `doctor` commands
- Streaming output for `read` command (progressive display while fetching)
- FTS5 full-text search for local message cache
- Async hook execution in `watch`

### Changed
- LOCO connection stability improvements: retry logic, backoff, CHANGESVR handling
- Security hardening: token log truncation, filename sanitization, URL domain allowlist, credential file permissions (0o600), LOCO frame size limits
- Code quality: split large functions into option structs, modular command architecture
- `read` output is now streamed progressively instead of buffered

## [0.7.2] - 2026-03-10

### Changed
- Unified Homebrew distribution to tap-only (`JungHoonGhae/homebrew-openkakao`)
- Release workflow updated to use `v*` tag pattern for automatic tap formula updates

## [0.7.1] - 2026-03-10

### Added
- `read` now includes messages received from others via LOGINLIST chatLog sync (`chatDatas[].l`)
- Cache.db-free auto-relogin via `email_cmd` config option + 3-tier fallback (saved → Doppler → Cache.db)

### Fixed
- Clarified watch state persistence: only saved on Ctrl-C (SIGINT), not SIGTERM
- MemoChat visibility scoped to default LOCO LCHATLIST path (does not appear in standard list)

## [0.7.0] - 2026-03-09

### Changed
- Polished error model across all commands with consistent exit codes
- Output consistency improvements across LOCO and REST commands

## [0.6.0] - 2026-03-09

### Added
- `lib.rs` and integration test infrastructure (`tests/loco_crypto_test.rs`, `tests/loco_packet_test.rs`, `tests/message_db_test.rs`)
- `OpenKakaoError` type with retryable distinction and `check_loco_status` helper
- Watch reconnect resilience: exponential jitter, SYNCMSG cursor resume on reconnect
- Rich message type rendering: photo, video, file, multi-photo attachment display
- CI: parallel test/lint/build-macos jobs with caching

### Changed
- Extracted all commands from `main.rs` into dedicated `src/commands/` modules (analytics, auth, chats, doctor, download, members, probe, read, rest, send, watch)
- `main.rs` reduced by ~2200 lines

## [0.5.0] - 2026-03-09

### Added
- `stats <chat_id>` — chat analytics (message counts, hourly activity histogram, top senders)
- `cache` / `cache-search` / `cache-stats` — local SQLite message cache with full-text search
- `config.example.toml` — documented example configuration file
- Homebrew formula (`Formula/openkakao.rb`) for macOS distribution
- `media.rs` — media type detection, image dimension parsing, download helpers
- `message_db.rs` — SQLite local message cache with upsert, search, sync cursor tracking
- `util.rs` — shared BSON helpers, formatting, chat type helpers, message rendering, validation

### Changed
- Modularized codebase: extracted commands into `src/commands/` (send, watch, doctor, download, analytics)
- Reduced `main.rs` by ~2200 lines (28% smaller)
- `profile-hints` now carries per-chat `GETMEM` tokens through the local graph and surfaces them as additional `SYNCMAINPF` / `UPLINKPROF` probe candidates
- user-targeted local graph lookups (`profile --local`, `profile-hints --local-graph --user-id`) now prefer chat IDs inferred from cached profile hints before scanning the full LOCO graph

### Fixed
- `GETMEM`-backed local graph and profile lookups now retry through LOCO reconnects on transient `early eof` / socket reset failures

## [0.4.3] - 2026-03-09

### Added
- `auth.password_cmd` for unattended relogin via external secret commands such as Doppler
- `auth-status` persisted recovery-state inspection
- `probe` for raw LOCO method inspection
- `profile-hints` for cached profile and revision hint inspection during LOCO reverse engineering
- `loco-blocked` for LOCO-backed block or hidden-style member inspection
- `friends --local` for a LOCO-derived partial friend graph built from known chats
- `profile --local` and `profile --chat-id <chat_id>` for LOCO-backed profile reads when REST profile paths are unhealthy
- `members --full` for richer chat-scoped GETMEM member data
- `profile-hints --app-state` and `--app-state-diff` for before/after KakaoTalk app-state snapshot comparison
- local-graph GETMEM chat metadata in `profile-hints`, including per-chat request tokens and member counts

### Changed
- `read` is now LOCO-first by default, with `--rest` for the older cache-backed path
- `chats` is now LOCO-first by default, with `--rest` for the older cache-backed path
- `members` is now LOCO-first by default, with `--rest` for the older REST member list
- `chatinfo` is now the primary room-info command; `loco-chatinfo` remains as a hidden compatibility alias
- `doctor --loco` is now the documented LOCO connectivity check; `loco-test` remains as a hidden compatibility alias
- auth recovery now uses explicit step outcomes (`unavailable`, `failed`, `recovered`) instead of implicit optional results

### Fixed
- preserved recovery fallback order when `password_cmd` is configured
- prevented missing relogin passwords from aborting the full auth recovery ladder
- avoided Tokio runtime panic during LOCO auth recovery

## [0.4.2] - 2026-03-08

### Fixed
- Homebrew-installed `openkakao-rs` now ships the same `send` CLI surface as `main`, including the default outgoing prefix behavior and `--no-prefix` / `-y` flags.

### Tests
- Added regression coverage for outgoing message prefix formatting and `send` flag parsing.

## [0.4.1] - 2026-03-07

### Security
- `loco_oneshot` TLS/Legacy 경로에 `MAX_FRAME_SIZE` 검증 추가 (악성 서버 OOM 방지)
- multi-frame 재조립 루프에 `total_needed` 상한 검증 추가
- 패스워드 로그 출력 제거 (기존: 앞 10자 노출 → 변경: 길이만 표시)
- 토큰 로그 prefix를 40자 → 8자로 축소
- 다운로드 파일명에 `sanitize_filename()` 적용 (path traversal 방지)
- 미디어 다운로드 URL 도메인 allowlist 검증 (`.kakao.com`, `.kakaocdn.net`만 허용)
- `email`, `refresh_token` 파라미터에 URL 인코딩 적용 (form body injection 방지)
- LOCO 서버 응답의 `port` 값 범위 검증 (`1~65535`)
- LOCO 패킷 `body_length`에 `MAX_BODY_SIZE` (100MB) 상한 체크 추가
- AES-GCM 프레임 수신에 `MAX_FRAME_SIZE` 검증 추가
- DER 파서에 bounds check 추가 (OOB read 방지)
- JPEG 파서에 `len < 2` 체크 추가 (무한루프 방지)
- credential 파일을 `OpenOptions::mode(0o600)` 으로 생성 (TOCTOU 제거)

### Added
- `send-file <chat_id> <file>` — LOCO SHIP+POST로 미디어/파일 전송 (사진/동영상/파일, 자동 타입 감지)
- `send-photo` — `send-file`의 alias
- `doctor`에 버전 드리프트 경고 — 설치된 KakaoTalk 버전과 저장된 credentials 버전 불일치 감지
- `watch --read-receipt` — 수신 메시지에 NOTIREAD 읽음 처리 전송
- `watch --max-reconnect N` — 연결 끊김 시 자동 재연결 (기본 5회, exponential backoff, CHANGESVR 대응)
- `watch --download-media [--download-dir DIR]` — 미디어 메시지 자동 다운로드 (사진/동영상/음성/이모티콘/파일)
- `download <chat_id> <log_id> [-o DIR]` — 특정 메시지의 미디어 첨부파일 다운로드
- `relogin --email` — 저장된 이메일 대신 직접 지정

## [0.3.0] - 2026-03-07

### Added
- `doctor [--loco]` — 설치 상태/토큰/연결 진단 커맨드
- `send` 커맨드에 `--yes`/`-y` 플래그 (확인 프롬프트 생략)
- `loco-read` 커맨드에 `--delay-ms`, `--force`, `--since`, `--cursor` 옵션
- `read` 커맨드에 `--before`, `--cursor`, `--since`, `--all` 페이지네이션 옵션
- `relogin --password` 옵션 (캐시된 비밀번호 대신 직접 입력)
- 오픈챗 안전장치 — `send`, `loco-read`에서 오픈챗 접근 시 `--force` 필수
- `loco-read --all`로 서버 보관 전체 히스토리 조회 (SYNCMSG 페이지네이션)
- `loco-chatinfo <chat_id>` — LOCO 채팅방 상세 정보

### Changed
- LOCO 암호화를 AES-128-CFB (encrypt_type=2) → **AES-128-GCM** (encrypt_type=3)으로 마이그레이션
- LOCO 인증에 login.json access_token (65자) 사용 — Cache.db REST 토큰(138자) 대신
- Cache.db 의존성 제거 — LOCO 커맨드는 더 이상 Cache.db에 접근하지 않음
- -950 토큰 만료 시 자동 재로그인 시도

### Removed
- **Python CLI 제거** (`openkakao/` 디렉토리, `pyproject.toml`, `login_test.py`, `refresh_and_login.py`, `test_connection.py`)
  — Rust CLI (`openkakao-rs`)가 모든 기능을 대체

## [0.2.0-beta] - 2026-03-04

### Added (openkakao-rs)
- `send <chat_id> "메시지"` — LOCO WRITE로 메시지 전송
- `watch [--chat-id ID] [--raw]` — 실시간 메시지 수신
- `loco-read <chat_id> [-n count] [--all]` — SYNCMSG 기반 채팅 히스토리 조회
- `loco-chats [--all]` — LOCO LCHATLIST로 채팅방 목록 조회
- `loco-members <chat_id>` — 채팅방 멤버 조회
- `relogin [--fresh-xvc]` — login.json + X-VC로 토큰 자동 갱신
- Homebrew formula (`brew install openkakao-rs`)

### Fixed
- LOCO LOGINLIST -950 해결 (login.json으로 fresh access_token 발급)
- SYNCMSG pagination 안정화 (cnt=50, max 필수)

## [0.2.0] - 2026-02-26

### Added (openkakao — Python, 현재 제거됨)
- `openkakao chats` — 채팅방 목록 조회 (pilsner REST API)
- `openkakao read <chat_id>` — 메시지 읽기 (페이징 지원)
- `openkakao members <chat_id>` — 채팅방 멤버 조회
- `openkakao scrap <url>` — 링크 프리뷰
- `openkakao friends --hidden` — 숨긴 친구 표시 옵션
- `openkakao chats --unread` — 안 읽은 채팅방 필터
- `openkakao chats --all` — 전체 채팅방 페이징 조회
- REST API: `get_chats()`, `get_all_chats()`, `get_messages()`, `get_chat_members()`
- REST API: `add_favorite()`, `remove_favorite()`, `hide_friend()`, `unhide_friend()`
- REST API: `get_friend_profile()`, `get_profiles()`, `get_scrap_preview()`
- talk-pilsner.kakao.com 엔드포인트 발견 및 통합
- CLAUDE.md 에이전트 핸드오프 문서
- docs/TECHNICAL_REFERENCE.md 기술 레퍼런스

### Changed
- 버전 0.1.0 → 0.2.0
- MyProfile 데이터클래스에 `profile_image_url`, `background_image_url` 필드 추가
- `_request()` 메서드가 GET 요청 시 body를 전송하지 않도록 수정

## [0.1.0] - 2026-02-26

### Added
- 초기 릴리스
- `openkakao auth` — 토큰 상태 확인
- `openkakao login --save` — macOS 캐시에서 인증 정보 추출
- `openkakao me` — 내 프로필 보기
- `openkakao friends` — 친구 목록 (즐겨찾기/검색 지원)
- `openkakao settings` — 계정 설정
- OAuth 토큰 자동 추출 (NSURLCache/Cache.db)
- LOCO 프로토콜 구현 (CHECKIN 성공, LOGINLIST -950 블로커)
- RSA-2048 OAEP(SHA-1) + AES-128-CFB 암호화
- BSON 패킷 인코더/디코더
