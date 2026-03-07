# Watch Hook Design

## Goal

Add a first-pass hook system to `openkakao-rs watch` so incoming message events can trigger a local command without expanding the trust boundary to external webhooks by default.

## Scope

- Extend `watch` with CLI flags only
- Support a single local command hook via `--hook-cmd`
- Support a single webhook sink via `--webhook-url`
- Support repeated webhook headers via `--webhook-header`
- Support optional webhook signing via `--webhook-signing-secret`
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
5. If configured, event JSON is written to stdin for the local command
6. If configured, the same event JSON is POSTed to the webhook URL
7. If signing is enabled, the request includes:
   - `X-OpenKakao-Timestamp`
   - `X-OpenKakao-Signature`
   and signs `timestamp.payload` with HMAC-SHA256
8. Hook result is logged; `watch` continues unless `--hook-fail-fast` is enabled

## CLI shape

- `--hook-cmd <CMD>`
- `--webhook-url <URL>`
- `--webhook-header "Name: Value"` repeatable
- `--webhook-signing-secret <SECRET>`
- `--hook-chat-id <ID>` repeatable
- `--hook-keyword <TEXT>` repeatable
- `--hook-type <TYPE>` repeatable
- `--hook-fail-fast`

## Validation

- Add helper-level tests for message rendering / hook filtering / command flag parsing
- Run `cargo test`
- Update docs with examples using shell scripts and `jq`
