# Roadmap

## North star: local DB tailing (replaces AX polling for receive detection)

`ax-watch` today polls the KakaoTalk chat list via the macOS Accessibility API
and infers "a new message arrived" from list-level signals (unread count,
message preview). That is a lossy proxy with known limits: it needs the main
window open/visible on the active Space, only sees loaded chat rows, and can't
cleanly tell your own sends from incoming messages.

The complete long-term answer is to stop inferring and read the actual message
stream from KakaoTalk's local SQLCipher DB, which stores every message with a
monotonic `log_id`:

- A watcher tails `log_id > last_seen` and emits exact message events
  (author, text, timestamp, chat_id) — no unread/preview heuristics.
- Works for **all** chats regardless of window state (closed, minimized, on
  another Space) — the window-visibility requirement disappears entirely.
- Distinguishes self vs. incoming perfectly; no false positives.
- Read-only and local: no server contact, no ban risk, no focus stealing.

When this lands, `ax-watch` becomes a fallback rather than the primary path.

**Blocker:** the DB key-derivation formula changed in recent KakaoTalk macOS
builds (observed on 26.5.0 — the derived filename/key no longer matches
on-disk), so decryption currently fails even though userId and device UUID are
recovered. Reaching this north star requires reverse-engineering the current
key derivation, and it may need redoing on future KakaoTalk builds.

## If the DB stays sealed: message-bubble diffing (AX, Tier 2)

The correct AX-only way to also catch messages in a chat you keep open (where
the unread count stays 0) is to track the actual message **bubbles** in each
open window and emit on genuinely new bubbles — distinguishing your own sends
by bubble alignment/author. This is deliberately *not* the preview-text-change
heuristic (which fires on your own sends and any room activity). It only works
for open windows, so it complements list-level unread for closed chats. Build
this only when real usage hits the open-room gap — until then, YAGNI.
