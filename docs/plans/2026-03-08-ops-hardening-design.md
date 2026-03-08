# 2026-03-08 Ops Hardening Design

## Goal

Finish the highest-priority operator work before pausing feature expansion:

1. expose auth recovery state in `doctor` and stderr logs
2. validate a real launchd-backed unattended service path
3. add safety guards around unattended send, hooks, and webhooks

## Scope

Included:

- persisted recovery state in `doctor --json`
- human-readable recovery and safety summaries in `doctor`
- auth recovery state logging in REST and LOCO recovery paths
- config-backed guardrails for unattended send, `watch --hook-cmd`, and `watch --webhook-url`
- launchd example assets and a smoke validation script

Excluded:

- multi-day soak testing
- new transports
- queue semantics for watch hooks or webhooks

## Approach

### Recovery visibility

Keep `state.json` as the source of truth and surface it in two places:

- `auth-status` for direct operator inspection
- `doctor` for broader diagnostics

The JSON form of `doctor` now returns:

- `checks`
- `recovery_state`
- `safety_state`

### Safety guards

Add config-backed defaults instead of one-off flags:

- `safety.min_unattended_send_interval_secs`
- `safety.min_hook_interval_secs`
- `safety.min_webhook_interval_secs`
- `safety.hook_timeout_secs`
- `safety.webhook_timeout_secs`
- `safety.allow_insecure_webhooks`

These guards should be narrow and predictable:

- unattended sends are rate-limited
- local hooks are rate-limited and timed out
- webhooks are rate-limited and timed out
- remote webhooks default to HTTPS only

### launchd validation

Treat launchd as an operator concern, not a docs-only example:

- check in reusable example assets
- add a smoke script that really bootstraps a LaunchAgent
- prefer validating `watch`
- fall back to `watch-cache` only if auth is not ready

## Success criteria

- `doctor` shows recovery and safety context
- auth recovery stderr logs include state summaries
- unattended send bursts are blocked by policy
- hooks and webhooks cannot silently run forever
- insecure remote webhooks are blocked by default
- a real launchd smoke run reaches `state = running`
