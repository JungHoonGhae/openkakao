//! `ax-watch` — login-free receive detection. Polls KakaoTalk's chat list via
//! the macOS Accessibility API and fires the existing hook/webhook machinery
//! when a chat's unread count increases. No server contact (no ban risk),
//! background (never steals focus), non-intrusive (never opens a chat, so
//! unread state is untouched). Replaces the LOCO-based `watch`, which needs a
//! server session that recent KakaoTalk builds break.

/// Decide whether a chat-list row should fire an event this poll.
///
/// - The first poll (`first == true`) only records a baseline and never fires,
///   so pre-existing unread messages don't flood on startup.
/// - Afterwards, fire when the unread count rose above the previous value. A
///   row not seen before (`prev == None`) counts as previously 0, so a chat
///   that appears with unread (e.g. a new message bumped a formerly off-screen
///   chat to the top) still fires.
#[allow(dead_code)] // Called by Task 4's poll loop
pub fn should_emit(prev: Option<i32>, cur: i32, first: bool) -> bool {
    !first && cur > prev.unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_poll_never_emits() {
        assert!(!should_emit(None, 5, true));
        assert!(!should_emit(Some(0), 5, true));
    }

    #[test]
    fn emits_when_unread_increases() {
        assert!(should_emit(Some(0), 3, false));
        assert!(should_emit(Some(2), 5, false));
    }

    #[test]
    fn no_emit_when_unread_same_or_decreases() {
        assert!(!should_emit(Some(3), 3, false));
        assert!(!should_emit(Some(5), 2, false));
    }

    #[test]
    fn newly_seen_chat_with_unread_emits() {
        // prev None (first time this chat appears in the list) on a non-first
        // poll: a real incoming message that bumped the chat into view.
        assert!(should_emit(None, 1, false));
    }

    #[test]
    fn newly_seen_chat_without_unread_does_not_emit() {
        assert!(!should_emit(None, 0, false));
    }
}
