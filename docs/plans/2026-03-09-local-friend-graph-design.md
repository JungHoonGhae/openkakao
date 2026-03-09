# Local Friend Graph and Profile Hint Correlation

## Goal

Reduce REST dependence for friend/profile inspection before a full global LOCO friend surface is known.

## Approach

1. Build a best-effort local friend graph by:
   - listing known chats through LOCO
   - fetching `GETMEM` for each chat on the same LOCO session
   - merging member/profile fields by `user_id`
2. Expose the graph through user-facing commands:
   - `friends --local`
   - `profile <user_id> --local`
3. Correlate cache-based friend/profile hints with the graph:
   - `profile-hints --local-graph`
   - for each cached request, show matching user ids, candidate chat ids, and access permits

## Why this shape

- `GETMEM` is already proven and safe for read-only use.
- The missing piece is still a true global LOCO friend surface.
- Building a local graph gives immediate value without pretending to have a full friend list.
- The graph also gives better inputs for future `SYNCMAINPF` or related probes.

## Non-goals

- no claim that `friends --local` is authoritative
- no favorite/hidden/phone-number support on the local graph path
- no mutation wiring for `hide/unhide` until semantics are proven

## Success criteria

- `friends --local` returns a merged graph from known chats
- `profile --local` resolves users without requiring REST or a known `chat_id`
- `profile-hints --local-graph` narrows future LOCO friend/profile probes
