//! `ax-watch` — login-free receive detection. Polls KakaoTalk's chat list via
//! the macOS Accessibility API and fires the existing hook/webhook machinery
//! when a chat's unread count increases. No server contact (no ban risk),
//! background (never steals focus), non-intrusive (never opens a chat, so
//! unread state is untouched). Replaces the LOCO-based `watch`, which needs a
//! server session that recent KakaoTalk builds break.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Result};
use chrono::Utc;

use crate::ax_send::{self, DefaultServiceScraper, ServiceScrapeResult, ServiceScraper};
use crate::commands::watch::{
    parse_webhook_header, run_watch_command_hook_async, run_watch_webhook, validate_webhook_url,
    watch_hook_matches, WatchHookConfig, WatchMessageEvent, WebhookFormat,
};
use crate::util::require_permission;
use openkakao_cli::bujamentor_service::{
    append_log_record, run_direct_service_hook, validate_hook_program_path,
    validate_watch_runtime_paths, watch_log_record, write_config_invalid_status, write_status,
    ClosedReason, DirectHookLimiter, DirectServiceHookError, LogEvent, ServiceHookEvent,
    WatchStatusRecord, HOOK_TOTAL_DEADLINE_SECS, WATCH_INTERVAL_SECS,
};

/// Decide whether a chat-list row should fire an event this poll.
///
/// - The first poll only records a baseline, so existing messages do not flood.
/// - An unread-count increase indicates a new background message.
/// - A changed non-empty preview also indicates a new message when KakaoTalk
///   keeps the room open and therefore leaves its unread count at zero.
pub fn should_emit(
    prev_unread: Option<i32>,
    cur_unread: i32,
    prev_preview: Option<&str>,
    cur_preview: &str,
    first: bool,
) -> bool {
    if first {
        return false;
    }
    cur_unread > prev_unread.unwrap_or(0)
        || prev_preview.is_some_and(|prev| prev != cur_preview && !cur_preview.is_empty())
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
    pub service_mode: bool,
    pub status_path: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub hook_path: Option<PathBuf>,
}

