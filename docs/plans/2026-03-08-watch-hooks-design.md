# Watch Hook Design

## Goal

Add a first-pass hook system to `openkakao-rs watch` so incoming message events can trigger a local command without expanding the trust boundary to external webhooks by default.

## Scope

- Extend `watch` with CLI flags only
- Support a single local command hook via `--hook-cmd`
- Filter hook execution by:
  - `chat_id`
  - `keyword`
  - `message_type`
- Pass event data through:
  - JSON on stdin
  - selected metadata via environment variables
- Default hook failures to log-and-continue
- Add `--hook-fail-fast` to terminate `watch` on hook errors
- Document usage in CLI and automation docs

## Non-goals

- No config file in v1
- No outbound webhook POST in v1
- No autonomous reply system in v1
- No retry queue, persistence, or daemon mode in v1

## Data flow

1. `watch` receives a `MSG` packet
2. Packet is normalized into a structured event JSON object
3. Hook filters are evaluated
4. If matched, the configured command is invoked locally
5. Event JSON is written to stdin and core metadata is exported as env vars
6. Hook result is logged; `watch` continues unless `--hook-fail-fast` is enabled

## CLI shape

- `--hook-cmd <CMD>`
- `--hook-chat-id <ID>` repeatable
- `--hook-keyword <TEXT>` repeatable
- `--hook-type <TYPE>` repeatable
- `--hook-fail-fast`

## Validation

- Add helper-level tests for message rendering / hook filtering / command flag parsing
- Run `cargo test`
- Update docs with examples using shell scripts and `jq`
