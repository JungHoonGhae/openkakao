# AI Agent Integration Guide

openkakao-rs is designed for AI agent integration. All commands support `--json` for structured output.

## Safety Model

LOCO write operations (send, delete, edit, react) are **disabled by default** to prevent account bans.

### Safe commands (always available, no server contact)

```bash
# Read chats from local KakaoTalk database (SQLCipher, no network)
openkakao-rs local-chats --json
openkakao-rs local-read <chat_id> -n 30 --json
openkakao-rs local-search "keyword" --json
openkakao-rs local-schema

# Preview actions without executing
openkakao-rs send 123 "message" --dry-run --json
openkakao-rs delete 123 456 --dry-run --json
```

### Safe commands (REST API, lower risk)

```bash
openkakao-rs chats --json
openkakao-rs read <chat_id> --rest --json
openkakao-rs friends --json
openkakao-rs me --json
openkakao-rs doctor --json
```

### Risky commands (require opt-in)

These require `allow_loco_write = true` in `~/.config/openkakao/config.toml`:

```bash
openkakao-rs send <chat_id> "message" -y --json
openkakao-rs send --me "test" -y --json    # Send to memo chat
openkakao-rs delete <chat_id> <log_id> -y --json
openkakao-rs edit <chat_id> <log_id> "new" -y --json
openkakao-rs react <chat_id> <log_id> --json
```

## Unattended Mode

For fully non-interactive operation:

```bash
openkakao-rs --unattended --allow-non-interactive-send send <chat_id> "msg" -y --json
```

Or configure in `~/.config/openkakao/config.toml`:

```toml
[mode]
unattended = true

[send]
allow_non_interactive = true

[safety]
allow_loco_write = true
min_unattended_send_interval_secs = 10
```

## Recommended Agent Workflow

1. **Read** with `local-chats` / `local-read` (zero risk)
2. **Preview** with `--dry-run` before any write
3. **Execute** only after user confirmation
4. **Prefer** `--me` flag for testing sends

## JSON Output

All commands with `--json` return structured JSON to stdout. Diagnostic messages go to stderr.

```bash
# List chats
openkakao-rs local-chats --json
# Returns: [{"chat_id": 123, "chat_type": 0, "chat_name": "...", ...}]

# Read messages
openkakao-rs local-read 123 --json
# Returns: [{"log_id": 456, "chat_id": 123, "sender_name": "...", "message": "...", ...}]

# Dry-run
openkakao-rs send 123 "hello" --dry-run --json
# Returns: {"dry_run": true, "action": "send", "chat_id": 123, "message": "..."}
```

## Diagnostics

```bash
openkakao-rs doctor --json        # Check installation, credentials, local DB access
openkakao-rs auth-status --json   # Check auth recovery state
```