/// Build the current-time ISO-8601 string, matching the LOCO watch's
/// `received_at` format (UTC, RFC 3339) so both event sources agree.
fn now_iso() -> String {
    Utc::now().to_rfc3339()
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

fn build_service_event(event: &WatchMessageEvent) -> ServiceHookEvent {
    ServiceHookEvent {
        event_type: event.event_type.to_string(),
        received_at: event.received_at.clone(),
        method: event.method.clone(),
        chat_id: event.chat_id,
        chat_name: event.chat_name.clone(),
        log_id: event.log_id,
        author_id: event.author_id,
        author_nickname: event.author_nickname.clone(),
        message_type: event.message_type,
        message: event.message.clone(),
        attachment: event.attachment.clone(),
        unread: event.unread,
    }
}

fn service_filter_config(options: &AxWatchOptions) -> WatchHookConfig {
    WatchHookConfig {
        command: None,
        webhook_url: None,
        webhook_headers: vec![],
        webhook_signing_secret: None,
        webhook_format: WebhookFormat::Raw,
        chat_ids: vec![],
        chat_names: options.hook_chats.clone(),
        keywords: options.hook_keywords.clone(),
        message_types: vec![],
        fail_fast: false,
        min_hook_interval_secs: options.min_hook_interval_secs,
        min_webhook_interval_secs: options.min_webhook_interval_secs,
        hook_timeout_secs: options.hook_timeout_secs,
        webhook_timeout_secs: options.webhook_timeout_secs,
    }
}

fn service_status_path(options: &AxWatchOptions) -> Result<&Path> {
    options
        .status_path
        .as_deref()
        .ok_or_else(|| anyhow!("--service-mode requires --status-path"))
}

fn service_log_path(options: &AxWatchOptions) -> Result<&Path> {
    options
        .log_path
        .as_deref()
        .ok_or_else(|| anyhow!("--service-mode requires --log-path"))
}

fn validate_service_mode_options(options: &AxWatchOptions) -> Result<()> {
    let status_path = service_status_path(options)?;
    let log_path = service_log_path(options)?;
    validate_watch_runtime_paths(status_path, log_path)?;

    if options.interval_secs != WATCH_INTERVAL_SECS {
        anyhow::bail!("--service-mode requires --interval {}", WATCH_INTERVAL_SECS);
    }
    if options.hook_cmd.is_some() || options.webhook_url.is_some() {
        anyhow::bail!("--service-mode forbids legacy --hook-cmd/--webhook-url sinks");
    }
    if !options.webhook_headers.is_empty() || options.webhook_signing_secret.is_some() {
        anyhow::bail!("--service-mode forbids webhook-only options");
    }
    if let Some(hook_path) = options.hook_path.as_deref() {
        validate_hook_program_path(hook_path)?;
        require_permission(
            options.unattended && options.allow_side_effects,
            "service ax-watch hook execution",
            "Re-run with --unattended --allow-watch-side-effects, or set both in ~/.config/openkakao/config.toml.",
        )?;
    }
    Ok(())
}

fn write_service_config_invalid_status(options: &AxWatchOptions) {
    let Some(status_path) = options.status_path.as_deref() else {
        return;
    };
    let Some(log_path) = options.log_path.as_deref() else {
        return;
    };
    if validate_watch_runtime_paths(status_path, log_path).is_ok() {
        let _ = write_config_invalid_status(status_path);
    }
}

async fn run_service_hook(
    hook_path: &Path,
    limiter: &mut DirectHookLimiter,
    event: &ServiceHookEvent,
) -> std::result::Result<(), ClosedReason> {
    if limiter.is_rate_limited() {
        return Err(ClosedReason::HookRateLimited);
    }
    limiter.record_attempt();
    match run_direct_service_hook(hook_path, event, HOOK_TOTAL_DEADLINE_SECS).await {
        Ok(()) => Ok(()),
        Err(DirectServiceHookError::TimedOut) => Err(ClosedReason::HookTimedOut),
        Err(DirectServiceHookError::Failed(_)) => Err(ClosedReason::HookFailed),
    }
}

fn transition_logger_failed(
    status_path: &Path,
    status: &mut WatchStatusRecord,
    error: anyhow::Error,
) -> Result<()> {
    status.mark_degraded(Utc::now(), ClosedReason::LoggerFailed);
    match write_status(status_path, status) {
        Ok(()) => Err(error.context("watch logger failure")),
        Err(write_error) => Err(error.context(format!(
            "watch logger failure; additionally failed to persist logger_failed status: {write_error}"
        ))),
    }
}

fn append_watch_log_or_transition(
    log_path: &Path,
    status_path: &Path,
    status: &mut WatchStatusRecord,
    event: LogEvent,
) -> Result<()> {
    let record = watch_log_record(Utc::now(), event, status)?;
    match append_log_record(log_path, &record, SystemTime::now()) {
        Ok(()) => Ok(()),
        Err(error) => transition_logger_failed(status_path, status, error),
    }
}

struct ServiceLoopState {
    baseline: HashMap<String, (i32, String)>,
    first: bool,
    hook_limiter: DirectHookLimiter,
    status: WatchStatusRecord,
}

async fn run_service_iteration<S: ServiceScraper>(
    scraper: &S,
    filter_config: &WatchHookConfig,
    hook_path: Option<&Path>,
    status_path: &Path,
    log_path: &Path,
    loop_state: &mut ServiceLoopState,
) -> Result<()> {
    loop_state.status.poll_count = loop_state.status.poll_count.saturating_add(1);

    match scraper.scrape() {
        ServiceScrapeResult::Success(rows) => {
            let mut failure = None;
            for row in &rows {
                let prev = loop_state.baseline.get(&row.name);
                let emitted = should_emit(
                    prev.map(|(unread, _)| *unread),
                    row.unread,
                    prev.map(|(_, preview)| preview.as_str()),
                    &row.preview,
                    loop_state.first,
                );

                if emitted {
                    let event = build_event(row);
                    if watch_hook_matches(filter_config, &event) {
                        if let Some(hook_path) = hook_path {
                            let service_event = build_service_event(&event);
                            match run_service_hook(
                                hook_path,
                                &mut loop_state.hook_limiter,
                                &service_event,
                            )
                            .await
                            {
                                Ok(()) => {
                                    loop_state.status.hook_success_count =
                                        loop_state.status.hook_success_count.saturating_add(1);
                                }
                                Err(reason) => {
                                    if reason == ClosedReason::HookRateLimited {
                                        loop_state.status.hook_rate_limited_count = loop_state
                                            .status
                                            .hook_rate_limited_count
                                            .saturating_add(1);
                                    } else {
                                        loop_state.status.hook_failure_count =
                                            loop_state.status.hook_failure_count.saturating_add(1);
                                    }
                                    failure = Some(reason);
                                    loop_state.baseline.insert(
                                        row.name.clone(),
                                        (row.unread, row.preview.clone()),
                                    );
                                    break;
                                }
                            }
                        }
                    }
                }
                loop_state
                    .baseline
                    .insert(row.name.clone(), (row.unread, row.preview.clone()));
            }

            if let Some(reason) = failure {
                loop_state.status.mark_degraded(Utc::now(), reason);
            } else {
                loop_state.status.mark_healthy(Utc::now());
            }
        }
        ServiceScrapeResult::AxUnavailable => {
            loop_state
                .status
                .mark_degraded(Utc::now(), ClosedReason::AxUnavailable);
        }
        ServiceScrapeResult::Failed => {
            loop_state
                .status
                .mark_degraded(Utc::now(), ClosedReason::ScrapeFailed);
        }
    }

    write_status(status_path, &loop_state.status)?;
    append_watch_log_or_transition(
        log_path,
        status_path,
        &mut loop_state.status,
        LogEvent::WatchPollCompleted,
    )?;
    loop_state.first = false;
    Ok(())
}

fn cmd_ax_watch_service(options: AxWatchOptions) -> Result<()> {
    if let Err(error) = validate_service_mode_options(&options) {
        write_service_config_invalid_status(&options);
        return Err(error);
    }

    let status_path = service_status_path(&options)?.to_path_buf();
    let log_path = service_log_path(&options)?.to_path_buf();
    let filter_config = service_filter_config(&options);
    let hook_path = options.hook_path.clone();
    let scraper = DefaultServiceScraper;
    let mut loop_state = ServiceLoopState {
        baseline: HashMap::new(),
        first: true,
        hook_limiter: DirectHookLimiter::new(Duration::from_secs(options.interval_secs)),
        status: WatchStatusRecord::starting_now(Utc::now()),
    };
    write_status(&status_path, &loop_state.status)?;
    append_watch_log_or_transition(
        &log_path,
        &status_path,
        &mut loop_state.status,
        LogEvent::WatchStarted,
    )?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        loop {
            run_service_iteration(
                &scraper,
                &filter_config,
                hook_path.as_deref(),
                &status_path,
                &log_path,
                &mut loop_state,
            )
            .await?;
            tokio::time::sleep(Duration::from_secs(options.interval_secs)).await;
        }
    })
}

