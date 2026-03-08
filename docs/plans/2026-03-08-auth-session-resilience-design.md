# Auth Session Resilience Design

## Goal

Improve long-running unattended reliability for LOCO-backed workflows, especially `watch`, without turning authentication recovery into an unbounded flood of relogin and renewal attempts.

## Scope

This phase only touches:

- LOCO connection recovery
- `watch` reconnect behavior
- persisted recovery state

This phase does not change:

- REST stabilization behavior
- new user-facing auth commands
- webhook/hook semantics

## Recommended Approach

Use bounded resilience with a persisted state file.

- Keep trying to recover by default
- Record recent success and failure context in `~/.config/openkakao/state.json`
- Rate-limit expensive recovery paths like `relogin` and `renew`
- Preserve process-level reconnect behavior, but consult persisted cooldowns before reauth attempts
- Treat `watch` as the first consumer of the new policy model

## Why This Approach

Alternative 1: memory-only runtime counters
- simpler
- but loses context on restart and cannot prevent reauth floods across process restarts

Alternative 2: fail fast after a few auth failures
- safer in a narrow sense
- but fails the unattended reliability goal

Alternative 3: persisted bounded resilience
- preserves long-running behavior across restarts
- keeps recovery attempts explicit and inspectable
- gives room for future REST reuse

This is the recommended approach.

## State File

Location:
- `~/.config/openkakao/state.json`

Initial fields:
- `last_success_at`
- `last_success_transport`
- `last_recovery_source`
- `last_renew_at`
- `last_relogin_at`
- `consecutive_failures`
- `last_failure_kind`
- `last_failure_at`
- `cooldown_until`

This remains a single JSON document for now. Avoid a larger schema until operational needs are clearer.

## Failure Kinds

Use a small normalized set:
- `network`
- `auth_expired`
- `auth_relogin_needed`
- `auth_recovery_exhausted`
- `upstream_change_suspected`
- `unknown`

These are for policy and logging, not a public API guarantee.

## Recovery Model

For `watch` and LOCO connect:

1. attempt connection with current credentials
2. if login succeeds, reset failure counters and persist success
3. if LOCO returns `-950`, consult persisted cooldown and recovery timestamps
4. attempt configured recovery ladder (`relogin` / `renew`) only if allowed by cooldown
5. if recovery succeeds, persist new credentials and success state
6. if recovery fails repeatedly, mark a cooldown window before the next expensive auth attempt
7. continue lower-risk reconnect attempts around the cooldown instead of hammering auth endpoints

## Cooldown Policy

Start simple:

- relogin minimum interval: 5 minutes
- renew minimum interval: 2 minutes
- repeated auth exhaustion increases a shared cooldown up to 30 minutes
- plain socket/network reconnects keep their current short exponential backoff

This creates two layers:
- transport reconnect backoff for transient connection loss
- auth recovery cooldown for expensive reauthentication paths

## Logging

Add structured stderr lines for key events:
- auth success
- auth failure kind
- recovery skipped due to cooldown
- recovery succeeded with source
- cooldown entered

Goal: an operator should understand the recent recovery path from stderr and `state.json` alone.

## Testing

Add tests for:
- default empty state
- cooldown checks for renew and relogin
- state transition after success and failure
- recovery throttling decisions

Prefer pure helper tests over integration-heavy tests in this phase.

## Rollout

Phase 1:
- implement state file
- wire it into LOCO connect and `watch`
- document behavior in CLI/auth docs later if needed

Phase 2:
- reuse the same state model for REST stabilization if it proves useful
