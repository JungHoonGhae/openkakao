<div align="center">
  <h1>OpenKakao</h1>
  <p>Unofficial CLI for KakaoTalk on macOS.</p>
  <p>It works well as a terminal tool for humans and as a local interface for AI or agent workflows through JSON output, watch mode, hooks, and webhooks.</p>
  <p>The executable name is <code>openkakao-cli</code>.</p>
</div>

<p align="center">
  <a href="#quick-start"><strong>Quick Start</strong></a> ·
  <a href="#highlights"><strong>Highlights</strong></a> ·
  <a href="#docs"><strong>Docs</strong></a> ·
  <a href="#claude-code-skill"><strong>Claude Code Skill</strong></a>
</p>

<p align="center">
  <a href="https://github.com/JungHoonGhae/openkakao-cli/stargazers"><img src="https://img.shields.io/github/stars/JungHoonGhae/openkakao-cli" alt="GitHub stars" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="MIT License" /></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.75+-orange.svg" alt="Rust" /></a>
  <a href="https://openkakao.vercel.app/"><img src="https://img.shields.io/badge/status-deprecated-red" alt="Status Deprecated" /></a>
  <a href="https://openkakao.vercel.app/"><img src="https://img.shields.io/badge/docs-fumadocs-black" alt="Docs" /></a>
</p>

[한국어](README.md) | **English**

