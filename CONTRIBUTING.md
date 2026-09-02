# Contributing

OpenKakao에 기여해주셔서 감사합니다.

## 시작하기

```bash
git clone https://github.com/JungHoonGhae/openkakao-cli.git
cd openkakao-cli
cargo build
```

## 개발 환경

| 도구 | 버전 |
|------|------|
| Rust | `rust-toolchain.toml`이 관리 (현재 1.95.0 pin) |
| macOS | KakaoTalk 데스크탑 앱 설치 및 로그인 |

`rustup`이 설치되어 있으면 레포 디렉토리에서 `cargo` 명령을 처음 실행할 때 pin된 버전이 자동으로 다운로드·활성화됩니다. 시스템 rustc를 수동으로 맞출 필요는 없습니다.

Rust 버전을 올리려면 `rust-toolchain.toml`의 `channel`만 수정해 PR을 올리면 CI와 로컬이 동시에 움직입니다.

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

## Push 전 체크리스트

CI와 동일한 게이트를 로컬에서 먼저 돌려 주세요. `main`과 릴리스 워크플로우 모두 이 세 명령이 통과해야 진행됩니다.

```bash
cargo fmt --manifest-path Cargo.toml --check
cargo clippy --manifest-path Cargo.toml -- -D warnings
cargo test --manifest-path Cargo.toml
```

## 릴리스 절차

1. `CHANGELOG.md`의 `[Unreleased]` 섹션을 새 버전 섹션으로 정리
2. `Cargo.toml`의 `version` 필드 bump (+ `cargo update -p openkakao-cli`로 `Cargo.lock` 반영)
3. Push 전 체크리스트 통과 확인
4. `main`에 커밋·푸시 후 `git tag vX.Y.Z && git push origin vX.Y.Z`
5. 릴리스 워크플로우의 `verify` job이 통과해야 빌드·Homebrew tap 업데이트가 진행됨 — `verify`가 빨갛게 나면 태그만 남고 release는 만들어지지 않으므로, fix 후 버전을 한 단계 올려 재태그할 것 (태그 force-move 금지: v1.1.0 인시던트의 원인)

## 주의사항

- **절대** 실제 토큰, 사용자 ID, 개인정보를 커밋하지 마세요
- credentials.json, .env 파일은 .gitignore에 포함되어 있습니다
- 카카오 서버에 과도한 요청을 보내는 코드를 작성하지 마세요

## 이슈

버그 리포트나 기능 요청은 [Issues](https://github.com/JungHoonGhae/openkakao-cli/issues)에 등록해주세요.
