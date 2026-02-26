# Contributing

OpenKakao에 기여해주셔서 감사합니다.

## 시작하기

```bash
git clone https://github.com/JungHoonGhae/kakaotalk-cli.git
cd kakaotalk-cli
pip install -e .
```

## 개발 환경

| 도구 | 버전 |
|------|------|
| Python | >= 3.11 |
| pip | 최신 |

## 브랜치 전략

- `main` — 안정 릴리스
- `dev` — 개발 브랜치
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

1. `dev` 브랜치에서 feature 브랜치를 생성합니다
2. 변경 사항을 커밋합니다
3. `dev` 브랜치로 PR을 생성합니다
4. PR 템플릿을 채워주세요

## 코드 스타일

- Python 표준 라이브러리를 우선 사용 (외부 의존성 최소화)
- 타입 힌트 사용 (`str`, `int`, `list[str]` 등)
- docstring은 간결하게 (한 줄 설명)

## 주의사항

- **절대** 실제 토큰, 사용자 ID, 개인정보를 커밋하지 마세요
- credentials.json, .env 파일은 .gitignore에 포함되어 있습니다
- 카카오 서버에 과도한 요청을 보내는 코드를 작성하지 마세요

## 이슈

버그 리포트나 기능 요청은 [Issues](https://github.com/JungHoonGhae/kakaotalk-cli/issues)에 등록해주세요.
