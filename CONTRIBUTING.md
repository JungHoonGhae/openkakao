# Contributing

OpenKakao에 기여해주셔서 감사합니다.

## 시작하기

```bash
git clone https://github.com/JungHoonGhae/openkakao.git
cd openkakao/openkakao-rs
cargo build
```

## 개발 환경

| 도구 | 버전 |
|------|------|
| Rust | >= 1.75 |
| macOS | KakaoTalk 데스크탑 앱 설치 및 로그인 |

## 브랜치 전략

- `main` — 안정 릴리스
- `feature/*` — 기능 브랜치
- `fix/*` — 버그 수정

## 커밋 컨벤션

[Conventional Commits](https://www.conventionalcommits.org/) 형식을 따릅니다:

```
feat: 새 기능 추가
fix: 버그 수정
docs: 문서 변경
refactor: 코드 리팩토링
test: 테스트 추가/수정
chore: 빌드/도구 변경
```

예시:

```
feat: add chat room search by name
fix: handle expired token gracefully
docs: update API endpoint documentation
```

## Pull Request

1. `main` 브랜치에서 feature 브랜치를 생성합니다
2. 변경 사항을 커밋합니다
3. `main` 브랜치로 PR을 생성합니다
4. PR 템플릿을 채워주세요

## 코드 스타일

- `cargo fmt` — 포매팅
- `cargo clippy` — 린트
- 외부 의존성 추가 시 최소화

## 주의사항

- **절대** 실제 토큰, 사용자 ID, 개인정보를 커밋하지 마세요
- credentials.json, .env 파일은 .gitignore에 포함되어 있습니다
- 카카오 서버에 과도한 요청을 보내는 코드를 작성하지 마세요

## 이슈

버그 리포트나 기능 요청은 [Issues](https://github.com/JungHoonGhae/openkakao/issues)에 등록해주세요.
