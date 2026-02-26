# OpenKakao Rust Rewrite + Deployment Design

- Date: 2026-02-26
- Scope: Python CLI parity rewrite in Rust + deployment automation
- Status: Approved

## Goals

1. Rewrite current OpenKakao CLI behavior in Rust with feature parity.
2. Ship macOS binaries for both Apple Silicon and Intel.
3. Automate GitHub Releases and Homebrew tap updates.
4. Keep Python version available during transition.

## Confirmed Decisions

- Migration mode: staged transition (parallel release now, Rust default later)
- Initial Rust binary name: `openkakao-rs`
- Deployment targets: `aarch64-apple-darwin` + `x86_64-apple-darwin`
- Distribution channels: GitHub Releases + separate Homebrew tap repository
- Homebrew tap repo strategy: separate repository (recommended)

## Architecture

- New crate: `openkakao-rs/`
- Modules:
  - `auth`: read OAuth token and metadata from KakaoTalk macOS `Cache.db`
  - `credentials`: load/save `~/.config/openkakao/credentials.json` (mode `0600`)
  - `rest`: clients for `katalk.kakao.com` and `talk-pilsner.kakao.com`
  - `model`: typed structures for friends/chats/messages/profile members
  - `cli` (`main.rs`): subcommands and command orchestration
- Runtime: synchronous CLI execution (`reqwest::blocking`)

## Feature Parity (Phase 1)

- `auth`: token status check
- `login --save`: extract credentials and save locally
- `me`: profile info
- `friends`: list/filter/search
- `chats`: list chats / unread / pagination-all
- `read`: chat messages with `--count`, `--before`
- `members`: chat room members
- `settings`: account settings
- `scrap`: link preview

Out of scope for this phase:
- LOCO messaging/login fixes
- message sending

## Error Handling and Validation

- Unified error handling via `anyhow` (context-rich internal errors)
- User-friendly CLI messages for expected failures (missing token, expired token, etc.)
- Test strategy:
  - unit-level parsing and formatting tests (next phase)
  - optional e2e tests behind environment flags

## Deployment Plan

### GitHub Releases

- Workflow trigger: git tag `openkakao-rs-v*`
- Build jobs:
  - `macos-14` -> `aarch64-apple-darwin`
  - `macos-13` -> `x86_64-apple-darwin`
- Artifacts:
  - `openkakao-rs-aarch64-apple-darwin.tar.gz`
  - `openkakao-rs-x86_64-apple-darwin.tar.gz`
  - matching `.sha256` files
- Release upload: automated via GitHub Actions

### Homebrew Tap

- Separate tap repo: `JungHoonGhae/homebrew-openkakao`
- Workflow updates `Formula/openkakao-rs.rb` with:
  - release asset URLs
  - SHA256 for both architectures
- Formula install command:
  - `brew tap JungHoonGhae/openkakao`
  - `brew install openkakao-rs`

## Transition Plan

1. Release Rust binary in parallel with Python package.
2. Collect usage and compatibility feedback.
3. Switch default docs/install path to Rust in next minor release.
4. Keep Python variant as fallback during grace period.

## Risks and Mitigations

- Risk: API response shape drift
  - Mitigation: defensive JSON parsing and graceful fallback output
- Risk: token extraction breakage due app updates
  - Mitigation: keep manual credential input fallback
- Risk: release/tap drift
  - Mitigation: single release workflow updates both channels from same version tag
