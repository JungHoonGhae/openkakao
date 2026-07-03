//! `ax-watch` — login-free receive detection. Polls KakaoTalk's chat list via
//! the macOS Accessibility API and fires the existing hook/webhook machinery
//! when a chat's unread count increases. No server contact (no ban risk),
//! background (never steals focus), non-intrusive (never opens a chat, so
//! unread state is untouched). Replaces the LOCO-based `watch`, which needs a
//! server session that recent KakaoTalk builds break.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;

use crate::ax_send;
use crate::commands::watch::{
    parse_webhook_header, run_watch_command_hook_async, run_watch_webhook, validate_webhook_url,
    watch_hook_matches, WatchHookConfig, WatchMessageEvent, WebhookFormat,
};
use crate::util::require_permission;

/// Decide whether a chat-list row should fire an event this poll.
///
/// - The first poll (`first == true`) only records a baseline and never fires,
///   so pre-existing unread messages don't flood on startup.
/// - Afterwards, fire when the unread count rose above the previous value. A
///   row not seen before (`prev == None`) counts as previously 0, so a chat
///   that appears with unread (e.g. a new message bumped a formerly off-screen
///   chat to the top) still fires.
pub fn should_emit(prev: Option<i32>, cur: i32, first: bool) -> bool {
    !first && cur > prev.unwrap_or(0)
}

pub struct AxWatchOptions {
    pub interval_secs: u64,
    pub hook_cmd: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_headers: Vec<String>,
    pub webhook_signing_secret: Option<String>,
    pub webhook_format: WebhookFormat,
    pub hook_chats: Vec<String>,
    pub hook_keywords: Vec<String>,
    pub fail_fast: bool,
    pub allow_insecure_webhooks: bool,
    pub min_hook_interval_secs: u64,
    pub min_webhook_interval_secs: u64,
    pub hook_timeout_secs: u64,
    pub webhook_timeout_secs: u64,
    pub json: bool,
    pub unattended: bool,
    pub allow_side_effects: bool,
}

/// Build the current-time ISO-8601 string, matching the LOCO watch's
/// `received_at` format (UTC, RFC 3339) so both event sources agree.
fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn build_event(row: &ax_send::ChatListRow) -> WatchMessageEvent {
    WatchMessageEvent {
        event_type: "ax_unread",
        received_at: now_iso(),
        method: "ax".to_string(),
        chat_id: 0,
        chat_name: row.name.clone(),
        log_id: 0,
        author_id: 0,
        author_nickname: String::new(),
        message_type: 1,
        message: row.preview.clone(),
        attachment: String::new(),
        unread: row.unread,
    }
}

/// Poll KakaoTalk's chat list and fire hooks/webhooks on unread increases.
/// Runs until interrupted (Ctrl-C). Never opens a chat, never steals focus.
pub fn cmd_ax_watch(options: AxWatchOptions) -> Result<()> {
    if options.hook_cmd.is_some() || options.webhook_url.is_some() {
        require_permission(
            options.unattended && options.allow_side_effects,
            "ax-watch side effects (hooks or webhooks)",
            "Re-run with --unattended --allow-watch-side-effects, or set both in ~/.config/openkakao/config.toml.",
        )?;
    }

    if let Some(url) = &options.webhook_url {
        validate_webhook_url(url, options.allow_insecure_webhooks)?;
    }
    let webhook_headers = options
        .webhook_headers
        .iter()
        .map(|h| parse_webhook_header(h))
        .collect::<Result<Vec<_>>>()?;

    let hook_config = WatchHookConfig {
        command: options.hook_cmd.clone(),
        webhook_url: options.webhook_url.clone(),
        webhook_headers,
        webhook_signing_secret: options.webhook_signing_secret.clone(),
        webhook_format: options.webhook_format,
        chat_ids: vec![],
        chat_names: options.hook_chats.clone(),
        keywords: options.hook_keywords.clone(),
        message_types: vec![],
        fail_fast: options.fail_fast,
        min_hook_interval_secs: options.min_hook_interval_secs,
        min_webhook_interval_secs: options.min_webhook_interval_secs,
        hook_timeout_secs: options.hook_timeout_secs,
        webhook_timeout_secs: options.webhook_timeout_secs,
    };
    let has_sinks = hook_config.command.is_some() || hook_config.webhook_url.is_some();

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut baseline: HashMap<String, i32> = HashMap::new();
        let mut first = true;
        eprintln!(
            "[ax-watch] polling KakaoTalk chat list every {}s (Ctrl-C to stop)",
            options.interval_secs
        );
        loop {
            match ax_send::scrape_chat_list() {
                Ok(rows) => {
                    for row in &rows {
                        let prev = baseline.get(&row.name).copied();
                        if should_emit(prev, row.unread, first) {
                            let event = build_event(row);
                            if options.json {
                                println!("{}", event.as_json());
                            } else {
                                eprintln!(
                                    "[ax-watch] {} (+{} unread): {}",
                                    event.chat_name, event.unread, event.message
                                );
                            }
                            if has_sinks && watch_hook_matches(&hook_config, &event) {
                                if hook_config.command.is_some() {
                                    if let Err(e) =
                                        run_watch_command_hook_async(&hook_config, &event).await
                                    {
                                        eprintln!("[ax-watch] hook failed: {e}");
                                        if hook_config.fail_fast {
                                            return Err(e);
                                        }
                                    }
                                }
                                if hook_config.webhook_url.is_some() {
                                    let cfg = hook_config.clone();
                                    let ev = event.clone();
                                    if let Err(e) = tokio::task::spawn_blocking(move || {
                                        run_watch_webhook(&cfg, &ev)
                                    })
                                    .await
                                    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
                                    {
                                        eprintln!("[ax-watch] webhook failed: {e}");
                                        if hook_config.fail_fast {
                                            return Err(e);
                                        }
                                    }
                                }
                            }
                        }
                        baseline.insert(row.name.clone(), row.unread);
                    }
                    first = false;
                }
                Err(e) => {
                    eprintln!("[ax-watch] scrape failed (retrying next poll): {e}");
                }
            }
            tokio::time::sleep(Duration::from_secs(options.interval_secs)).await;
        }
    })
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
