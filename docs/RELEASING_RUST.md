# Releasing `openkakao-rs`

이 문서는 `openkakao-rs`의 GitHub Release + Homebrew tap 자동 배포 절차를 정리한다.

## 1. 사전 준비

- 메인 저장소: `JungHoonGhae/kakaotalk-cli` (이 저장소)
- tap 저장소: `JungHoonGhae/homebrew-openkakao`
- GitHub Actions secret 필요:
  - `HOMEBREW_TAP_TOKEN`

## 2. `HOMEBREW_TAP_TOKEN`은 어디서 구하나?

아래 둘 중 하나로 발급 가능하다.

1. Fine-grained PAT (권장)
- GitHub 우상단 프로필 -> `Settings` -> `Developer settings` -> `Personal access tokens` -> `Fine-grained tokens`
- `Generate new token`
- Repository access: `Only select repositories` -> `homebrew-openkakao`
- Permissions:
  - `Contents`: `Read and write`
  - `Metadata`: `Read`
- 만료일 설정 후 토큰 생성

2. Classic PAT
- 동일 경로에서 `Tokens (classic)` -> `Generate new token (classic)`
- scope: `repo`
- 권한이 넓어서 fine-grained보다 비권장

## 3. 토큰을 Actions Secret으로 등록

- 메인 저장소(`kakaotalk-cli`)로 이동
- `Settings` -> `Secrets and variables` -> `Actions`
- `New repository secret`
  - Name: `HOMEBREW_TAP_TOKEN`
  - Secret: 발급한 토큰 문자열

## 4. 릴리스 실행

워크플로는 태그 푸시로 실행된다.

```bash
git tag openkakao-rs-v0.1.0
git push origin openkakao-rs-v0.1.0
```

실행되는 작업:
- macOS ARM64, x86_64 빌드
- GitHub Release 에셋 업로드
- `homebrew-openkakao`의 `Formula/openkakao-rs.rb` 자동 업데이트

## 5. 사용자 설치

```bash
brew tap JungHoonGhae/openkakao
brew install openkakao-rs
```

## 6. 트러블슈팅

- tap 업데이트가 안 되면:
  - `HOMEBREW_TAP_TOKEN` 유효성/만료 여부 확인
  - 토큰이 `homebrew-openkakao`에 실제 쓰기 권한이 있는지 확인
- 릴리스만 되고 tap 미반영이면:
  - Actions 로그에서 `update-homebrew-tap` job 실패 원인 확인
