# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
