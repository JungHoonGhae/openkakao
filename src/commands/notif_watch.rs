//! `notif-watch` — login-free receive detection via the macOS Notification
//! Center database. Polls `usernoted`'s SQLite (read-only) for KakaoTalk
//! notifications and emits them through the shared watch sinks. Works with the
//! KakaoTalk window closed, minimized, or on another Space, and self-sent
//! messages never appear (a message you send posts no notification). No server
//! contact (no ban risk), no SQLCipher decryption, no login — a read-only read
//! of a plaintext local SQLite file.
//!
//! Limits (structural — see docs): muted/notifications-off chats and the chat
//! you are currently focused on post no notification, so they are not seen; the
//! DB retains only notifications still in Notification Center (a forward-only
//! live stream, not history).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use plist::Value;
use rusqlite::{Connection, OpenFlags};

use crate::commands::watch::{
    dispatch_event, parse_webhook_header, validate_webhook_url, WatchHookConfig, WatchMessageEvent,
    WebhookFormat,
};
use crate::util::require_permission;

/// Seconds between the CFAbsoluteTime epoch (2001-01-01) and the Unix epoch.
const CFABSOLUTE_EPOCH_OFFSET: f64 = 978_307_200.0;

/// One KakaoTalk message pulled from a notification payload.
#[derive(Debug, Clone, PartialEq)]
pub struct NotifMessage {
    /// `/req/titl` — chat room / sender display name.
    pub chat_name: String,
    /// `/req/body` — message text. Empty for image messages or when the user's
    /// notification-preview setting hides content.
    pub body: String,
    /// `iden` prefix — stable per chat room.
    pub room_id: i64,
    /// `iden` suffix — per-message id (dedup / ordering key).
    pub msg_id: i64,
    /// `/date` — CFAbsoluteTime (seconds since 2001-01-01).
    pub ts: f64,
    /// `/req/atta[0]/pat` — local path of the first attachment, or empty.
    pub attachment_path: String,
}

/// Decode one notification payload (binary plist) into a `NotifMessage`.
///
/// Returns `None` when the payload lacks a parseable `iden` (`<room>_<msg>`,
/// both decimal) — that id is the dedup key, so a notification without it can't
/// be tracked. A missing title/body/attachment is fine (fields default empty).
pub fn parse_notification(payload: &[u8]) -> Option<NotifMessage> {
    let root = Value::from_reader(std::io::Cursor::new(payload)).ok()?;
    let dict = root.as_dictionary()?;
    let req = dict.get("req").and_then(|v| v.as_dictionary());

    let req_str = |k: &str| -> Option<String> {
        req.and_then(|d| d.get(k))
            .and_then(|v| v.as_string())
            .map(|s| s.to_string())
    };

    // iden = "<room_id>_<msg_id>", both decimal. Required.
    let iden = req_str("iden")?;
    let (room_s, msg_s) = iden.split_once('_')?;
    let room_id = room_s.parse::<i64>().ok()?;
    let msg_id = msg_s.parse::<i64>().ok()?;

    let chat_name = req_str("titl").unwrap_or_default();
    let body = req_str("body").unwrap_or_default();

    let ts = dict.get("date").and_then(value_as_f64).unwrap_or(0.0);

    let attachment_path = req
        .and_then(|r| r.get("atta"))
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("pat"))
        .and_then(|v| v.as_string())
        .unwrap_or_default()
        .to_string();

    Some(NotifMessage {
        chat_name,
        body,
        room_id,
        msg_id,
        ts,
        attachment_path,
    })
}

/// `date` may be stored as a real, an integer, or a numeric string.
fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Real(f) => Some(*f),
        Value::Integer(i) => i.as_signed().map(|n| n as f64),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

/// CFAbsoluteTime → RFC3339 UTC string, matching the other watch sources'
/// `received_at` format.
fn ts_to_rfc3339(cf: f64) -> String {
    let unix = cf + CFABSOLUTE_EPOCH_OFFSET;
    chrono::DateTime::<chrono::Utc>::from_timestamp(unix as i64, 0)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339()
}

/// Map a decoded notification to the shared watch event.
pub fn build_event(m: &NotifMessage) -> WatchMessageEvent {
    WatchMessageEvent {
        event_type: "notif_message",
        received_at: ts_to_rfc3339(m.ts),
        method: "notif".to_string(),
        chat_id: m.room_id,
        chat_name: m.chat_name.clone(),
        log_id: m.msg_id,
        author_id: 0,
        author_nickname: String::new(),
        // 1 = text, 2 = has attachment (mirrors the LOCO message_type convention
        // closely enough for hook filtering).
        message_type: if m.attachment_path.is_empty() { 1 } else { 2 },
        message: m.body.clone(),
        attachment: m.attachment_path.clone(),
        unread: 0,
    }
}

/// Decide whether a notification should fire this poll.
///
/// The first poll only records a baseline (so a backlog of notifications already
/// sitting in Notification Center doesn't flood on startup); afterwards each
/// `msg_id` fires exactly once.
pub fn should_emit(seen: &HashSet<i64>, msg_id: i64, first: bool) -> bool {
    !first && !seen.contains(&msg_id)
}

