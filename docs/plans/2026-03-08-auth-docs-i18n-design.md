---
title: Auth Policy, Transport Boundary, and Korean Docs Completion
date: 2026-03-08
---

# Goal

Finish the current documentation and operations milestone without expanding product scope.

The work is limited to three outcomes:

1. wire existing auth policy config into real recovery behavior
2. document the REST vs LOCO boundary clearly enough for operator decisions
3. make the Korean docs path coherent for first-time users

# Chosen Approach

## Auth policy

Keep the current recovery mechanisms and only make their order policy-driven.

- `auth.prefer_relogin = true` keeps `relogin` before `renew`
- `auth.prefer_relogin = false` tries `renew` before `relogin`
- `auth.auto_renew = false` removes renewal attempts from the ladder

This preserves the existing operational model while making unattended behavior explicit and testable.

## Transport boundary

Do not invent a new abstraction layer. Document the practical split directly in operator docs:

- REST for lightweight account checks and recently cached data
- LOCO for authoritative chat access, sending, watch, and media workflows

Add one dedicated page plus cross-links from auth, quickstart, and CLI overview.

## Korean docs

Prefer complete translation of the highest-traffic pages over shallow translation of every page.

Priority pages:

- overview
- getting started
- auth/configuration
- chat/message/watch CLI guides
- troubleshooting
- automation overview

Also fix `/ko` internal links so the locale does not silently bounce back to English docs.

# Validation

- Rust: `cargo fmt`, `cargo test`, `cargo clippy -- -D warnings`
- Docs: `pnpm build`
- Content QA: grep for stale `/docs/` links under `docs-ko`
