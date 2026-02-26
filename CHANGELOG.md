# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-02-26

### Added
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
