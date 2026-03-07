# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] - 2026-03-07

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