/// Poll KakaoTalk's chat list and fire hooks/webhooks on unread increases.
/// Runs until interrupted (Ctrl-C). Never opens a chat, never steals focus.
pub fn cmd_ax_watch(options: AxWatchOptions) -> Result<()> {
    if options.service_mode {
        return cmd_ax_watch_service(options);
    }

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
        let mut baseline: HashMap<String, (i32, String)> = HashMap::new();
        let mut first = true;
        eprintln!(
            "[ax-watch] polling KakaoTalk chat list every {}s (Ctrl-C to stop)",
            options.interval_secs
        );
        loop {
            match ax_send::scrape_chat_list() {
                Ok(rows) => {
                    for row in &rows {
                        let prev = baseline.get(&row.name);
                        if should_emit(
                            prev.map(|(unread, _)| *unread),
                            row.unread,
                            prev.map(|(_, preview)| preview.as_str()),
                            &row.preview,
                            first,
                        ) {
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
                                    .unwrap_or_else(|e| Err(anyhow!(e)))
                                    {
                                        eprintln!("[ax-watch] webhook failed: {e}");
                                        if hook_config.fail_fast {
                                            return Err(e);
                                        }
                                    }
                                }
                            }
                        }
                        baseline.insert(row.name.clone(), (row.unread, row.preview.clone()));
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
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};

    use tempfile::tempdir;

    #[derive(Clone)]
    struct FakeScraper {
        result: ServiceScrapeResult,
    }

    impl ServiceScraper for FakeScraper {
        fn scrape(&self) -> ServiceScrapeResult {
            self.result.clone()
        }
    }

    fn canonical_service_options(root: &Path) -> AxWatchOptions {
        AxWatchOptions {
            interval_secs: WATCH_INTERVAL_SECS,
            hook_cmd: None,
            webhook_url: None,
            webhook_headers: vec![],
            webhook_signing_secret: None,
            webhook_format: WebhookFormat::Raw,
            hook_chats: vec![],
            hook_keywords: vec![],
            fail_fast: false,
            allow_insecure_webhooks: false,
            min_hook_interval_secs: 2,
            min_webhook_interval_secs: 2,
            hook_timeout_secs: 20,
            webhook_timeout_secs: 10,
            json: false,
            unattended: true,
            allow_side_effects: true,
            service_mode: true,
            status_path: Some(root.join("watch-status.json")),
            log_path: Some(root.join("watch.log")),
            hook_path: None,
        }
    }

    fn write_hook(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn first_poll_never_emits() {
        assert!(!should_emit(None, 5, None, "new", true));
        assert!(!should_emit(Some(0), 5, Some("old"), "new", true));
    }

    #[test]
    fn emits_when_unread_increases() {
        assert!(should_emit(Some(0), 3, Some("old"), "old", false));
        assert!(should_emit(Some(2), 5, Some("old"), "old", false));
    }

    #[test]
    fn emits_when_preview_changes_without_unread() {
        assert!(should_emit(Some(0), 0, Some("old"), "new", false));
    }

    #[test]
    fn no_emit_when_state_is_unchanged_or_preview_becomes_empty() {
        assert!(!should_emit(Some(3), 3, Some("same"), "same", false));
        assert!(!should_emit(Some(5), 2, Some("old"), "", false));
    }

    #[test]
    fn newly_seen_chat_with_unread_emits() {
        assert!(should_emit(None, 1, None, "new", false));
    }

    #[test]
    fn newly_seen_chat_without_unread_does_not_emit() {
        assert!(!should_emit(None, 0, None, "new", false));
    }

    #[test]
    fn service_mode_requires_fixed_interval() {
        let error = validate_service_mode_options(&AxWatchOptions {
            interval_secs: 3,
            ..canonical_service_options(Path::new("/tmp"))
        })
        .unwrap_err();
        assert!(error.to_string().contains("--interval 5"));
    }

    #[test]
    fn service_iteration_counts_hook_success_and_logs_watch_poll() {
        let temp = tempdir().unwrap();
        let mut options = canonical_service_options(temp.path());
        let hook_path = temp.path().join("hook.sh");
        write_hook(&hook_path, "#!/bin/sh\nexit 0\n");
        options.hook_path = Some(hook_path.clone());

        let status_path = service_status_path(&options).unwrap().to_path_buf();
        let log_path = service_log_path(&options).unwrap().to_path_buf();
        let filter_config = service_filter_config(&options);
        let scraper = FakeScraper {
            result: ServiceScrapeResult::Success(vec![ax_send::ChatListRow {
                name: "Alice".to_string(),
                unread: 1,
                preview: "hello".to_string(),
                timestamp: String::new(),
            }]),
        };
        let mut loop_state = ServiceLoopState {
            baseline: HashMap::new(),
            first: false,
            hook_limiter: DirectHookLimiter::new(Duration::from_secs(WATCH_INTERVAL_SECS)),
            status: WatchStatusRecord::starting_now(Utc::now()),
        };

        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(run_service_iteration(
                &scraper,
                &filter_config,
                options.hook_path.as_deref(),
                &status_path,
                &log_path,
                &mut loop_state,
            ))
            .unwrap();

        assert_eq!(
            loop_state.status.state,
            openkakao_cli::bujamentor_service::WatchState::Healthy
        );
        assert_eq!(loop_state.status.poll_count, 1);
        assert_eq!(loop_state.status.hook_success_count, 1);
        assert_eq!(loop_state.status.hook_failure_count, 0);
        assert_eq!(loop_state.status.hook_rate_limited_count, 0);
        let log = fs::read_to_string(&log_path).unwrap();
        assert!(log.contains("\"service\":\"watch\""));
        assert!(log.contains("\"event\":\"watch_poll_completed\""));
    }

    #[test]
    fn service_iteration_marks_logger_failed_when_watch_log_is_unusable() {
        let temp = tempdir().unwrap();
        let options = canonical_service_options(temp.path());
        let status_path = service_status_path(&options).unwrap().to_path_buf();
        let log_path = service_log_path(&options).unwrap().to_path_buf();
        let target = temp.path().join("real.log");
        fs::write(&target, "target\n").unwrap();
        symlink(&target, &log_path).unwrap();

        let filter_config = service_filter_config(&options);
        let scraper = FakeScraper {
            result: ServiceScrapeResult::Success(Vec::new()),
        };
        let mut loop_state = ServiceLoopState {
            baseline: HashMap::new(),
            first: true,
            hook_limiter: DirectHookLimiter::new(Duration::from_secs(WATCH_INTERVAL_SECS)),
            status: WatchStatusRecord::starting_now(Utc::now()),
        };

        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(run_service_iteration(
                &scraper,
                &filter_config,
                None,
                &status_path,
                &log_path,
                &mut loop_state,
            ))
            .unwrap_err();
        assert!(error.to_string().contains("watch logger failure"));

        let persisted: WatchStatusRecord =
            serde_json::from_str(&fs::read_to_string(&status_path).unwrap()).unwrap();
        assert_eq!(persisted.failure, Some(ClosedReason::LoggerFailed));
        assert_eq!(
            persisted.state,
            openkakao_cli::bujamentor_service::WatchState::Degraded
        );
    }
}