/// Options for `notif-watch` (mirrors `AxWatchOptions`).
pub struct NotifWatchOptions {
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

/// Path to the macOS Notification Center database.
fn notif_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("No home directory")?;
    Ok(home.join("Library/Group Containers/group.com.apple.usernoted/db2/db"))
}

/// Read the current KakaoTalk notification payloads, read-only.
///
// ponytail: opens immutable (ignores -wal) so a fresh connection each poll never
// locks the live DB; the only cost is a notification still in the WAL isn't seen
// until macOS checkpoints it into the main file — fine at watch cadence. Upgrade
// path if that latency ever matters: copy db+db-wal to a temp file and open that.
fn read_kakao_payloads(db_path: &Path) -> Result<Vec<Vec<u8>>> {
    let uri = format!("file:{}?mode=ro&immutable=1", db_path.display());
    let conn = Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("open notification DB: {}", db_path.display()))?;

    let mut stmt = conn.prepare(
        "SELECT r.data FROM record r JOIN app a ON r.app_id = a.app_id \
         WHERE a.identifier LIKE '%kakao%' AND r.data IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Whether KakaoTalk is registered as a notifier in an open notification DB.
/// Split out from [`check_access`] so it can be tested against an in-memory DB.
fn kakao_notifier_registered(conn: &Connection) -> rusqlite::Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM app WHERE identifier LIKE '%kakao%'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// Notification-receive readiness, for `doctor`.
pub struct NotifAccess {
    /// Whether the notification DB exists and could be opened read-only.
    pub db_readable: bool,
    /// Whether KakaoTalk is registered as a notifier (needs notifications on).
    pub kakao_registered: bool,
    /// Human-readable summary for the doctor row.
    pub detail: String,
}

/// Probe whether `notif-watch` can work on this machine (read-only, no writes).
pub fn check_access() -> NotifAccess {
    let path = match notif_db_path() {
        Ok(p) => p,
        Err(e) => {
            return NotifAccess {
                db_readable: false,
                kakao_registered: false,
                detail: format!("cannot resolve home directory: {e}"),
            }
        }
    };
    if !path.exists() {
        return NotifAccess {
            db_readable: false,
            kakao_registered: false,
            detail: format!("notification DB not found at {}", path.display()),
        };
    }
    let uri = format!("file:{}?mode=ro&immutable=1", path.display());
    match Connection::open_with_flags(
        &uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => {
            let kakao = kakao_notifier_registered(&conn).unwrap_or(false);
            NotifAccess {
                db_readable: true,
                kakao_registered: kakao,
                detail: if kakao {
                    "notification DB readable; KakaoTalk registered as notifier".to_string()
                } else {
                    "notification DB readable, but KakaoTalk is not a registered notifier — \
                     enable KakaoTalk notifications in System Settings > Notifications"
                        .to_string()
                },
            }
        }
        Err(e) => NotifAccess {
            db_readable: false,
            kakao_registered: false,
            detail: format!("cannot open notification DB read-only: {e}"),
        },
    }
}

/// Poll the Notification Center DB and fire hooks/webhooks on new KakaoTalk
/// messages. Runs until interrupted (Ctrl-C).
pub fn cmd_notif_watch(options: NotifWatchOptions) -> Result<()> {
    if options.hook_cmd.is_some() || options.webhook_url.is_some() {
        require_permission(
            options.unattended && options.allow_side_effects,
            "notif-watch side effects (hooks or webhooks)",
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

    let db_path = notif_db_path()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // ponytail: unbounded set of seen msg_ids. A long-running watch grows it
        // slowly; bound to a ring buffer only if memory ever matters.
        let mut seen: HashSet<i64> = HashSet::new();
        let mut first = true;
        eprintln!(
            "[notif-watch] polling macOS notification center every {}s (Ctrl-C to stop)",
            options.interval_secs
        );
        loop {
            match read_kakao_payloads(&db_path) {
                Ok(payloads) => {
                    for payload in &payloads {
                        let Some(msg) = parse_notification(payload) else {
                            continue;
                        };
                        if should_emit(&seen, msg.msg_id, first) {
                            let event = build_event(&msg);
                            let human_line = format!(
                                "[notif-watch] {}: {}",
                                event.chat_name,
                                if event.message.is_empty() {
                                    "(attachment)"
                                } else {
                                    &event.message
                                }
                            );
                            dispatch_event(
                                &event,
                                &hook_config,
                                has_sinks,
                                options.json,
                                &human_line,
                                "notif-watch",
                            )
                            .await?;
                        }
                        seen.insert(msg.msg_id);
                    }
                    first = false;
                }
                Err(e) => {
                    eprintln!("[notif-watch] read failed (retrying next poll): {e}");
                }
            }
            tokio::time::sleep(Duration::from_secs(options.interval_secs)).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist::{Dictionary, Value};

    /// Build a synthetic notification payload matching KakaoTalk's structure,
    /// so tests carry no real message content.
    fn make_payload(
        titl: &str,
        body: Option<&str>,
        iden: &str,
        date: f64,
        attachment: Option<&str>,
    ) -> Vec<u8> {
        let mut req = Dictionary::new();
        req.insert("titl".into(), Value::String(titl.into()));
        req.insert("iden".into(), Value::String(iden.into()));
        if let Some(b) = body {
            req.insert("body".into(), Value::String(b.into()));
        }
        if let Some(p) = attachment {
            let mut atta = Dictionary::new();
            atta.insert("pat".into(), Value::String(p.into()));
            req.insert("atta".into(), Value::Array(vec![Value::Dictionary(atta)]));
        }
        let mut root = Dictionary::new();
        root.insert("app".into(), Value::String("com.kakao.KakaoTalkMac".into()));
        root.insert("date".into(), Value::Real(date));
        root.insert("req".into(), Value::Dictionary(req));

        let mut buf = Vec::new();
        Value::Dictionary(root)
            .to_writer_binary(&mut buf)
            .expect("write bplist");
        buf
    }

    #[test]
    fn parses_text_message() {
        let p = make_payload(
            "테스트방",
            Some("안녕하세요"),
            "474071844489941_3891201215281264642",
            806497332.35,
            None,
        );
        let m = parse_notification(&p).expect("should parse");
        assert_eq!(m.chat_name, "테스트방");
        assert_eq!(m.body, "안녕하세요");
        assert_eq!(m.room_id, 474071844489941);
        assert_eq!(m.msg_id, 3891201215281264642);
        assert_eq!(m.attachment_path, "");
    }

    #[test]
    fn parses_image_message_with_empty_body_and_attachment() {
        let p = make_payload(
            "테스트방",
            None,
            "474071844489941_3891759278451095553",
            806563858.72,
            Some("/tmp/photo.jpeg"),
        );
        let m = parse_notification(&p).expect("should parse");
        assert_eq!(m.body, "");
        assert_eq!(m.attachment_path, "/tmp/photo.jpeg");
        let ev = build_event(&m);
        assert_eq!(ev.message, "");
        assert_eq!(ev.attachment, "/tmp/photo.jpeg");
        assert_eq!(ev.message_type, 2);
    }

    #[test]
    fn same_room_shares_room_id_distinct_msg_id() {
        let a = parse_notification(&make_payload(
            "방", Some("첫"), "474071844489941_100", 1.0, None,
        ))
        .unwrap();
        let b = parse_notification(&make_payload(
            "방", Some("둘"), "474071844489941_200", 2.0, None,
        ))
        .unwrap();
        assert_eq!(a.room_id, b.room_id);
        assert_ne!(a.msg_id, b.msg_id);
    }

    #[test]
    fn rejects_payload_without_parseable_iden() {
        // no iden at all
        let no_iden = {
            let mut req = Dictionary::new();
            req.insert("titl".into(), Value::String("방".into()));
            req.insert("body".into(), Value::String("hi".into()));
            let mut root = Dictionary::new();
            root.insert("req".into(), Value::Dictionary(req));
            let mut buf = Vec::new();
            Value::Dictionary(root).to_writer_binary(&mut buf).unwrap();
            buf
        };
        assert!(parse_notification(&no_iden).is_none());
        // non-numeric iden
        let bad = make_payload("방", Some("hi"), "abc_def", 1.0, None);
        assert!(parse_notification(&bad).is_none());
    }

    #[test]
    fn build_event_maps_iden_and_timestamp() {
        let m = parse_notification(&make_payload(
            "방", Some("hi"), "42_99", 0.0, None,
        ))
        .unwrap();
        let ev = build_event(&m);
        assert_eq!(ev.chat_id, 42);
        assert_eq!(ev.log_id, 99);
        assert_eq!(ev.method, "notif");
        assert_eq!(ev.event_type, "notif_message");
        // date=0 CFAbsoluteTime == 2001-01-01T00:00:00Z
        assert!(ev.received_at.starts_with("2001-01-01T00:00:00"));
    }

    #[test]
    fn detects_kakao_notifier_registration() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE app(app_id INTEGER PRIMARY KEY, identifier TEXT);")
            .unwrap();
        // no kakao yet
        assert!(!kakao_notifier_registered(&conn).unwrap());
        conn.execute(
            "INSERT INTO app(identifier) VALUES ('com.kakao.kakaotalkmac')",
            [],
        )
        .unwrap();
        assert!(kakao_notifier_registered(&conn).unwrap());
    }

    #[test]
    fn should_emit_baselines_first_poll_then_dedups() {
        let mut seen = HashSet::new();
        // first poll: never emit, just baseline
        assert!(!should_emit(&seen, 1, true));
        seen.insert(1);
        // later poll: a new id emits
        assert!(should_emit(&seen, 2, false));
        seen.insert(2);
        // already seen: no re-emit
        assert!(!should_emit(&seen, 2, false));
        // baselined id: no emit
        assert!(!should_emit(&seen, 1, false));
    }
}