> [!CAUTION]
> **Deprecated / maintenance paused (2026-06)** — Due to personal work commitments, this project is not being actively maintained right now. The login issues below may remain unresolved.
>
> Recent KakaoTalk macOS builds broke **most login paths:**
> - `login --save` — newer builds no longer cache the auth token, so it cannot be extracted. ([#15](https://github.com/JungHoonGhae/openkakao-cli/issues/15))
> - `login --manual` — an unseen device gets `status=-100` (device not registered), but the current macOS app has no automated device-registration (passcode) endpoint (it 404s), so login cannot complete. ([#20](https://github.com/JungHoonGhae/openkakao-cli/issues/20), [#22](https://github.com/JungHoonGhae/openkakao-cli/issues/22))
>
> **🚨 Do NOT repeatedly retry login from an unregistered device.** Kakao may block your account's "sub-device login" or restrict the account (this has actually been reported).
>
> For relatively safe, server-free use, stick to the `local-*` commands (local DB, read-only).

> [!WARNING]
> This project is an unofficial CLI and is not affiliated with or endorsed by Kakao Corp. It is built for research, automation, and local workflows around the macOS KakaoTalk app.
> Depending on how you use it, Kakao may interpret that use as a violation of its Terms of Service or operating policies, and your account may be suspended or permanently deleted.
> Review the relevant policies yourself before using it and proceed only if you accept full responsibility for that risk.

<div align="center">
<table>
  <tr>
    <td align="center"><strong>Works with</strong></td>
    <td align="center"><img src="docs/assets/logos/openclaw.svg" width="32" alt="OpenClaw" /><br /><sub>OpenClaw</sub></td>
    <td align="center"><img src="docs/assets/logos/claude.svg" width="32" alt="Claude Code" /><br /><sub>Claude Code</sub></td>
    <td align="center"><img src="docs/assets/logos/codex.svg" width="32" alt="Codex" /><br /><sub>Codex</sub></td>
    <td align="center"><img src="docs/assets/logos/cursor.svg" width="32" alt="Cursor" /><br /><sub>Cursor</sub></td>
    <td align="center"><img src="docs/assets/logos/bash.svg" width="32" alt="Bash" /><br /><sub>Bash</sub></td>
    <td align="center"><img src="docs/assets/logos/http.svg" width="32" alt="HTTP" /><br /><sub>HTTP</sub></td>
  </tr>
</table>
</div>

<p align="center">
  <a href="https://www.star-history.com/?repos=JungHoonGhae%2Fopenkakao&type=date&legend=top-left">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/image?repos=JungHoonGhae/openkakao-cli&type=date&theme=dark&legend=top-left" />
      <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/image?repos=JungHoonGhae/openkakao-cli&type=date&legend=top-left" />
      <img alt="Star History Chart" src="https://api.star-history.com/image?repos=JungHoonGhae/openkakao-cli&type=date&legend=top-left" width="600" />
    </picture>
  </a>
</p>

<p align="center">
  <img src="assets/thumbnail-en.png" alt="openkakao" width="720" />
</p>

## Quick Start

### For Human

```bash
# Homebrew
brew tap JungHoonGhae/openkakao
brew install openkakao-cli

# 1. Save auth data
#    Recent KakaoTalk no longer caches the token, so email+password login is recommended (#15)
openkakao-cli login --manual --save
#    (older builds where cache extraction still works: openkakao-cli login --save)

# 2. List chats
openkakao-cli chats

# 3. Read messages
openkakao-cli read <chat_id> -n 20

# 4. Read from local DB (no server contact)
openkakao-cli local-chats
openkakao-cli local-read <chat_id>

# 5. Send a message (requires allow_loco_write = true in config)
openkakao-cli send <chat_id> "Hello from CLI!"
```

Only force the older cache-backed path when you need it:

```bash
openkakao-cli chats --rest
openkakao-cli read <chat_id> --rest
openkakao-cli members <chat_id> --rest
```

### For Agent

```bash
# Structured output
openkakao-cli --json chats
openkakao-cli --json read <chat_id> -n 20

# Safe local DB reads (no server contact)
openkakao-cli local-chats --json
openkakao-cli local-read <chat_id> --json

# Preview before executing
openkakao-cli send <chat_id> "message" --dry-run --json

# Real-time event stream
openkakao-cli watch --json

# Connect to local hooks or webhooks
openkakao-cli --unattended --allow-watch-side-effects watch \
  --hook-cmd 'jq . > /tmp/openkakao-event.json'
```

To use it directly from Claude Code:

```bash
npx skills add JungHoonGhae/skills@openkakao-cli
```

## Highlights

- Extracts auth data from the macOS KakaoTalk app
- Reads chats, messages, members, friends, and profiles
- Sends messages, watches real-time events, and handles media over LOCO
- Fits well into `jq`, `cron`, SQLite, and LLM workflows through `--json`
- Connects to local automation and agent flows through `watch`, hooks, and webhooks
- Can recover some reads with `friends --local`, `profile --local`, and `profile --chat-id`
- Read safely from local DB with `local-chats`, `local-read`, `local-search` (no server contact)
- Preview any write with `--dry-run` before executing
- Send to memo chat with `send --me` for quick testing
- LOCO write ops disabled by default — opt in with `safety.allow_loco_write = true`

## Where It Fits

- when you want chat history as JSON for downstream tools
- when KakaoTalk should become an input channel for local scripts or operator tools
- when you want to trigger follow-up actions from watch events through hooks or webhooks
- when you want one CLI that works for both direct terminal use and AI-driven local workflows

## Safety Mode

Since v1.1.0, LOCO write operations (send, delete, edit, react) are **disabled by default**.
To protect your account, commands that write to the server require explicit opt-in.

```toml
# ~/.config/openkakao/config.toml
[safety]
allow_loco_write = true
```

Read-only operations are always available:

| Command | Description | Server Contact |
|---------|-------------|----------------|
| `local-chats` | List chats from local DB | None |
| `local-read <id>` | Read messages from local DB | None |
| `local-search "keyword"` | Search local DB | None |
| `chats --rest` | List chats via REST | REST |
| `read <id> --rest` | Read messages via REST | REST |
| `send ... --dry-run` | Preview send without executing | None |

## Requirements

| Requirement | Notes |
|-------------|-------|
| macOS | KakaoTalk desktop app must be installed and logged in |
| Rust >= 1.75 | Only for source builds |

## Installation

### Homebrew

```bash
brew tap JungHoonGhae/openkakao
brew install openkakao-cli
```

### From source

```bash
git clone https://github.com/JungHoonGhae/openkakao-cli.git
cd openkakao/openkakao-cli
cargo install --path .
```

## Docs

- Documentation site: https://openkakao.vercel.app/
- Quick start: https://openkakao.vercel.app/docs/getting-started/quickstart/
- CLI reference: https://openkakao.vercel.app/docs/cli/overview/
- Automation overview: https://openkakao.vercel.app/docs/automation/overview/
- LLM / agent workflows: https://openkakao.vercel.app/docs/automation/llm-agent-workflows/
- Watch patterns: https://openkakao.vercel.app/docs/automation/watch-patterns/
- Protocol docs: https://openkakao.vercel.app/docs/protocol/overview/

Reverse engineering / local app-state diff:

```bash
openkakao-cli profile-hints --local-graph --json
openkakao-cli profile-hints --app-state --json > /tmp/profile-before.json
openkakao-cli profile-hints --app-state --app-state-diff /tmp/profile-before.json --json
```

## Claude Code Skill

```bash
npx skills add JungHoonGhae/skills@openkakao-cli
```

## Development

```bash
cd openkakao-cli
cargo build --release
```

Detailed usage, operational notes, and protocol details live in the docs site.

## Support

If this tool helps you, consider supporting its maintenance:

<a href="https://www.buymeacoffee.com/lucas.ghae">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" height="50">
</a>

## Contributing

Bug reports and PRs are welcome.

## License

MIT
