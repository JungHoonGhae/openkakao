mod auth;
mod auth_flow;
mod config;
mod credentials;
mod error;
mod export;
mod loco;
mod model;
mod rest;
mod state;

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{Datelike, Local, TimeZone};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use hmac::{Hmac, Mac};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;

use crate::auth::{extract_refresh_token, get_credential_candidates};
use crate::auth_flow::{
    attempt_relogin, attempt_renew, connect_loco_with_reauth, get_rest_ready_client,
    resolve_base_credentials, select_best_credential, set_auth_policy, AuthPolicy, RecoveryAttempt,
};
use crate::config::{load_config, OpenKakaoConfig};
use crate::credentials::{load_credentials, save_credentials};
use crate::export::ExportFormat;
use crate::model::{json_i64, json_string, ChatMember, KakaoCredentials};
use crate::rest::KakaoRestClient;
use crate::state::{
    auth_cooldown_remaining_secs, hook_remaining_secs, mark_hook_attempt,
    mark_unattended_send_attempt, mark_webhook_attempt, record_failure, record_guard,
    record_transport_success, recovery_snapshot, safety_snapshot, unattended_send_remaining_secs,
    webhook_remaining_secs,
};

static NO_COLOR: AtomicBool = AtomicBool::new(false);

fn color_enabled() -> bool {
    !NO_COLOR.load(Ordering::Relaxed)
}

const VERSION: &str = env!("CARGO_PKG_VERSION");
const SEND_PREFIX: &str = "🤖 [Sent via openkakao]";

fn format_outgoing_message(message: &str, no_prefix: bool) -> String {
    if no_prefix {
        message.to_string()
    } else {
        format!("{} {}", SEND_PREFIX, message)
    }
}

#[derive(Debug, Clone)]
struct WatchHookConfig {
    command: Option<String>,
    webhook_url: Option<String>,
    webhook_headers: Vec<(String, String)>,
    webhook_signing_secret: Option<String>,
    chat_ids: Vec<i64>,
    keywords: Vec<String>,
    message_types: Vec<i32>,
    fail_fast: bool,
    min_hook_interval_secs: u64,
    min_webhook_interval_secs: u64,
    hook_timeout_secs: u64,
    webhook_timeout_secs: u64,
}

#[derive(Debug, Clone)]
struct WatchOptions {
    unattended: bool,
    allow_side_effects: bool,
    filter_chat_id: Option<i64>,
    raw: bool,
    read_receipt: bool,
    max_reconnect: u32,
    download_media: bool,
    download_dir: String,
    hook_cmd: Option<String>,
    webhook_url: Option<String>,
    webhook_headers: Vec<String>,
    webhook_signing_secret: Option<String>,
    hook_chat_ids: Vec<i64>,
    hook_keywords: Vec<String>,
    hook_types: Vec<i32>,
    hook_fail_fast: bool,
    min_hook_interval_secs: u64,
    min_webhook_interval_secs: u64,
    hook_timeout_secs: u64,
    webhook_timeout_secs: u64,
    allow_insecure_webhooks: bool,
}

#[derive(Debug, Clone, Serialize)]
struct LocoBlockedMember {
    user_id: i64,
    nickname: String,
    profile_image_url: String,
    full_profile_image_url: String,
    suspended: bool,
    suspicion: String,
    block_type: i32,
    is_plus: bool,
}

#[derive(Debug, Clone, Serialize)]
struct LocoBlockedSnapshot {
    revision: i64,
    plus_revision: i64,
    members: Vec<LocoBlockedMember>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProfileCacheHint {
    entry_id: i64,
    kind: String,
    request_key: String,
    user_ids: Vec<i64>,
    chat_id: Option<i64>,
    access_permit: Option<String>,
    category: Option<String>,
    data_on_fs: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
struct ProfileRevisionHints {
    profile_list_revision: Option<i64>,
    designated_friends_revision: Option<i64>,
    block_friends_sync_enabled: Option<bool>,
    block_channels_sync_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct ProfileHintsSnapshot {
    revisions: ProfileRevisionHints,
    cached_requests: Vec<ProfileCacheHint>,
    app_state: Option<KakaoAppStateSnapshot>,
    app_state_diff: Option<Vec<KakaoAppStateDiffEntry>>,
    local_graph: Option<LocalFriendGraphHintSummary>,
    syncmainpf_candidates: Vec<SyncMainPfCandidate>,
    syncmainpf_probe_results: Vec<SyncMainPfProbeResult>,
    uplinkprof_probe_results: Vec<MethodProbeResult>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProfileHintsBaseline {
    app_state: Option<KakaoAppStateSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalFriendGraphEntry {
    user_id: i64,
    account_id: i64,
    nickname: String,
    country_iso: String,
    status_message: String,
    profile_image_url: String,
    full_profile_image_url: String,
    original_profile_image_url: String,
    access_permits: Vec<String>,
    suspicion: String,
    suspended: bool,
    memorial: bool,
    member_type: i32,
    chat_ids: Vec<i64>,
    chat_titles: Vec<String>,
    is_self: bool,
    hidden_like: bool,
    hidden_block_type: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalFriendGraphChatMeta {
    chat_id: i64,
    title: String,
    getmem_token: Option<i64>,
    member_count: usize,
}

#[derive(Debug, Clone)]
struct LocalFriendGraphSnapshot {
    user_count: usize,
    chat_count: usize,
    failed_chat_ids: Vec<i64>,
    chat_meta: Vec<LocalFriendGraphChatMeta>,
    entries: Vec<LocalFriendGraphEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalFriendGraphHintSummary {
    user_count: usize,
    chat_count: usize,
    failed_chat_ids: Vec<i64>,
    chat_meta: Vec<LocalFriendGraphChatMeta>,
    candidate_matches: Vec<LocalFriendGraphHintMatch>,
}

#[derive(Debug, Clone, Serialize)]
struct LocalFriendGraphHintMatch {
    entry_id: i64,
    kind: String,
    requested_user_ids: Vec<i64>,
    matched_user_ids: Vec<i64>,
    candidate_chat_ids: Vec<i64>,
    candidate_access_permits: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct SyncMainPfCandidate {
    user_id: i64,
    account_id: i64,
    is_self: bool,
    source_entry_ids: Vec<i64>,
    bodies: Vec<serde_json::Value>,
    uplinkprof_bodies: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
struct SyncMainPfProbeResult {
    body: serde_json::Value,
    packet_status_code: i16,
    body_status: Option<i32>,
    push_count: usize,
    push_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MethodProbeResult {
    method: String,
    body: serde_json::Value,
    packet_status_code: i16,
    body_status: Option<i32>,
    push_count: usize,
    push_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KakaoAppStateFile {
    path: String,
    kind: String,
    size: u64,
    modified_unix: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KakaoAppStateSnapshot {
    root: String,
    preferences_dir: String,
    cache_db: String,
    files: Vec<KakaoAppStateFile>,
}

#[derive(Debug, Clone, Serialize)]
struct KakaoAppStateDiffEntry {
    path: String,
    change: String,
    before_size: Option<u64>,
    after_size: Option<u64>,
    before_modified_unix: Option<u64>,
    after_modified_unix: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct ChatListing {
    chat_id: i64,
    kind: String,
    title: String,
    has_unread: bool,
    unread_count: Option<i64>,
    active_members: Option<i32>,
    last_log_id: Option<i64>,
    last_seen_log_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct LocoMemberProfile {
    user_id: i64,
    account_id: i64,
    nickname: String,
    country_iso: String,
    status_message: String,
    profile_image_url: String,
    full_profile_image_url: String,
    original_profile_image_url: String,
    access_permit: String,
    suspicion: String,
    suspended: bool,
    memorial: bool,
    member_type: i32,
    ut: i64,
}

#[derive(Debug, Clone)]
struct LocoGetMemSnapshot {
    token: Option<i64>,
    members: Vec<LocoMemberProfile>,
}

impl LocoMemberProfile {
    fn from_getmem_doc(doc: &bson::Document) -> Self {
        Self {
            user_id: get_bson_i64(doc, &["userId"]),
            account_id: get_bson_i64(doc, &["accountId"]),
            nickname: get_bson_str(doc, &["nickName", "nickname"]),
            country_iso: get_bson_str(doc, &["countryIso"]),
            status_message: get_bson_str(doc, &["statusMessage"]),
            profile_image_url: get_bson_str(doc, &["profileImageUrl"]),
            full_profile_image_url: get_bson_str(doc, &["fullProfileImageUrl"]),
            original_profile_image_url: get_bson_str(doc, &["originalProfileImageUrl"]),
            access_permit: get_bson_str(doc, &["accessPermit"]),
            suspicion: get_bson_str(doc, &["suspicion"]),
            suspended: get_bson_bool(doc, &["suspended"]),
            memorial: get_bson_bool(doc, &["memorial"]),
            member_type: get_bson_i32(doc, &["type"]),
            ut: get_bson_i64(doc, &["ut"]),
        }
    }

    fn as_chat_member(&self) -> ChatMember {
        ChatMember {
            user_id: self.user_id,
            nickname: self.nickname.clone(),
            friend_nickname: String::new(),
            country_iso: self.country_iso.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct ReadCommandOptions {
    count: usize,
    cursor: Option<i64>,
    since: Option<String>,
    all: bool,
    delay_ms: u64,
    force: bool,
    rest: bool,
    json: bool,
}

#[derive(Debug, Clone)]
struct WatchMessageEvent {
    event_type: &'static str,
    received_at: String,
    method: String,
    chat_id: i64,
    chat_name: String,
    log_id: i64,
    author_id: i64,
    author_nickname: String,
    message_type: i32,
    message: String,
    attachment: String,
}

impl WatchMessageEvent {
    fn as_json(&self) -> Value {
        serde_json::json!({
            "event_type": self.event_type,
            "received_at": self.received_at,
            "method": self.method,
            "chat_id": self.chat_id,
            "chat_name": self.chat_name,
            "log_id": self.log_id,
            "author_id": self.author_id,
            "author_nickname": self.author_nickname,
            "message_type": self.message_type,
            "message": self.message,
            "attachment": self.attachment,
        })
    }
}

fn message_type_label(message_type: i32) -> &'static str {
    match message_type {
        1 => "text",
        2 => "photo",
        3 => "video",
        5 => "contact",
        12 => "voice",
        14 => "emoticon",
        16 => "live",
        18 => "search",
        22 => "map",
        23 => "profile",
        26 => "file",
        27 => "multi-photo",
        71 | 72 => "poll",
        _ => "unknown",
    }
}

fn render_message_content(body: &bson::Document, msg_type: i32) -> String {
    match msg_type {
        1 => body.get_str("msg").unwrap_or("").to_string(),
        2 => "사진을 보냈습니다.".to_string(),
        3 => "동영상을 보냈습니다.".to_string(),
        5 => "연락처를 보냈습니다.".to_string(),
        12 => "음성메시지를 보냈습니다.".to_string(),
        14 => "이모티콘을 보냈습니다.".to_string(),
        16 => "라이브톡".to_string(),
        18 => "샵검색을 보냈습니다.".to_string(),
        22 => "지도를 보냈습니다.".to_string(),
        23 => "프로필을 보냈습니다.".to_string(),
        26 => "파일을 보냈습니다.".to_string(),
        27 => "멀티사진을 보냈습니다.".to_string(),
        71 | 72 => "투표를 보냈습니다.".to_string(),
        _ => body
            .get_str("msg")
            .map(String::from)
            .unwrap_or_else(|_| format!("[type={}]", msg_type)),
    }
}

fn watch_hook_matches(config: &WatchHookConfig, event: &WatchMessageEvent) -> bool {
    if !config.chat_ids.is_empty() && !config.chat_ids.contains(&event.chat_id) {
        return false;
    }

    if !config.message_types.is_empty() && !config.message_types.contains(&event.message_type) {
        return false;
    }

    if !config.keywords.is_empty() {
        let haystack = event.message.to_lowercase();
        if !config
            .keywords
            .iter()
            .any(|keyword| haystack.contains(&keyword.to_lowercase()))
        {
            return false;
        }
    }

    true
}

fn parse_webhook_header(header: &str) -> Result<(String, String)> {
    let (name, value) = header
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid webhook header, expected 'Name: Value'"))?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        anyhow::bail!("invalid webhook header, expected non-empty name and value");
    }
    Ok((name.to_string(), value.to_string()))
}

fn build_webhook_signature(secret: &str, timestamp: &str, payload: &[u8]) -> Result<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("failed to initialize webhook signer: {}", e))?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(payload);
    Ok(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ))
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

fn validate_webhook_url(webhook_url: &str, allow_insecure_webhooks: bool) -> Result<()> {
    let url = reqwest::Url::parse(webhook_url)
        .map_err(|e| anyhow::anyhow!("invalid webhook URL '{}': {}", webhook_url, e))?;
    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = url.host_str().unwrap_or_default();
            if is_loopback_host(host) || allow_insecure_webhooks {
                Ok(())
            } else {
                anyhow::bail!(
                    "refusing insecure webhook URL '{}'; use https or localhost, or opt in via config safety.allow_insecure_webhooks = true",
                    webhook_url
                )
            }
        }
        other => anyhow::bail!(
            "unsupported webhook URL scheme '{}'; use https or localhost http",
            other
        ),
    }
}

fn validate_outbound_message(message: &str) -> Result<()> {
    if message.trim().is_empty() {
        anyhow::bail!("refusing to send an empty or whitespace-only message");
    }
    Ok(())
}

fn run_watch_command_hook(config: &WatchHookConfig, event: &WatchMessageEvent) -> Result<()> {
    let command = config
        .command
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing hook command"))?;
    if let Some(remaining) = hook_remaining_secs(config.min_hook_interval_secs)? {
        record_guard("hook_rate_limited")?;
        eprintln!(
            "[guard/hook] Skipping local hook for chat {} log {}: {}s rate-limit remaining.",
            event.chat_id, event.log_id, remaining
        );
        return Ok(());
    }
    mark_hook_attempt()?;
    let payload = serde_json::to_vec_pretty(&event.as_json())?;

    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .env("OPENKAKAO_EVENT_TYPE", event.event_type)
        .env("OPENKAKAO_CHAT_ID", event.chat_id.to_string())
        .env("OPENKAKAO_CHAT_NAME", &event.chat_name)
        .env("OPENKAKAO_LOG_ID", event.log_id.to_string())
        .env("OPENKAKAO_AUTHOR_ID", event.author_id.to_string())
        .env("OPENKAKAO_AUTHOR_NICKNAME", &event.author_nickname)
        .env("OPENKAKAO_MESSAGE_TYPE", event.message_type.to_string())
        .env(
            "OPENKAKAO_MESSAGE_TYPE_LABEL",
            message_type_label(event.message_type),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(&payload)?;
    }

    let timeout = Duration::from_secs(config.hook_timeout_secs.max(1));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow::anyhow!(
                "hook command timed out after {}s",
                config.hook_timeout_secs
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "hook command exited with status {}",
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string())
        ))
    }
}

fn run_watch_webhook(config: &WatchHookConfig, event: &WatchMessageEvent) -> Result<()> {
    let webhook_url = config
        .webhook_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing webhook url"))?;
    if let Some(remaining) = webhook_remaining_secs(config.min_webhook_interval_secs)? {
        record_guard("webhook_rate_limited")?;
        eprintln!(
            "[guard/webhook] Skipping webhook for chat {} log {}: {}s rate-limit remaining.",
            event.chat_id, event.log_id, remaining
        );
        return Ok(());
    }
    mark_webhook_attempt()?;
    let payload = serde_json::to_vec(&event.as_json())?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(config.webhook_timeout_secs.max(1)))
        .build()?;
    let mut request = client
        .post(webhook_url)
        .header("Content-Type", "application/json");

    for (name, value) in &config.webhook_headers {
        request = request.header(name, value);
    }

    if let Some(secret) = &config.webhook_signing_secret {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let signature = build_webhook_signature(secret, &timestamp, &payload)?;
        request = request
            .header("X-OpenKakao-Timestamp", &timestamp)
            .header("X-OpenKakao-Signature", signature);
    }

    let response = request.body(payload).send()?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "webhook returned non-success status {}",
            response.status()
        ))
    }
}

#[derive(Parser, Debug)]
#[command(name = "openkakao-rs")]
#[command(about = "OpenKakao Rust CLI", long_about = None)]
#[command(version = VERSION)]
struct Cli {
    #[arg(long, global = true, help = "Output as JSON")]
    json: bool,
    #[arg(long, global = true, help = "Disable colored output")]
    no_color: bool,
    #[arg(
        long,
        global = true,
        help = "Explicitly acknowledge unattended or non-interactive operation"
    )]
    unattended: bool,
    #[arg(
        long,
        global = true,
        help = "Allow non-interactive send operations when combined with --unattended"
    )]
    allow_non_interactive_send: bool,
    #[arg(
        long,
        global = true,
        help = "Allow watch read receipts, hooks, and webhooks when combined with --unattended"
    )]
    allow_watch_side_effects: bool,
    #[arg(
        long,
        global = true,
        help = "Do not prepend '🤖 [Sent via openkakao]' prefix to outgoing messages"
    )]
    no_prefix: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Verify token validity
    Auth,
    /// Show persisted auth recovery state and cooldowns
    AuthStatus,
    /// Extract credentials from KakaoTalk cache
    Login {
        #[arg(long)]
        save: bool,
    },
    /// Show own profile
    Me,
    /// List friends
    Friends {
        #[arg(short = 'f', long)]
        favorites: bool,
        #[arg(long)]
        hidden: bool,
        #[arg(short = 's', long)]
        search: Option<String>,
        #[arg(
            long,
            help = "Build a local friend graph from LOCO GETMEM across known chats"
        )]
        local: bool,
        #[arg(
            long,
            help = "When used with --local, only include users seen in this chat"
        )]
        chat_id: Option<i64>,
        #[arg(long, help = "When used with --local, only include this user")]
        user_id: Option<i64>,
    },
    /// List chat rooms
    Chats {
        #[arg(short = 'a', long = "all")]
        show_all: bool,
        #[arg(short = 'u', long)]
        unread: bool,
        #[arg(long, help = "Search chat rooms by title")]
        search: Option<String>,
        #[arg(long = "type", help = "Filter by type: dm, group, memo, open")]
        chat_type: Option<String>,
        #[arg(long, help = "Force REST chat list path instead of LOCO")]
        rest: bool,
    },
    /// Read messages from a chat room
    Read {
        chat_id: i64,
        #[arg(short = 'n', long, default_value_t = 30)]
        count: usize,
        #[arg(long, help = "Before this logId (backward pagination)")]
        before: Option<i64>,
        #[arg(long, help = "Resume from cursor (logId from previous run)")]
        cursor: Option<i64>,
        #[arg(long, help = "Filter messages after this date (YYYY-MM-DD)")]
        since: Option<String>,
        #[arg(long, help = "Fetch all available messages")]
        all: bool,
        #[arg(
            long,
            default_value_t = 100,
            help = "Delay between LOCO batches in ms (ignored for --rest)"
        )]
        delay_ms: u64,
        #[arg(long, help = "Allow LOCO full-history reads on open chats")]
        force: bool,
        #[arg(long, help = "Force REST read path instead of LOCO")]
        rest: bool,
    },
    /// List members of a chat room
    Members {
        chat_id: i64,
        #[arg(long, help = "Force REST member list path instead of LOCO")]
        rest: bool,
        #[arg(long, help = "Show richer LOCO member profile fields")]
        full: bool,
    },
    /// Get detailed information about a chat room
    Chatinfo { chat_id: i64 },
    /// Show account settings
    Settings,
    /// Get link preview (OG tags) for a URL
    Scrap { url: String },
    /// Show a friend's profile
    Profile {
        user_id: i64,
        #[arg(long, help = "Use chat-scoped LOCO member profile for this chat")]
        #[arg(conflicts_with = "local")]
        chat_id: Option<i64>,
        #[arg(
            long,
            help = "Resolve from the local LOCO friend graph built from known chats"
        )]
        local: bool,
    },
    /// Add a friend to favorites
    Favorite { user_id: i64 },
    /// Remove a friend from favorites
    Unfavorite { user_id: i64 },
    /// Hide a friend
    Hide { user_id: i64 },
    /// Unhide a friend
    Unhide { user_id: i64 },
    /// List profile cards (multi-profile)
    Profiles,
    /// Show notification alarm keywords
    Keywords,
    /// Show unread chat summary
    Unread,
    /// Export chat messages
    Export {
        chat_id: i64,
        #[arg(long, default_value = "txt", help = "Output format: json, csv, txt")]
        format: String,
        #[arg(short = 'o', long, help = "Output file (default: stdout)")]
        output: Option<String>,
    },
    /// Search messages in a chat room
    Search { chat_id: i64, query: String },
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Attempt to renew OAuth token using cached refresh_token
    Renew,
    /// Re-login via login.json to obtain LOCO access_token
    Relogin {
        /// Generate fresh X-VC values instead of using cached one
        #[arg(long)]
        fresh_xvc: bool,
        /// Supply current password (cached password may be expired)
        #[arg(long)]
        password: Option<String>,
        /// Override email/phone from Cache.db
        #[arg(long)]
        email: Option<String>,
    },
    #[command(hide = true)]
    /// Test LOCO protocol connection (legacy command)
    LocoTest,
    /// Send a message via LOCO protocol
    Send {
        chat_id: i64,
        message: String,
        #[arg(long, help = "Allow sending to open chats (higher ban risk)")]
        force: bool,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
    },
    /// Watch real-time messages via LOCO protocol
    Watch {
        #[arg(long, help = "Filter by chat ID")]
        chat_id: Option<i64>,
        #[arg(long, help = "Show raw BSON body")]
        raw: bool,
        #[arg(long, help = "Send read receipts (NOTIREAD) for incoming messages")]
        read_receipt: bool,
        #[arg(
            long,
            default_value_t = 5,
            help = "Max reconnect attempts (0 = no reconnect)"
        )]
        max_reconnect: u32,
        #[arg(long, help = "Auto-download media attachments")]
        download_media: bool,
        #[arg(
            long,
            default_value = "downloads",
            help = "Directory for downloaded media"
        )]
        download_dir: String,
        #[arg(long, help = "Run a local shell command for matched events")]
        hook_cmd: Option<String>,
        #[arg(long, help = "POST matched events to a webhook URL")]
        webhook_url: Option<String>,
        #[arg(
            long = "webhook-header",
            help = "Additional webhook header in 'Name: Value' format"
        )]
        webhook_header: Vec<String>,
        #[arg(
            long = "webhook-signing-secret",
            help = "Sign webhook payloads with HMAC-SHA256 and emit X-OpenKakao-Timestamp / X-OpenKakao-Signature"
        )]
        webhook_signing_secret: Option<String>,
        #[arg(long = "hook-chat-id", help = "Only trigger hooks for these chat IDs")]
        hook_chat_id: Vec<i64>,
        #[arg(
            long = "hook-keyword",
            help = "Only trigger hooks when message text contains keyword"
        )]
        hook_keyword: Vec<String>,
        #[arg(
            long = "hook-type",
            help = "Only trigger hooks for these message type codes"
        )]
        hook_type: Vec<i32>,
        #[arg(long, help = "Stop watch when a hook command fails")]
        hook_fail_fast: bool,
    },
    /// Send a photo via LOCO protocol (alias for send-file)
    SendPhoto {
        chat_id: i64,
        /// Path to image file (JPEG/PNG/GIF)
        file: String,
        #[arg(long, help = "Allow sending to open chats (higher ban risk)")]
        force: bool,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
    },
    /// Send a file (photo/video/document) via LOCO protocol
    SendFile {
        chat_id: i64,
        /// Path to file
        file: String,
        #[arg(long, help = "Allow sending to open chats (higher ban risk)")]
        force: bool,
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
    },
    /// Download media attachment from a specific message
    Download {
        chat_id: i64,
        log_id: i64,
        #[arg(short = 'o', long, help = "Output directory (default: downloads)")]
        output_dir: Option<String>,
    },
    #[command(hide = true)]
    /// List chat rooms via LOCO protocol (legacy command)
    LocoChats {
        #[arg(short = 'a', long = "all")]
        show_all: bool,
    },
    #[command(hide = true)]
    /// Read messages via LOCO protocol (legacy command)
    LocoRead {
        chat_id: i64,
        #[arg(short = 'n', long, default_value_t = 30)]
        count: i32,
        #[arg(long, help = "Resume from this logId (cursor from previous run)")]
        cursor: Option<i64>,
        #[arg(long, help = "Filter messages after this date (YYYY-MM-DD)")]
        since: Option<String>,
        #[arg(long, help = "Fetch all available messages")]
        all: bool,
        #[arg(
            long,
            default_value_t = 100,
            help = "Delay between batches in ms (rate limit)"
        )]
        delay_ms: u64,
        #[arg(long, help = "Allow operations on open chats (higher ban risk)")]
        force: bool,
    },
    #[command(hide = true)]
    /// List members of a chat room via LOCO protocol (legacy command)
    LocoMembers { chat_id: i64 },
    #[command(hide = true)]
    /// Get chat room info via LOCO protocol (legacy command)
    LocoChatinfo { chat_id: i64 },
    /// List blocked/hidden-style members via LOCO protocol
    LocoBlocked,
    /// Probe a LOCO method and print the raw response
    Probe {
        method: String,
        #[arg(long, help = "JSON object body to send with the probe")]
        body: Option<String>,
    },
    #[command(hide = true)]
    /// Inspect cached friend/profile hints for LOCO reverse engineering
    ProfileHints {
        #[arg(
            long,
            help = "Include a local KakaoTalk app-state file snapshot for before/after diffing"
        )]
        app_state: bool,
        #[arg(
            long,
            help = "Compare the current app-state snapshot against a previous profile-hints JSON file"
        )]
        app_state_diff: Option<String>,
        #[arg(
            long,
            help = "Also build a local LOCO friend graph and correlate cache hints"
        )]
        local_graph: bool,
        #[arg(long, help = "Generate SYNCMAINPF body candidates for this user")]
        user_id: Option<i64>,
        #[arg(
            long,
            help = "Probe generated SYNCMAINPF candidates in one LOCO session"
        )]
        probe_syncmainpf: bool,
        #[arg(
            long,
            help = "Probe generated UPLINKPROF candidates in one LOCO session"
        )]
        probe_uplinkprof: bool,
    },
    #[command(hide = true)]
    /// Probe an arbitrary LOCO method and print the raw response (legacy command)
    LocoProbe {
        method: String,
        #[arg(long, help = "JSON object body to send with the probe")]
        body: Option<String>,
    },
    /// Watch Cache.db for fresh tokens (poll every N seconds)
    WatchCache {
        #[arg(long, default_value_t = 10)]
        interval: u64,
    },
    /// Run diagnostic checks on KakaoTalk installation and connectivity
    Doctor {
        /// Also test LOCO booking connectivity (makes network request)
        #[arg(long)]
        loco: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config()?;
    set_auth_policy(AuthPolicy::from_config(&config.auth));
    let json = cli.json;
    let unattended = cli.unattended || config.mode.unattended;
    let allow_non_interactive_send =
        cli.allow_non_interactive_send || config.send.allow_non_interactive;
    let allow_watch_side_effects = cli.allow_watch_side_effects || config.watch.allow_side_effects;
    let min_unattended_send_interval_secs = config
        .safety
        .min_unattended_send_interval_secs
        .unwrap_or(10);
    let min_hook_interval_secs = config.safety.min_hook_interval_secs.unwrap_or(2);
    let min_webhook_interval_secs = config.safety.min_webhook_interval_secs.unwrap_or(2);
    let hook_timeout_secs = config.safety.hook_timeout_secs.unwrap_or(20);
    let webhook_timeout_secs = config.safety.webhook_timeout_secs.unwrap_or(10);
    let no_prefix = if cli.no_prefix {
        true
    } else {
        matches!(config.send.default_prefix, Some(false))
    };

    // Respect NO_COLOR env var (https://no-color.org/) and --no-color flag
    if cli.no_color || std::env::var("NO_COLOR").is_ok() || json {
        NO_COLOR.store(true, Ordering::Relaxed);
    }

    match cli.command {
        Commands::Auth => cmd_auth(json)?,
        Commands::AuthStatus => cmd_auth_status(json)?,
        Commands::Login { save } => cmd_login(save)?,
        Commands::Me => cmd_me(json)?,
        Commands::Friends {
            favorites,
            hidden,
            search,
            local,
            chat_id,
            user_id,
        } => cmd_friends(favorites, hidden, search, local, chat_id, user_id, json)?,
        Commands::Chats {
            show_all,
            unread,
            search,
            chat_type,
            rest,
        } => cmd_chats(show_all, unread, search, chat_type, rest, json)?,
        Commands::Read {
            chat_id,
            count,
            before,
            cursor,
            since,
            all,
            delay_ms,
            force,
            rest,
        } => cmd_read(
            chat_id,
            ReadCommandOptions {
                count,
                cursor: cursor.or(before),
                since,
                all,
                delay_ms,
                force,
                rest,
                json,
            },
        )?,
        Commands::Members {
            chat_id,
            rest,
            full,
        } => cmd_members(chat_id, rest, full, json)?,
        Commands::Chatinfo { chat_id } => cmd_chatinfo(chat_id, json)?,
        Commands::Settings => cmd_settings(json)?,
        Commands::Scrap { url } => cmd_scrap(&url, json)?,
        Commands::Profile {
            user_id,
            chat_id,
            local,
        } => cmd_profile(user_id, chat_id, local, json)?,
        Commands::Favorite { user_id } => cmd_favorite(user_id)?,
        Commands::Unfavorite { user_id } => cmd_unfavorite(user_id)?,
        Commands::Hide { user_id } => cmd_hide(user_id)?,
        Commands::Unhide { user_id } => cmd_unhide(user_id)?,
        Commands::Profiles => cmd_profiles(json)?,
        Commands::Keywords => cmd_keywords(json)?,
        Commands::Unread => cmd_unread(json)?,
        Commands::Export {
            chat_id,
            format,
            output,
        } => cmd_export(chat_id, &format, output.as_deref())?,
        Commands::Search { chat_id, query } => cmd_search(chat_id, &query, json)?,
        Commands::Completions { shell } => {
            generate(
                shell,
                &mut Cli::command(),
                "openkakao-rs",
                &mut io::stdout(),
            );
        }
        Commands::Renew => cmd_renew(json)?,
        Commands::Relogin {
            fresh_xvc,
            password,
            email,
        } => cmd_relogin(json, fresh_xvc, password, email)?,
        Commands::LocoTest => {
            eprintln!("[deprecated] 'loco-test' is now hidden. Prefer 'doctor --loco'.");
            cmd_loco_test()?
        }
        Commands::Send {
            chat_id,
            message,
            force,
            yes,
        } => {
            let msg = format_outgoing_message(&message, no_prefix);
            cmd_send(
                chat_id,
                &msg,
                force,
                yes,
                unattended,
                allow_non_interactive_send,
                min_unattended_send_interval_secs,
            )?
        }
        Commands::SendPhoto {
            chat_id,
            file,
            force,
            yes,
        } => cmd_send_file(
            chat_id,
            &file,
            force,
            yes,
            unattended,
            allow_non_interactive_send,
            min_unattended_send_interval_secs,
        )?,
        Commands::SendFile {
            chat_id,
            file,
            force,
            yes,
        } => cmd_send_file(
            chat_id,
            &file,
            force,
            yes,
            unattended,
            allow_non_interactive_send,
            min_unattended_send_interval_secs,
        )?,
        Commands::Watch {
            chat_id,
            raw,
            read_receipt,
            max_reconnect,
            download_media,
            download_dir,
            hook_cmd,
            webhook_url,
            webhook_header,
            webhook_signing_secret,
            hook_chat_id,
            hook_keyword,
            hook_type,
            hook_fail_fast,
        } => cmd_watch(WatchOptions {
            unattended,
            allow_side_effects: allow_watch_side_effects,
            filter_chat_id: chat_id,
            raw,
            read_receipt,
            max_reconnect: config.watch.default_max_reconnect.unwrap_or(max_reconnect),
            download_media,
            download_dir,
            hook_cmd,
            webhook_url,
            webhook_headers: webhook_header,
            webhook_signing_secret,
            hook_chat_ids: hook_chat_id,
            hook_keywords: hook_keyword,
            hook_types: hook_type,
            hook_fail_fast,
            min_hook_interval_secs,
            min_webhook_interval_secs,
            hook_timeout_secs,
            webhook_timeout_secs,
            allow_insecure_webhooks: config.safety.allow_insecure_webhooks,
        })?,
        Commands::Download {
            chat_id,
            log_id,
            output_dir,
        } => cmd_download(chat_id, log_id, output_dir.as_deref())?,
        Commands::LocoChats { show_all } => {
            eprintln!("[deprecated] 'loco-chats' is now hidden. Prefer 'chats' (LOCO by default).");
            cmd_loco_chats(show_all, false, None, None, json)?
        }
        Commands::LocoRead {
            chat_id,
            count,
            cursor,
            since,
            all,
            delay_ms,
            force,
        } => {
            eprintln!("[deprecated] 'loco-read' is now hidden. Prefer 'read' (LOCO by default).");
            cmd_loco_read(
                chat_id,
                count,
                cursor,
                since.as_deref(),
                all,
                delay_ms,
                force,
                json,
            )?
        }
        Commands::LocoMembers { chat_id } => {
            eprintln!(
                "[deprecated] 'loco-members' is now hidden. Prefer 'members' (LOCO by default)."
            );
            cmd_loco_members(chat_id, false, json)?
        }
        Commands::LocoChatinfo { chat_id } => {
            eprintln!("[deprecated] 'loco-chatinfo' is now hidden. Prefer 'chatinfo'.");
            cmd_loco_chatinfo(chat_id, json)?
        }
        Commands::LocoBlocked => cmd_loco_blocked(json)?,
        Commands::Probe { method, body } => cmd_loco_probe(&method, body.as_deref(), json)?,
        Commands::ProfileHints {
            app_state,
            app_state_diff,
            local_graph,
            user_id,
            probe_syncmainpf,
            probe_uplinkprof,
        } => cmd_profile_hints(
            app_state,
            app_state_diff,
            local_graph,
            user_id,
            probe_syncmainpf,
            probe_uplinkprof,
            json,
        )?,
        Commands::LocoProbe { method, body } => {
            eprintln!("[deprecated] 'loco-probe' is now hidden. Prefer 'probe'.");
            cmd_loco_probe(&method, body.as_deref(), json)?
        }
        Commands::WatchCache { interval } => cmd_watch_cache(interval)?,
        Commands::Doctor { loco } => cmd_doctor(json, loco, &config)?,
    }

    Ok(())
}

fn cmd_auth(json: bool) -> Result<()> {
    let creds = get_creds()?;
    let client = KakaoRestClient::new(creds.clone())?;
    let valid = client.verify_token()?;

    if json {
        let out = serde_json::json!({
            "user_id": creds.user_id,
            "token_prefix": creds.oauth_token.chars().take(8).collect::<String>(),
            "app_version": creds.app_version,
            "valid": valid,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("  User ID: {}", creds.user_id);
    println!(
        "  Token:   {}...",
        creds.oauth_token.chars().take(8).collect::<String>()
    );
    println!("  Version: {}", creds.app_version);

    if valid {
        if color_enabled() {
            println!("  {}", "Token is valid!".green());
        } else {
            println!("  Token is valid!");
        }
    } else {
        if color_enabled() {
            println!("  {}", "Token is invalid or expired.".red());
        } else {
            println!("  Token is invalid or expired.");
        }
        println!(
            "  Hint: open KakaoTalk, open chat list once, then run 'openkakao-rs login --save'."
        );
    }

    Ok(())
}

fn cmd_auth_status(json: bool) -> Result<()> {
    let snapshot = recovery_snapshot()?;

    if json {
        let out = serde_json::json!({
            "path": snapshot.path,
            "last_success_at": snapshot.last_success_at,
            "last_success_transport": snapshot.last_success_transport,
            "last_recovery_source": snapshot.last_recovery_source,
            "last_failure_kind": snapshot.last_failure_kind,
            "last_failure_at": snapshot.last_failure_at,
            "consecutive_failures": snapshot.consecutive_failures,
            "cooldown_until": snapshot.cooldown_until,
            "auth_cooldown_remaining_secs": snapshot.auth_cooldown_remaining_secs,
            "relogin_available_in_secs": snapshot.relogin_available_in_secs,
            "renew_available_in_secs": snapshot.renew_available_in_secs,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("Auth recovery state");
    println!("  State file:            {}", snapshot.path);
    println!(
        "  Last success:          {}",
        snapshot.last_success_at.as_deref().unwrap_or("never")
    );
    println!(
        "  Last transport:        {}",
        snapshot
            .last_success_transport
            .as_deref()
            .unwrap_or("unknown")
    );
    println!(
        "  Last recovery source:  {}",
        snapshot.last_recovery_source.as_deref().unwrap_or("none")
    );
    println!(
        "  Last failure kind:     {}",
        snapshot.last_failure_kind.as_deref().unwrap_or("none")
    );
    println!(
        "  Last failure at:       {}",
        snapshot.last_failure_at.as_deref().unwrap_or("never")
    );
    println!("  Consecutive failures:  {}", snapshot.consecutive_failures);
    println!(
        "  Auth cooldown:         {}",
        format_remaining(
            snapshot.auth_cooldown_remaining_secs,
            snapshot.cooldown_until.as_deref()
        )
    );
    println!(
        "  Relogin available in:  {}",
        format_simple_remaining(snapshot.relogin_available_in_secs)
    );
    println!(
        "  Renew available in:    {}",
        format_simple_remaining(snapshot.renew_available_in_secs)
    );
    Ok(())
}

fn format_simple_remaining(value: Option<u64>) -> String {
    match value {
        Some(secs) => format!("{}s", secs),
        None => "now".to_string(),
    }
}

fn format_remaining(remaining_secs: Option<u64>, until: Option<&str>) -> String {
    match (remaining_secs, until) {
        (Some(secs), Some(until)) => format!("{}s (until {})", secs, until),
        (Some(secs), None) => format!("{}s", secs),
        _ => "none".to_string(),
    }
}

fn cmd_login(save: bool) -> Result<()> {
    let candidates = get_credential_candidates(8)?;
    let Some(_) = candidates.first() else {
        println!("Could not extract credentials. Is KakaoTalk running?");
        return Ok(());
    };
    let creds = select_best_credential(candidates)?;

    println!("Credentials extracted!");
    println!("  User ID: {}", creds.user_id);
    println!(
        "  Token:   {}...",
        creds.oauth_token.chars().take(8).collect::<String>()
    );

    let client = KakaoRestClient::new(creds.clone())?;
    if client.verify_token()? {
        println!("  Token verified OK");
    } else {
        println!("  Token may be expired for some operations");
    }

    if save {
        let path = save_credentials(&creds)?;
        println!("Credentials saved to {}", path.display());
    }

    Ok(())
}

fn cmd_me(json: bool) -> Result<()> {
    let rest_result = (|| -> Result<()> {
        let client = get_rest_client()?;
        let profile = client.get_my_profile()?;

        if json {
            println!("{}", serde_json::to_string_pretty(&profile)?);
            return Ok(());
        }

        print_section_title("My Profile");
        println!("  Source:   REST");
        println!("  Nickname: {}", profile.nickname);
        if !profile.status_message.is_empty() {
            println!("  Status:   {}", profile.status_message);
        }
        println!("  Email:    {}", profile.email);
        println!("  Account:  {}", profile.account_id);
        println!("  User ID:  {}", profile.user_id);
        if !profile.profile_image_url.is_empty() {
            println!("  Image:    {}", profile.profile_image_url);
        }
        Ok(())
    })();

    match rest_result {
        Ok(()) => Ok(()),
        Err(rest_err) => {
            eprintln!("[me] REST profile failed: {rest_err:#}. Trying local LOCO friend graph.");
            let creds = get_creds()?;
            let snapshot = build_local_friend_graph().map_err(|local_err| {
                anyhow::anyhow!(
                    "REST me failed: {rest_err:#}\nlocal LOCO fallback also failed: {local_err:#}"
                )
            })?;
            let profile = snapshot
                .entries
                .into_iter()
                .find(|entry| entry.user_id == creds.user_id || entry.is_self)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "REST me failed: {rest_err:#}\nlocal LOCO fallback could not find self profile"
                    )
                })?;

            if json {
                println!("{}", serde_json::to_string_pretty(&profile)?);
                return Ok(());
            }

            print_section_title("My Profile");
            println!("  Source:   local LOCO friend graph");
            println!("  Nickname: {}", profile.nickname);
            if !profile.status_message.is_empty() {
                println!("  Status:   {}", profile.status_message);
            }
            println!("  Account:  {}", profile.account_id);
            println!("  User ID:  {}", profile.user_id);
            if !profile.country_iso.is_empty() {
                println!("  Country:  {}", profile.country_iso);
            }
            if !profile.full_profile_image_url.is_empty() {
                println!("  Image:    {}", profile.full_profile_image_url);
            } else if !profile.profile_image_url.is_empty() {
                println!("  Image:    {}", profile.profile_image_url);
            }
            if !profile.chat_ids.is_empty() {
                println!(
                    "  Seen in:  {} chat(s) [{}]",
                    profile.chat_ids.len(),
                    profile
                        .chat_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Ok(())
        }
    }
}

fn filter_friend_search<T, F>(items: &mut Vec<T>, search: Option<String>, key: F)
where
    F: Fn(&T) -> (String, String),
{
    if let Some(query) = search {
        let q = query.to_lowercase();
        items.retain(|item| {
            let (primary, secondary) = key(item);
            primary.to_lowercase().contains(&q) || secondary.to_lowercase().contains(&q)
        });
    }
}

fn cmd_friends_local(
    favorites: bool,
    hidden: bool,
    search: Option<String>,
    chat_id: Option<i64>,
    user_id: Option<i64>,
    json: bool,
) -> Result<()> {
    if favorites {
        anyhow::bail!("friends --local does not support --favorites yet");
    }

    let mut snapshot = build_local_friend_graph()?;
    if hidden {
        let creds = get_creds()?;
        let rt = tokio::runtime::Runtime::new()?;
        let blocked = rt.block_on(async move {
            let mut client = loco::client::LocoClient::new(creds);
            loco_connect_with_auto_refresh(&mut client).await?;
            fetch_loco_blocked_snapshot(&mut client).await
        })?;
        merge_blocked_members_into_local_graph(&mut snapshot, blocked);
    }

    snapshot.entries.retain(|entry| !entry.is_self);
    if let Some(chat_id) = chat_id {
        snapshot
            .entries
            .retain(|entry| entry.chat_ids.contains(&chat_id));
    }
    if let Some(user_id) = user_id {
        snapshot.entries.retain(|entry| entry.user_id == user_id);
    }
    if hidden {
        snapshot.entries.retain(|entry| entry.hidden_like);
    }
    filter_friend_search(&mut snapshot.entries, search, |entry| {
        (entry.nickname.clone(), entry.status_message.clone())
    });

    if json {
        println!("{}", serde_json::to_string_pretty(&snapshot.entries)?);
        return Ok(());
    }

    let rows = snapshot
        .entries
        .iter()
        .map(|entry| {
            vec![
                entry.nickname.clone(),
                truncate(&entry.status_message, 30),
                entry.chat_ids.len().to_string(),
                entry.country_iso.clone(),
                entry
                    .hidden_block_type
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                entry.user_id.to_string(),
            ]
        })
        .collect::<Vec<_>>();

    let title = if hidden {
        format!("Local hidden-like friends ({})", rows.len())
    } else {
        format!("Local friends ({})", rows.len())
    };
    print_section_title(&title);
    if !snapshot.failed_chat_ids.is_empty() {
        println!(
            "  note: skipped {} chats with GETMEM failures",
            snapshot.failed_chat_ids.len()
        );
    }
    if hidden {
        println!("  note: hidden output is inferred from LOCO BLSYNC/BLMEMBER and may include blocked-style entries.");
    }
    print_table(
        &["Name", "Status", "Chats", "Country", "Type", "User ID"],
        rows,
    );
    Ok(())
}

fn cmd_friends(
    favorites: bool,
    hidden: bool,
    search: Option<String>,
    local: bool,
    chat_id: Option<i64>,
    user_id: Option<i64>,
    json: bool,
) -> Result<()> {
    if local {
        return cmd_friends_local(favorites, hidden, search, chat_id, user_id, json);
    }

    if chat_id.is_some() || user_id.is_some() {
        anyhow::bail!("--chat-id and --user-id require --local");
    }

    let client = get_rest_client()?;
    let mut friends = client.get_friends()?;

    if favorites {
        friends.retain(|f| f.favorite);
    }

    if !hidden {
        friends.retain(|f| !f.hidden);
    }

    filter_friend_search(&mut friends, search, |friend| {
        (friend.display_name(), friend.phone_number.clone())
    });

    if json {
        println!("{}", serde_json::to_string_pretty(&friends)?);
        return Ok(());
    }

    let mut rows = Vec::new();
    for f in friends {
        let mut name = f.display_name();
        if f.favorite {
            name.push_str(" *");
        }
        let status = truncate(&f.status_message, 30);
        rows.push(vec![name, status, f.phone_number, f.user_id.to_string()]);
    }

    print_section_title(&format!("Friends ({})", rows.len()));
    print_table(&["Name", "Status", "Phone", "User ID"], rows);
    Ok(())
}

fn cmd_chats_rest(
    show_all: bool,
    unread: bool,
    search: Option<String>,
    chat_type: Option<String>,
    json: bool,
) -> Result<()> {
    let client = get_rest_client()?;

    let mut chats = if show_all {
        client.get_all_chats()?
    } else {
        client.get_chats(None)?.0
    };

    if unread {
        chats.retain(|c| c.unread_count > 0);
    }

    if let Some(ref query) = search {
        let q = query.to_lowercase();
        chats.retain(|c| c.display_title().to_lowercase().contains(&q));
    }

    if let Some(ref t) = chat_type {
        let lowered = t.to_lowercase();
        let kind = match lowered.as_str() {
            "dm" => "DirectChat".to_string(),
            "group" => "MultiChat".to_string(),
            "memo" => "MemoChat".to_string(),
            "open" => "OpenMultiChat".to_string(),
            "opendm" => "OpenDirectChat".to_string(),
            other => other.to_string(),
        };
        chats.retain(|c| c.kind == kind);
    }

    let listings = chats
        .into_iter()
        .map(|chat| {
            let title = chat.display_title();
            let active_members = chat.display_members.len() as i32;
            ChatListing {
                chat_id: chat.chat_id,
                kind: chat.kind,
                title,
                has_unread: chat.unread_count > 0,
                unread_count: Some(chat.unread_count),
                active_members: Some(active_members),
                last_log_id: None,
                last_seen_log_id: None,
            }
        })
        .collect::<Vec<_>>();

    if json {
        println!("{}", serde_json::to_string_pretty(&listings)?);
        return Ok(());
    }

    let mut rows = Vec::new();
    for c in listings {
        let kind = type_label(&c.kind);
        let unread_str = if c.has_unread {
            c.unread_count.unwrap_or(1).to_string()
        } else {
            String::new()
        };

        rows.push(vec![
            kind.to_string(),
            c.title,
            unread_str,
            c.chat_id.to_string(),
        ]);
    }

    print_section_title(&format!("Chats ({})", rows.len()));
    print_table(&["Type", "Name", "Unread", "Chat ID"], rows);
    Ok(())
}

fn cmd_chats(
    show_all: bool,
    unread: bool,
    search: Option<String>,
    chat_type: Option<String>,
    rest: bool,
    json: bool,
) -> Result<()> {
    if rest {
        return cmd_chats_rest(show_all, unread, search, chat_type, json);
    }

    match cmd_loco_chats(show_all, unread, search.clone(), chat_type.clone(), json) {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!(
                "[chats] LOCO chat list failed: {}. Falling back to REST recent chat list.",
                err
            );
            cmd_chats_rest(show_all, unread, search, chat_type, json)
        }
    }
}

fn cmd_read_rest(
    chat_id: i64,
    count: usize,
    cursor: Option<i64>,
    since: Option<&str>,
    all: bool,
    json: bool,
) -> Result<()> {
    let since_ts = parse_since_date(since)?;

    let creds = get_creds()?;
    let client = KakaoRestClient::new(creds.clone())?;

    let mut messages = if all {
        client.get_all_messages(chat_id, 100)?
    } else {
        let (msgs, _next_cursor) = client.get_messages(chat_id, cursor)?;
        msgs
    };

    // Apply --since filter
    if let Some(ts) = since_ts {
        messages.retain(|m| m.send_at >= ts);
    }

    let member_map = match client.get_chat_members(chat_id) {
        Ok(members) => member_name_map(&members, creds.user_id),
        Err(_) => {
            let mut fallback = HashMap::new();
            fallback.insert(creds.user_id, "Me".to_string());
            fallback
        }
    };

    if !all {
        if messages.len() > count {
            messages.truncate(count);
        }
        messages.reverse();
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&messages)?);
        return Ok(());
    }

    if messages.is_empty() {
        println!("No messages.");
        return Ok(());
    }

    for msg in &messages {
        let name = member_map
            .get(&msg.author_id)
            .cloned()
            .unwrap_or_else(|| msg.author_id.to_string());
        let time_str = format_time(msg.send_at);

        let body = match msg.message_type {
            1 => msg.message.clone(),
            2 => "(photo)".to_string(),
            71 => "(emoticon)".to_string(),
            _ => {
                if msg.message.is_empty() {
                    format!("(type={})", msg.message_type)
                } else {
                    msg.message.clone()
                }
            }
        };

        if color_enabled() {
            println!("{} [{}]: {}", time_str.dimmed(), name.bold(), body);
        } else {
            println!("{} [{}]: {}", time_str, name, body);
        }
    }

    if !all {
        if let Some(oldest) = messages.first().map(|m| m.log_id) {
            println!(
                "\nShowing {} messages. For older: openkakao-rs read {} --cursor {}",
                messages.len(),
                chat_id,
                oldest
            );
        }
    } else {
        println!("\nTotal: {} messages", messages.len());
    }

    Ok(())
}

fn cmd_read(chat_id: i64, options: ReadCommandOptions) -> Result<()> {
    if options.rest {
        return cmd_read_rest(
            chat_id,
            options.count,
            options.cursor,
            options.since.as_deref(),
            options.all,
            options.json,
        );
    }

    match cmd_loco_read(
        chat_id,
        options.count as i32,
        options.cursor,
        options.since.as_deref(),
        options.all,
        options.delay_ms,
        options.force,
        options.json,
    ) {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!(
                "[read] LOCO read failed: {}. Falling back to REST cache-backed read.",
                err
            );
            if options.force {
                eprintln!(
                    "[read] Note: --force only applies to LOCO and is ignored for REST fallback."
                );
            }
            if options.delay_ms != 100 {
                eprintln!(
                    "[read] Note: --delay-ms only applies to LOCO and is ignored for REST fallback."
                );
            }
            cmd_read_rest(
                chat_id,
                options.count,
                options.cursor,
                options.since.as_deref(),
                options.all,
                options.json,
            )
        }
    }
}

fn cmd_members_rest(chat_id: i64, json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let members = client.get_chat_members(chat_id)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&members)?);
        return Ok(());
    }

    let mut rows = Vec::new();
    for m in members {
        rows.push(vec![m.display_name(), m.user_id.to_string(), m.country_iso]);
    }

    print_section_title(&format!("Members ({})", rows.len()));
    print_table(&["Name", "User ID", "Country"], rows);
    Ok(())
}

fn cmd_members(chat_id: i64, rest: bool, full: bool, json: bool) -> Result<()> {
    if rest {
        return cmd_members_rest(chat_id, json);
    }

    match cmd_loco_members(chat_id, full, json) {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!(
                "[members] LOCO member list failed: {err:#}. Falling back to REST member list."
            );
            cmd_members_rest(chat_id, json)
        }
    }
}

fn cmd_chatinfo(chat_id: i64, json: bool) -> Result<()> {
    cmd_loco_chatinfo(chat_id, json)
}

fn cmd_settings(json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let settings = client.get_settings()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&settings)?);
        return Ok(());
    }

    print_section_title("Account Settings");
    println!("  Status:    {}", json_i64(&settings, "status"));
    println!("  Account:   {}", json_i64(&settings, "accountId"));
    println!("  Email:     {}", json_string(&settings, "emailAddress"));
    println!("  Country:   {}", json_string(&settings, "countryIso"));
    println!("  Version:   {}", json_string(&settings, "recentVersion"));
    println!("  Server:    {}", json_string(&settings, "server_time"));

    let profile = settings.get("profile").cloned().unwrap_or(Value::Null);
    if !profile.is_null() {
        println!("\n  Nickname:  {}", json_string(&profile, "nickname"));
        println!("  Status:    {}", json_string(&profile, "statusMessage"));
    }

    Ok(())
}

fn cmd_scrap(url: &str, json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let data = client.get_scrap_preview(url)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    print_section_title("Link Preview");
    println!("  Title: {}", json_string(&data, "title"));

    let description = json_string(&data, "description");
    if !description.is_empty() {
        println!("  Desc:  {}", truncate(&description, 200));
    }

    let canonical = json_string(&data, "canonicalUrl");
    if canonical.is_empty() {
        println!("  URL:   {}", url);
    } else {
        println!("  URL:   {}", canonical);
    }

    let image = json_string(&data, "mainImageUrl");
    if !image.is_empty() {
        println!("  Image: {}", image);
    }

    Ok(())
}

fn cmd_profile_rest(user_id: i64, json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let data = client.get_friend_profile(user_id)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let profile = data.get("profile").cloned().unwrap_or(Value::Null);
    print_section_title("Friend Profile");
    println!("  Nickname: {}", json_string(&profile, "nickname"));
    let status = json_string(&profile, "statusMessage");
    if !status.is_empty() {
        println!("  Status:   {}", status);
    }
    let image = json_string(&profile, "fullProfileImageUrl");
    if !image.is_empty() {
        println!("  Image:    {}", image);
    }

    Ok(())
}

fn cmd_profile_loco(chat_id: i64, user_id: i64, json: bool) -> Result<()> {
    let profiles = fetch_loco_member_profiles(chat_id)?;
    let profile = profiles
        .into_iter()
        .find(|profile| profile.user_id == user_id)
        .ok_or_else(|| anyhow::anyhow!("user {} not found in chat {}", user_id, chat_id))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
        return Ok(());
    }

    print_section_title("Friend Profile");
    println!("  Source:   LOCO GETMEM");
    println!("  Chat ID:  {}", chat_id);
    println!("  User ID:  {}", profile.user_id);
    println!("  Account:  {}", profile.account_id);
    println!("  Nickname: {}", profile.nickname);
    if !profile.status_message.is_empty() {
        println!("  Status:   {}", profile.status_message);
    }
    if !profile.country_iso.is_empty() {
        println!("  Country:  {}", profile.country_iso);
    }
    if !profile.full_profile_image_url.is_empty() {
        println!("  Image:    {}", profile.full_profile_image_url);
    } else if !profile.profile_image_url.is_empty() {
        println!("  Image:    {}", profile.profile_image_url);
    }
    if !profile.access_permit.is_empty() {
        println!("  Permit:   {}", profile.access_permit);
    }
    if !profile.suspicion.is_empty() {
        println!("  Suspicion: {}", profile.suspicion);
    }
    println!(
        "  Flags:    suspended={}, memorial={}",
        profile.suspended, profile.memorial
    );

    Ok(())
}

fn cmd_profile_local(user_id: i64, json: bool) -> Result<()> {
    let snapshot = build_local_friend_graph()?;
    let profile = snapshot
        .entries
        .into_iter()
        .find(|entry| entry.user_id == user_id)
        .ok_or_else(|| anyhow::anyhow!("user {} not found in local LOCO friend graph", user_id))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
        return Ok(());
    }

    print_section_title("Friend Profile");
    println!("  Source:   local LOCO friend graph");
    println!("  User ID:  {}", profile.user_id);
    println!("  Account:  {}", profile.account_id);
    println!("  Nickname: {}", profile.nickname);
    if !profile.status_message.is_empty() {
        println!("  Status:   {}", profile.status_message);
    }
    if !profile.country_iso.is_empty() {
        println!("  Country:  {}", profile.country_iso);
    }
    if !profile.full_profile_image_url.is_empty() {
        println!("  Image:    {}", profile.full_profile_image_url);
    } else if !profile.profile_image_url.is_empty() {
        println!("  Image:    {}", profile.profile_image_url);
    }
    if !profile.access_permits.is_empty() {
        println!("  Permit(s): {}", profile.access_permits.join(", "));
    }
    if !profile.chat_ids.is_empty() {
        println!(
            "  Seen in:  {} chat(s) [{}]",
            profile.chat_ids.len(),
            profile
                .chat_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

fn cmd_profile(user_id: i64, chat_id: Option<i64>, local: bool, json: bool) -> Result<()> {
    if let Some(chat_id) = chat_id {
        match cmd_profile_loco(chat_id, user_id, json) {
            Ok(()) => return Ok(()),
            Err(err) => {
                eprintln!(
                    "[profile] LOCO chat-scoped profile failed: {err:#}. Falling back to local graph / REST profile."
                );
            }
        }
    }

    if local {
        match cmd_profile_local(user_id, json) {
            Ok(()) => return Ok(()),
            Err(err) => {
                eprintln!(
                    "[profile] local LOCO friend graph lookup failed: {err:#}. Falling back to REST profile."
                );
            }
        }
    }

    match cmd_profile_rest(user_id, json) {
        Ok(()) => Ok(()),
        Err(rest_err) => {
            eprintln!(
                "[profile] REST profile failed: {rest_err:#}. Trying local LOCO friend graph."
            );
            cmd_profile_local(user_id, json).map_err(|local_err| {
                anyhow::anyhow!(
                    "REST profile failed: {rest_err:#}\nlocal LOCO fallback also failed: {local_err:#}"
                )
            })
        }
    }
}

fn cmd_favorite(user_id: i64) -> Result<()> {
    eprint!("Add user {} to favorites? [y/N] ", user_id);
    if !confirm()? {
        println!("Cancelled.");
        return Ok(());
    }
    let client = get_rest_client()?;
    client.add_favorite(user_id)?;
    println!("Added user {} to favorites.", user_id);
    Ok(())
}

fn cmd_unfavorite(user_id: i64) -> Result<()> {
    eprint!("Remove user {} from favorites? [y/N] ", user_id);
    if !confirm()? {
        println!("Cancelled.");
        return Ok(());
    }
    let client = get_rest_client()?;
    client.remove_favorite(user_id)?;
    println!("Removed user {} from favorites.", user_id);
    Ok(())
}

fn cmd_hide(user_id: i64) -> Result<()> {
    eprint!("Hide user {}? [y/N] ", user_id);
    if !confirm()? {
        println!("Cancelled.");
        return Ok(());
    }
    let client = get_rest_client()?;
    client.hide_friend(user_id)?;
    println!("Hidden user {}.", user_id);
    Ok(())
}

fn cmd_unhide(user_id: i64) -> Result<()> {
    eprint!("Unhide user {}? [y/N] ", user_id);
    if !confirm()? {
        println!("Cancelled.");
        return Ok(());
    }
    let client = get_rest_client()?;
    client.unhide_friend(user_id)?;
    println!("Unhidden user {}.", user_id);
    Ok(())
}

fn cmd_profiles(json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let data = client.get_profiles()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let profiles = data.get("profiles").and_then(Value::as_array);
    match profiles {
        Some(arr) if !arr.is_empty() => {
            println!("Profile Cards ({})", arr.len());
            for p in arr {
                println!(
                    "  - {} ({})",
                    json_string(p, "nickname"),
                    json_string(p, "statusMessage")
                );
            }
        }
        _ => println!("No profile cards found."),
    }

    Ok(())
}

fn cmd_keywords(json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let data = client.get_alarm_keywords()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let keywords = data.get("alarm_keywords").and_then(Value::as_array);
    match keywords {
        Some(arr) if !arr.is_empty() => {
            println!("Alarm Keywords ({})", arr.len());
            for kw in arr {
                if let Some(s) = kw.as_str() {
                    println!("  - {}", s);
                } else {
                    println!("  - {}", kw);
                }
            }
        }
        _ => println!("No alarm keywords set."),
    }

    Ok(())
}

fn cmd_unread(json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let chats = client.get_all_chats()?;

    let unread: Vec<_> = chats.into_iter().filter(|c| c.unread_count > 0).collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&unread)?);
        return Ok(());
    }

    if unread.is_empty() {
        println!("No unread chats.");
        return Ok(());
    }

    let total: i64 = unread.iter().map(|c| c.unread_count).sum();
    print_section_title(&format!(
        "Unread Summary ({} chats, {} messages)",
        unread.len(),
        total
    ));

    let mut rows = Vec::new();
    for c in unread {
        rows.push(vec![
            type_label(&c.kind).to_string(),
            c.display_title(),
            c.unread_count.to_string(),
            c.chat_id.to_string(),
        ]);
    }
    print_table(&["Type", "Name", "Unread", "Chat ID"], rows);
    Ok(())
}

fn cmd_export(chat_id: i64, format: &str, output: Option<&str>) -> Result<()> {
    let fmt = ExportFormat::from_str(format)?;
    let creds = get_creds()?;
    let my_user_id = creds.user_id;
    let client = KakaoRestClient::new(creds)?;

    eprintln!("Fetching all messages for chat {}...", chat_id);
    let messages = client.get_all_messages(chat_id, 100)?;
    let members = client.get_chat_members(chat_id).unwrap_or_default();

    if messages.is_empty() {
        eprintln!("No messages found. The pilsner server only caches recently opened chats.");
        return Ok(());
    }

    eprintln!("Exporting {} messages...", messages.len());
    export::export_messages(&messages, &members, my_user_id, &fmt, output)?;

    if let Some(path) = output {
        eprintln!("Exported to {}", path);
    }

    Ok(())
}

fn cmd_search(chat_id: i64, query: &str, json: bool) -> Result<()> {
    let creds = get_creds()?;
    let client = KakaoRestClient::new(creds.clone())?;

    eprintln!("Fetching messages for chat {}...", chat_id);
    eprintln!("Note: pilsner server only caches messages from recently opened chats.");

    let messages = client.get_all_messages(chat_id, 100)?;

    let q = query.to_lowercase();
    let matched: Vec<_> = messages
        .into_iter()
        .filter(|m| m.message.to_lowercase().contains(&q))
        .collect();

    let member_map = match client.get_chat_members(chat_id) {
        Ok(members) => member_name_map(&members, creds.user_id),
        Err(_) => HashMap::new(),
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&matched)?);
        return Ok(());
    }

    if matched.is_empty() {
        println!("No messages matching '{}'.", query);
        return Ok(());
    }

    print_section_title(&format!(
        "Search results for '{}' ({} matches)",
        query,
        matched.len()
    ));
    for msg in &matched {
        let name = member_map
            .get(&msg.author_id)
            .cloned()
            .unwrap_or_else(|| msg.author_id.to_string());
        let time_str = format_time(msg.send_at);
        if color_enabled() {
            println!("{} [{}]: {}", time_str.dimmed(), name.bold(), msg.message);
        } else {
            println!("{} [{}]: {}", time_str, name, msg.message);
        }
    }

    Ok(())
}

fn cmd_renew(json: bool) -> Result<()> {
    let creds = get_creds()?;
    eprintln!("Trying refresh_token renewal...");

    match attempt_renew(&creds)? {
        RecoveryAttempt::Unavailable { reason, .. } => {
            eprintln!("  {}.", reason);
            eprintln!("  Hint: Open KakaoTalk app and wait for it to auto-renew, then retry.");
        }
        RecoveryAttempt::Failed {
            source,
            detail,
            response,
        } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "outcome": "failed",
                        "source": source,
                        "detail": detail,
                        "response": response,
                    }))?
                );
            } else {
                eprintln!("Token renewal failed via {}.", source);
                eprintln!("Detail: {}", detail);
                if let Some(response) = response {
                    eprintln!("Response: {}", serde_json::to_string_pretty(&response)?);
                }
            }
        }
        RecoveryAttempt::Recovered {
            source,
            credentials,
            response,
        } => {
            save_credentials(&credentials)?;
            return print_renew_result(json, source, &response);
        }
    }

    Ok(())
}

fn print_renew_result(json: bool, source: &str, response: &Value) -> Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "outcome": "recovered",
                "source": source,
                "response": response,
            }))?
        );
    } else {
        eprintln!("  Token renewed successfully via {}!", source);
        if let Some(access) = response.get("access_token").and_then(Value::as_str) {
            println!("New access_token: {}...", &access[..8.min(access.len())]);
        }
        if let Some(refresh) = response.get("refresh_token").and_then(Value::as_str) {
            println!("New refresh_token: {}...", &refresh[..8.min(refresh.len())]);
        }
        if let Some(obj) = response.as_object() {
            for (k, v) in obj {
                if k == "access_token" || k == "refresh_token" || k == "status" {
                    continue;
                }
                eprintln!("  {}: {}", k, v);
            }
        }
    }
    Ok(())
}

fn cmd_relogin(
    json: bool,
    fresh_xvc: bool,
    password_override: Option<String>,
    email_override: Option<String>,
) -> Result<()> {
    let creds = get_creds()?;
    eprintln!("Extracting login.json parameters from Cache.db...");
    match attempt_relogin(
        &creds,
        fresh_xvc,
        password_override.as_deref(),
        email_override.as_deref(),
    )? {
        RecoveryAttempt::Unavailable { reason, .. } => {
            eprintln!("  {}.", reason);
        }
        RecoveryAttempt::Failed {
            source,
            detail,
            response,
        } => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "outcome": "failed",
                        "source": source,
                        "detail": detail,
                        "response": response,
                    }))?
                );
                return Ok(());
            }

            eprintln!("  Relogin failed via {}.", source);
            eprintln!("  Detail: {}", detail);
            if let Some(response) = response {
                let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);
                let msg = response
                    .get("message")
                    .or_else(|| response.get("msg"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !msg.is_empty() {
                    eprintln!("  Server message: {} (status={})", msg, status);
                }
            }
        }
        RecoveryAttempt::Recovered {
            source,
            credentials,
            response,
        } => {
            save_credentials(&credentials)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "outcome": "recovered",
                        "source": source,
                        "response": response,
                    }))?
                );
                return Ok(());
            }

            let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);
            eprintln!("  Status: {}", status);
            if let Some(access) = response.get("access_token").and_then(Value::as_str) {
                eprintln!("  access_token: {}...", &access[..8.min(access.len())]);
            }
            if let Some(refresh) = response.get("refresh_token").and_then(Value::as_str) {
                eprintln!("  refresh_token: {}...", &refresh[..8.min(refresh.len())]);
            }
            eprintln!("  Credentials saved via {}.", source);
        }
    }

    Ok(())
}

fn cmd_loco_test() -> Result<()> {
    let creds = get_creds()?;

    eprintln!("Testing LOCO connection for user {}...", creds.user_id);
    eprintln!(
        "  Token: {}...",
        creds.oauth_token.chars().take(8).collect::<String>()
    );

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds.clone());
        let login_data = client.full_connect_with_retry(3).await?;

        let status = login_data
            .get_i64("status")
            .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
            .unwrap_or(-1);
        let user_id = login_data
            .get_i64("userId")
            .or_else(|_| login_data.get_i32("userId").map(|v| v as i64))
            .unwrap_or(0);

        if status == 0 && user_id > 0 {
            println!("LOCO connection successful!");
            println!("  User ID: {}", user_id);

            if let Ok(chat_datas) = login_data.get_array("chatDatas") {
                println!("  Chat rooms: {}", chat_datas.len());
                for cd in chat_datas.iter() {
                    if let Some(doc) = cd.as_document() {
                        let cid = doc
                            .get_i64("c")
                            .or_else(|_| doc.get_i32("c").map(|v| v as i64))
                            .unwrap_or(0);
                        let ctype = doc.get_str("t").unwrap_or("?");
                        let members = doc.get_array("m").map(|a| a.len()).unwrap_or(0);
                        let li = doc.get_i64("ll").unwrap_or(0);
                        println!(
                            "    {} (type={}, members={}, lastLog={})",
                            cid, ctype, members, li
                        );
                    }
                }
            }
        } else {
            println!("LOCO login returned status={}", status);
            print_loco_error_hint(status);

            // Print the full response for debugging
            eprintln!("\nFull LOGINLIST response:");
            for (k, v) in login_data.iter() {
                if k != "chatDatas" && k != "revision" {
                    eprintln!("  {}: {:?}", k, v);
                }
            }
        }

        Ok(())
    })
}

/// Try to renew token via REST API. Returns the new access_token if successful.
fn try_renew_token(creds: &KakaoCredentials, refresh_token: &str) -> Result<Option<String>> {
    let rest = KakaoRestClient::new(creds.clone())?;

    // Try oauth2_token.json first (sends both access_token + refresh_token)
    eprintln!("[renew] Trying oauth2_token.json...");
    if let Ok(response) = rest.oauth2_token(refresh_token) {
        let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);
        eprintln!("[renew] oauth2_token.json status: {}", status);
        if status == 0 {
            return Ok(response
                .get("access_token")
                .and_then(Value::as_str)
                .map(String::from));
        }
    }

    // Fallback: try renew_token.json
    eprintln!("[renew] Trying renew_token.json...");
    let response = rest.renew_token(refresh_token)?;
    let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);
    eprintln!("[renew] renew_token.json status: {}", status);

    if let Some(obj) = response.as_object() {
        for (k, v) in obj {
            let v_str = format!("{}", v);
            if v_str.len() > 60 {
                eprintln!("  {}: {}...", k, &v_str[..60]);
            } else {
                eprintln!("  {}: {}", k, v);
            }
        }
    }

    if status != 0 {
        return Ok(None);
    }

    Ok(response
        .get("access_token")
        .and_then(Value::as_str)
        .map(String::from))
}

fn cmd_send(
    chat_id: i64,
    message: &str,
    force: bool,
    skip_confirm: bool,
    unattended: bool,
    allow_non_interactive_send: bool,
    min_unattended_send_interval_secs: u64,
) -> Result<()> {
    validate_outbound_message(message)?;
    if skip_confirm {
        require_permission(
            unattended && allow_non_interactive_send,
            "non-interactive send (-y/--yes)",
            "Re-run with --unattended --allow-non-interactive-send, or set both in ~/.config/openkakao/config.toml.",
        )?;
        if let Some(remaining) = unattended_send_remaining_secs(min_unattended_send_interval_secs)?
        {
            record_guard("unattended_send_rate_limited")?;
            anyhow::bail!(
                "unattended send is rate-limited for {}s; wait or raise safety.min_unattended_send_interval_secs",
                remaining
            );
        }
        mark_unattended_send_attempt()?;
    }
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        eprintln!("Connecting via LOCO...");
        loco_connect_with_auto_refresh(&mut client).await?;

        // Check chat type for open chat safety
        let room_info = client
            .send_command("CHATONROOM", bson::doc! { "chatId": chat_id })
            .await?;
        let chat_type = extract_chat_type(&room_info.body);
        let label = type_label(&chat_type);

        if is_open_chat(&chat_type) && !force {
            eprintln!(
                "Blocked: chat {} is {} (open chat). Open chats have higher ban risk.",
                chat_id, label
            );
            eprintln!("Use --force to override this safety check.");
            anyhow::bail!("Open chat send blocked (use --force to override)");
        }

        if is_open_chat(&chat_type) {
            eprintln!(
                "Warning: sending to {} (open chat). Proceed with caution.",
                label
            );
        }

        if !skip_confirm {
            eprint!(
                "Send to {} chat {}? Message: \"{}\"\n[y/N] ",
                label,
                chat_id,
                truncate(message, 50)
            );
            if !confirm()? {
                println!("Cancelled.");
                return Ok(());
            }
        }

        let response = client
            .send_command(
                "WRITE",
                bson::doc! {
                    "chatId": chat_id,
                    "msg": message,
                    "type": 1_i32,
                    "noSeen": false,
                },
            )
            .await?;

        let status = response.status();
        if status == 0 {
            println!("Message sent!");
        } else {
            println!("Send failed (status={})", status);
            eprintln!("Response: {:?}", response.body);
        }

        Ok(())
    })
}

fn cmd_send_file(
    chat_id: i64,
    file_path: &str,
    force: bool,
    skip_confirm: bool,
    unattended: bool,
    allow_non_interactive_send: bool,
    min_unattended_send_interval_secs: u64,
) -> Result<()> {
    if skip_confirm {
        require_permission(
            unattended && allow_non_interactive_send,
            "non-interactive file send (-y/--yes)",
            "Re-run with --unattended --allow-non-interactive-send, or set both in ~/.config/openkakao/config.toml.",
        )?;
        if let Some(remaining) = unattended_send_remaining_secs(min_unattended_send_interval_secs)?
        {
            record_guard("unattended_send_rate_limited")?;
            anyhow::bail!(
                "unattended send is rate-limited for {}s; wait or raise safety.min_unattended_send_interval_secs",
                remaining
            );
        }
        mark_unattended_send_attempt()?;
    }
    let path = Path::new(file_path);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file_path);
    }

    let data = std::fs::read(path)?;
    if data.len() < 4 {
        anyhow::bail!("File too small: {} bytes", data.len());
    }

    // Detect file type from magic bytes, then fall back to extension
    let file_ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let (msg_type, ext_str) = detect_media_type(&data, &file_ext);
    let ext = ext_str.as_str();

    let type_label_str = match msg_type {
        2 => "photo",
        3 => "video",
        14 => "gif",
        26 => "file",
        _ => "file",
    };

    // Get dimensions for images
    let (width, height) = match (msg_type, ext) {
        (2, "jpg") => jpeg_dimensions(&data).unwrap_or((0, 0)),
        (2, "png") => png_dimensions(&data).unwrap_or((0, 0)),
        _ => (0, 0),
    };

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("upload.{}", ext));

    eprintln!(
        "{}: {} ({} bytes, {}x{}, type={})",
        type_label_str,
        file_name,
        data.len(),
        width,
        height,
        msg_type
    );

    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds.clone());
        eprintln!("Connecting via LOCO...");
        loco_connect_with_auto_refresh(&mut client).await?;

        // Check chat type for open chat safety
        let room_info = client
            .send_command("CHATONROOM", bson::doc! { "chatId": chat_id })
            .await?;
        let chat_type = extract_chat_type(&room_info.body);
        let label = type_label(&chat_type);

        if is_open_chat(&chat_type) && !force {
            anyhow::bail!(
                "Blocked: chat {} is {} (open chat). Use --force to override.",
                chat_id,
                label
            );
        }

        if !skip_confirm {
            eprint!(
                "Send {} ({}) to {} chat {}?\n[y/N] ",
                file_name, type_label_str, label, chat_id
            );
            if !confirm()? {
                println!("Cancelled.");
                return Ok(());
            }
        }

        // Step 1: SHIP — request upload slot
        let checksum = {
            use sha1::Digest;
            let hash = sha1::Sha1::digest(&data);
            hex::encode(hash)
        };

        let ship_resp = client
            .send_command(
                "SHIP",
                bson::doc! {
                    "c": chat_id,
                    "s": data.len() as i64,
                    "t": msg_type,
                    "cs": &checksum,
                    "e": ext,
                    "ex": "{}",
                },
            )
            .await?;

        let ship_status = ship_resp.status();
        if ship_status != 0 {
            anyhow::bail!("SHIP failed (status={}): {:?}", ship_status, ship_resp.body);
        }

        let upload_key = ship_resp
            .body
            .get_str("k")
            .map_err(|_| anyhow::anyhow!("No key in SHIP response"))?
            .to_string();
        let vhost = ship_resp
            .body
            .get_str("vh")
            .map_err(|_| anyhow::anyhow!("No vhost in SHIP response"))?
            .to_string();
        let upload_port = ship_resp.body.get_i32("p").map(|p| p as u16).unwrap_or(443);

        eprintln!(
            "[ship] Upload server: {}:{}, key: {}",
            vhost, upload_port, upload_key
        );

        // Step 2: Upload via separate LOCO connection
        loco::client::loco_upload(
            &vhost,
            upload_port,
            creds.user_id,
            &upload_key,
            chat_id,
            &data,
            msg_type,
            width,
            height,
            &creds.app_version,
        )
        .await?;

        println!(
            "{} sent!",
            type_label_str[..1].to_uppercase() + &type_label_str[1..]
        );
        Ok(())
    })
}

/// Detect LOCO message type and extension from magic bytes and file extension.
fn detect_media_type(data: &[u8], file_ext: &str) -> (i32, String) {
    // Magic bytes detection
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        return (2, "jpg".into());
    }
    if data.len() >= 8 && &data[..8] == b"\x89PNG\r\n\x1a\n" {
        return (2, "png".into());
    }
    if data.len() >= 4 && &data[..4] == b"GIF8" {
        return (14, "gif".into());
    }
    // Video: ftyp box (MP4/MOV/3GP)
    if data.len() >= 8 && &data[4..8] == b"ftyp" {
        return (3, if file_ext == "mov" { "mov" } else { "mp4" }.into());
    }
    // WebM
    if data.len() >= 4 && &data[..4] == b"\x1a\x45\xdf\xa3" {
        return (3, "webm".into());
    }

    // Fall back to extension
    match file_ext {
        "jpg" | "jpeg" => (2, "jpg".into()),
        "png" => (2, "png".into()),
        "gif" => (14, "gif".into()),
        "mp4" | "mov" | "avi" | "mkv" | "webm" => (3, file_ext.into()),
        "m4a" | "aac" | "mp3" | "wav" | "ogg" => (12, file_ext.into()),
        _ => (
            26,
            if file_ext.is_empty() { "bin" } else { file_ext }.into(),
        ),
    }
}

/// Extract JPEG dimensions from SOF marker.
fn jpeg_dimensions(data: &[u8]) -> Option<(i32, i32)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        i += 2;
        if i + 2 > data.len() {
            return None;
        }
        // SOF markers (C0-CF except C4, C8, CC)
        if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
            if i + 7 > data.len() {
                return None;
            }
            let height = ((data[i + 3] as i32) << 8) | (data[i + 4] as i32);
            let width = ((data[i + 5] as i32) << 8) | (data[i + 6] as i32);
            return Some((width, height));
        }
        let len = ((data[i] as usize) << 8) | (data[i + 1] as usize);
        if len < 2 {
            return None;
        }
        i += len;
    }
    None
}

/// Extract PNG dimensions from IHDR chunk.
fn png_dimensions(data: &[u8]) -> Option<(i32, i32)> {
    if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = ((data[16] as i32) << 24)
        | ((data[17] as i32) << 16)
        | ((data[18] as i32) << 8)
        | (data[19] as i32);
    let height = ((data[20] as i32) << 24)
        | ((data[21] as i32) << 16)
        | ((data[22] as i32) << 8)
        | (data[23] as i32);
    Some((width, height))
}

/// Connect LOCO client and login, auto-refreshing token on -950.
async fn loco_connect_with_auto_refresh(
    client: &mut loco::client::LocoClient,
) -> Result<bson::Document> {
    match connect_loco_with_reauth(client).await {
        Ok(data) => Ok(data),
        Err(e) => {
            if let Some(status) = parse_loco_status_from_error(&e.to_string()) {
                print_loco_error_hint(status);
            }
            Err(e)
        }
    }
}

fn parse_loco_status_from_error(message: &str) -> Option<i64> {
    let marker = "status=";
    let idx = message.find(marker)?;
    let rest = &message[idx + marker.len()..];
    let digits: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    digits.parse::<i64>().ok()
}

fn cmd_watch(options: WatchOptions) -> Result<()> {
    if options.read_receipt || options.hook_cmd.is_some() || options.webhook_url.is_some() {
        require_permission(
            options.unattended && options.allow_side_effects,
            "watch side effects (read receipts, hooks, or webhooks)",
            "Re-run with --unattended --allow-watch-side-effects, or set both in ~/.config/openkakao/config.toml.",
        )?;
    }

    if let Some(webhook_url) = &options.webhook_url {
        validate_webhook_url(webhook_url, options.allow_insecure_webhooks)?;
    }

    let creds = get_creds()?;
    let parsed_webhook_headers = options
        .webhook_headers
        .iter()
        .map(|header| parse_webhook_header(header))
        .collect::<Result<Vec<_>>>()?;
    let hook_config = if options.hook_cmd.is_some() || options.webhook_url.is_some() {
        Some(WatchHookConfig {
            command: options.hook_cmd.clone(),
            webhook_url: options.webhook_url.clone(),
            webhook_headers: parsed_webhook_headers,
            webhook_signing_secret: options.webhook_signing_secret.clone(),
            chat_ids: options.hook_chat_ids.clone(),
            keywords: options.hook_keywords.clone(),
            message_types: options.hook_types.clone(),
            fail_fast: options.hook_fail_fast,
            min_hook_interval_secs: options.min_hook_interval_secs,
            min_webhook_interval_secs: options.min_webhook_interval_secs,
            hook_timeout_secs: options.hook_timeout_secs,
            webhook_timeout_secs: options.webhook_timeout_secs,
        })
    } else {
        None
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        let mut reconnect_count: u32 = 0;

        'reconnect: loop {
            // (Re)connect and login
            let login_data = match loco_connect_with_auto_refresh(&mut client).await {
                Ok(data) => data,
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("cooling down")
                        || err_msg.contains("-950")
                        || err_msg.contains("-999")
                    {
                        record_failure("auth_relogin_needed")?;
                        let delay = auth_cooldown_remaining_secs()?.unwrap_or(30);
                        eprintln!(
                            "[watch] Auth recovery not ready: {}. Retrying in {}s...",
                            e, delay
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                        client.disconnect();
                        continue 'reconnect;
                    }
                    record_failure("network")?;
                    if options.max_reconnect == 0 || reconnect_count >= options.max_reconnect {
                        return Err(e);
                    }
                    reconnect_count += 1;
                    let delay = std::cmp::min(2u64.pow(reconnect_count), 32);
                    eprintln!(
                        "[watch] Connect failed: {}. Reconnecting in {}s ({}/{})...",
                        e, delay, reconnect_count, options.max_reconnect
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    client.disconnect();
                    continue 'reconnect;
                }
            };

            // Build chat_id → name map from LOGINLIST chatDatas
            let mut chat_names: HashMap<i64, String> = HashMap::new();
            if let Ok(chat_datas) = login_data.get_array("chatDatas") {
                for cd in chat_datas {
                    if let Some(doc) = cd.as_document() {
                        let cid = get_bson_i64(doc, &["c", "chatId"]);
                        if cid != 0 {
                            let name = doc
                                .get_document("chatInfo")
                                .ok()
                                .and_then(|ci| ci.get_str("name").ok())
                                .map(String::from)
                                .unwrap_or_default();
                            let name = if name.is_empty() {
                                get_bson_str_array(doc, &["k"]).join(", ")
                            } else {
                                name
                            };
                            if !name.is_empty() {
                                chat_names.insert(cid, name);
                            }
                        }
                    }
                }
            }

            let chat_count = chat_names.len();
            if reconnect_count > 0 {
                eprintln!(
                    "[watch] Reconnected! ({} chats loaded)",
                    chat_count
                );
            } else {
                eprintln!(
                    "[watch] Connected! Listening for messages... ({} chats loaded)",
                    chat_count
                );
                if let Some(cid) = options.filter_chat_id {
                    eprintln!("[watch] Filtering chat_id={}", cid);
                }
                if let Some(config) = &hook_config {
                    if let Some(command) = &config.command {
                        eprintln!("[watch] Hook command enabled: {}", command);
                    }
                    if let Some(webhook_url) = &config.webhook_url {
                        eprintln!("[watch] Webhook enabled: {}", webhook_url);
                    }
                }
                eprintln!("[watch] Press Ctrl-C to stop.");
            }
            // Reset reconnect count on successful connection
            reconnect_count = 0;
            record_transport_success("watch")?;

            let mut ping_interval =
                tokio::time::interval(std::time::Duration::from_secs(60));
            // Skip the first immediate tick
            ping_interval.tick().await;

            loop {
                tokio::select! {
                    packet_result = client.recv_packet() => {
                        match packet_result {
                            Ok(packet) => {
                                let method = &packet.method;

                                // CHANGESVR: server requests reconnect
                                if method == "CHANGESVR" {
                                    eprintln!("[watch] Server requested reconnect (CHANGESVR)");
                                    client.disconnect();
                                    continue 'reconnect;
                                }

                                if options.raw {
                                    let now = chrono::Local::now().format("%H:%M:%S");
                                    println!("[{}] {} {:?}", now, method, packet.body);
                                    continue;
                                }

                                match method.as_str() {
                                    "MSG" => {
                                        let chat_id = packet.body
                                            .get_i64("chatId")
                                            .or_else(|_| packet.body.get_i32("chatId").map(|v| v as i64))
                                            .unwrap_or(0);

                                        if let Some(filter) = options.filter_chat_id {
                                            if chat_id != filter {
                                                continue;
                                            }
                                        }

                                        let chat_label = chat_names
                                            .get(&chat_id)
                                            .cloned()
                                            .unwrap_or_else(|| format!("{}", chat_id));

                                        let nick = packet.body
                                            .get_str("authorNickname")
                                            .map(String::from)
                                            .unwrap_or_else(|_| {
                                                packet.body
                                                    .get_document("author")
                                                    .ok()
                                                    .and_then(|a| a.get_str("nickName").ok())
                                                    .map(String::from)
                                                    .unwrap_or_else(|| "???".to_string())
                                            });

                                        let msg_type = packet.body
                                            .get_i32("type")
                                            .unwrap_or(0);

                                        let content = render_message_content(&packet.body, msg_type);
                                        let log_id = packet.body
                                            .get_i64("logId")
                                            .or_else(|_| packet.body.get_i32("logId").map(|v| v as i64))
                                            .unwrap_or(0);
                                        let author_id = packet.body
                                            .get_i64("authorId")
                                            .or_else(|_| packet.body.get_i32("authorId").map(|v| v as i64))
                                            .unwrap_or(0);
                                        let attachment = packet.body
                                            .get_str("attachment")
                                            .unwrap_or("")
                                            .to_string();
                                        let event = WatchMessageEvent {
                                            event_type: "message",
                                            received_at: chrono::Utc::now().to_rfc3339(),
                                            method: method.clone(),
                                            chat_id,
                                            chat_name: chat_label.clone(),
                                            log_id,
                                            author_id,
                                            author_nickname: nick.clone(),
                                            message_type: msg_type,
                                            message: content.clone(),
                                            attachment: attachment.clone(),
                                        };

                                        let now = chrono::Local::now().format("%H:%M:%S");
                                        if color_enabled() {
                                            println!(
                                                "{} {} {}: {}",
                                                format!("[{}]", now).dimmed(),
                                                format!("[{}]", chat_label).cyan(),
                                                nick.bold(),
                                                content
                                            );
                                        } else {
                                            println!(
                                                "[{}] [{}] {}: {}",
                                                now, chat_label, nick, content
                                            );
                                        }

                                        if let Some(config) = &hook_config {
                                            if watch_hook_matches(config, &event) {
                                                if config.command.is_some() {
                                                    match tokio::task::spawn_blocking({
                                                        let config = config.clone();
                                                        let event = event.clone();
                                                        move || run_watch_command_hook(&config, &event)
                                                    })
                                                    .await
                                                    {
                                                        Ok(Ok(())) => {}
                                                        Ok(Err(e)) => {
                                                            eprintln!("[watch] Hook failed: {}", e);
                                                            if config.fail_fast {
                                                                return Err(e);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            let err = anyhow::anyhow!("hook task join error: {}", e);
                                                            eprintln!("[watch] Hook failed: {}", err);
                                                            if config.fail_fast {
                                                                return Err(err);
                                                            }
                                                        }
                                                    }
                                                }
                                                if config.webhook_url.is_some() {
                                                    match tokio::task::spawn_blocking({
                                                        let config = config.clone();
                                                        let event = event.clone();
                                                        move || run_watch_webhook(&config, &event)
                                                    })
                                                    .await
                                                    {
                                                        Ok(Ok(())) => {}
                                                        Ok(Err(e)) => {
                                                            eprintln!("[watch] Webhook failed: {}", e);
                                                            if config.fail_fast {
                                                                return Err(e);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            let err = anyhow::anyhow!("webhook task join error: {}", e);
                                                            eprintln!("[watch] Webhook failed: {}", err);
                                                            if config.fail_fast {
                                                                return Err(err);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // Send read receipt if enabled
                                        if options.read_receipt && log_id > 0 {
                                            let _ = client.send_packet("NOTIREAD", bson::doc! {
                                                "chatId": chat_id,
                                                "watermark": log_id,
                                            }).await;
                                        }

                                        // Auto-download media if enabled
                                        if options.download_media
                                            && matches!(msg_type, 2 | 3 | 12 | 14 | 26 | 27)
                                            && !attachment.is_empty()
                                        {
                                                let dl_creds = client.credentials.clone();
                                                let dl_dir = options.download_dir.clone();
                                                tokio::task::spawn_blocking(move || {
                                                    if let Some((url, filename)) = parse_attachment_url(&attachment, msg_type) {
                                                        let dir = Path::new(&dl_dir).join(chat_id.to_string());
                                                        let save_name = format!("{}_{}", log_id, sanitize_filename(&filename));
                                                        let save_path = dir.join(&save_name);
                                                        match download_media_file(&dl_creds, &url, &save_path) {
                                                            Ok(bytes) => {
                                                                eprintln!(
                                                                    "[watch] Downloaded {} ({} bytes)",
                                                                    save_path.display(), bytes
                                                                );
                                                            }
                                                            Err(e) => {
                                                                eprintln!(
                                                                    "[watch] Download failed for {}: {}",
                                                                    save_name, e
                                                                );
                                                            }
                                                        }
                                                    }
                                                });
                                        }
                                    }
                                    "DECUNREAD" | "NOTIREAD" | "SYNCLINKCR" | "SYNCLINKUP"
                                    | "SYNCMSG" | "SYNCDLMSG" => {
                                        // Known push events, silently ignore
                                    }
                                    _ => {
                                        eprintln!("[watch] Push: {} (status={})", method, packet.status());
                                    }
                                }
                            }
                            Err(e) => {
                                let err_msg = e.to_string();
                                if err_msg.contains("-950") || err_msg.contains("-999") {
                                    record_failure("auth_relogin_needed")?;
                                    let delay = auth_cooldown_remaining_secs()?.unwrap_or(30);
                                    eprintln!(
                                        "[watch] Auth error: {}. Retrying in {}s...",
                                        e, delay
                                    );
                                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                                    client.disconnect();
                                    continue 'reconnect;
                                }
                                record_failure("network")?;
                                if options.max_reconnect == 0 {
                                    eprintln!("[watch] Connection lost: {}", e);
                                    return Err(e);
                                }
                                reconnect_count += 1;
                                if reconnect_count > options.max_reconnect {
                                    eprintln!(
                                        "[watch] Connection lost after {} reconnect attempts: {}",
                                        options.max_reconnect, e
                                    );
                                    return Err(e);
                                }
                                let delay = std::cmp::min(2u64.pow(reconnect_count), 32);
                                eprintln!(
                                    "[watch] Connection lost: {}. Reconnecting in {}s ({}/{})...",
                                    e, delay, reconnect_count, options.max_reconnect
                                );
                                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                                client.disconnect();
                                continue 'reconnect;
                            }
                        }
                    }
                    _ = ping_interval.tick() => {
                        if let Err(e) = client.send_packet("PING", bson::doc! {}).await {
                            record_failure("network")?;
                            eprintln!("[watch] PING failed: {}", e);
                            if options.max_reconnect == 0 {
                                return Err(anyhow::anyhow!("PING failed: {}", e));
                            }
                            reconnect_count += 1;
                            if reconnect_count > options.max_reconnect {
                                return Err(anyhow::anyhow!(
                                    "PING failed after {} reconnects: {}", options.max_reconnect, e
                                ));
                            }
                            let delay = std::cmp::min(2u64.pow(reconnect_count), 32);
                            eprintln!(
                                "[watch] Reconnecting in {}s ({}/{})...",
                                delay, reconnect_count, options.max_reconnect
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                            client.disconnect();
                            continue 'reconnect;
                        }
                    }
                    _ = tokio::signal::ctrl_c() => {
                        eprintln!("\n[watch] Shutting down...");
                        return Ok(());
                    }
                }
            }
        }
    })
}

async fn fetch_loco_chat_listings_with_client(
    client: &mut loco::client::LocoClient,
    login_data: &bson::Document,
    show_all: bool,
) -> Result<Vec<ChatListing>> {
    let response = client
        .send_command(
            "LCHATLIST",
            bson::doc! {
                "chatIds": bson::Bson::Array(vec![]),
                "maxIds": bson::Bson::Array(vec![]),
                "lastTokenId": 0_i64,
                "lastChatId": 0_i64,
            },
        )
        .await?;

    let lchat_status = response.status();
    eprintln!("[loco-chats] LCHATLIST status={}", lchat_status);

    let chat_datas = if lchat_status == 0 {
        response.body.get_array("chatDatas").ok()
    } else {
        None
    };
    let chat_datas = chat_datas.or_else(|| login_data.get_array("chatDatas").ok());
    let Some(chat_datas) = chat_datas else {
        return Ok(Vec::new());
    };

    let mut chats = Vec::new();
    for cd in chat_datas {
        if let Some(doc) = cd.as_document() {
            let chat_id = get_bson_i64(doc, &["c", "chatId"]);
            let kind = get_bson_str(doc, &["t", "type"]);
            let last_log_id = get_bson_i64(doc, &["s", "lastLogId"]);
            let last_seen = get_bson_i64(doc, &["ll", "lastSeenLogId"]);
            let has_unread = last_log_id > last_seen;
            let active_member_count = get_bson_i32(doc, &["a", "activeMembersCount"]);

            let title = doc
                .get_document("chatInfo")
                .ok()
                .and_then(|ci| ci.get_str("name").ok())
                .map(String::from)
                .unwrap_or_default();
            let title = if title.is_empty() {
                get_bson_str_array(doc, &["k"]).join(", ")
            } else {
                title
            };

            if !show_all && !has_unread && title.is_empty() {
                continue;
            }

            chats.push(ChatListing {
                chat_id,
                kind,
                title,
                has_unread,
                unread_count: None,
                active_members: Some(active_member_count),
                last_log_id: Some(last_log_id),
                last_seen_log_id: Some(last_seen),
            });
        }
    }

    Ok(chats)
}

fn cmd_loco_chats(
    show_all: bool,
    unread: bool,
    search: Option<String>,
    chat_type: Option<String>,
    json: bool,
) -> Result<()> {
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = loco_connect_with_auto_refresh(&mut client).await?;
        let mut chats =
            fetch_loco_chat_listings_with_client(&mut client, &login_data, show_all).await?;

        if unread {
            chats.retain(|chat| chat.has_unread);
        }

        if let Some(ref query) = search {
            let q = query.to_lowercase();
            chats.retain(|chat| chat.title.to_lowercase().contains(&q));
        }

        if let Some(ref t) = chat_type {
            let lowered = t.to_lowercase();
            let expected = match lowered.as_str() {
                "dm" => "DirectChat".to_string(),
                "group" => "MultiChat".to_string(),
                "memo" => "MemoChat".to_string(),
                "open" => "OpenMultiChat".to_string(),
                "opendm" => "OpenDirectChat".to_string(),
                other => other.to_string(),
            };
            chats.retain(|chat| chat.kind == expected);
        }

        if json {
            println!("{}", serde_json::to_string_pretty(&chats)?);
            return Ok(());
        }

        let rows = chats
            .iter()
            .map(|chat| {
                vec![
                    type_label(&chat.kind).to_string(),
                    chat.title.clone(),
                    if chat.has_unread {
                        "*".to_string()
                    } else {
                        String::new()
                    },
                    chat.chat_id.to_string(),
                ]
            })
            .collect::<Vec<_>>();

        print_section_title(&format!("Chats ({})", rows.len()));
        print_table(&["Type", "Name", "Unread", "Chat ID"], rows);

        Ok(())
    })
}

#[allow(clippy::too_many_arguments)]
fn cmd_loco_read(
    chat_id: i64,
    count: i32,
    cursor: Option<i64>,
    since: Option<&str>,
    fetch_all: bool,
    delay_ms: u64,
    force: bool,
    json: bool,
) -> Result<()> {
    let since_ts = parse_since_date(since)?;
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

        // Get lastLogId for this chat via CHATONROOM
        let room_info = client
            .send_command(
                "CHATONROOM",
                bson::doc! {
                    "chatId": chat_id,
                },
            )
            .await?;
        if room_info.status() != 0 {
            anyhow::bail!("CHATONROOM failed (status={})", room_info.status());
        }

        // Open chat safety check
        let chat_type = extract_chat_type(&room_info.body);
        if is_open_chat(&chat_type) {
            if fetch_all && !force {
                eprintln!(
                    "Blocked: --all on open chat ({}) has higher ban risk.",
                    type_label(&chat_type)
                );
                eprintln!("Use --force to override this safety check.");
                anyhow::bail!("Open chat full-history blocked (use --force)");
            }
            if is_open_chat(&chat_type) {
                eprintln!(
                    "Warning: reading from {} (open chat). Using conservative rate limiting.",
                    type_label(&chat_type)
                );
            }
        }

        // Enforce minimum 500ms delay for open chats to reduce ban risk
        let effective_delay = if is_open_chat(&chat_type) && delay_ms < 500 {
            eprintln!(
                "Note: delay raised to 500ms for open chat safety (was {}ms)",
                delay_ms
            );
            500
        } else {
            delay_ms
        };

        let last_log_id = room_info.body.get_i64("l").unwrap_or(0);
        if last_log_id == 0 {
            anyhow::bail!("No messages in this chat");
        }

        // Build member name map from CHATONROOM members
        let mut member_names: HashMap<i64, String> = HashMap::new();
        if let Ok(members) = room_info.body.get_array("m") {
            for m in members {
                if let Some(doc) = m.as_document() {
                    let uid = get_bson_i64(doc, &["userId"]);
                    let nick = get_bson_str(doc, &["nickName", "nickname"]);
                    if uid > 0 && !nick.is_empty() {
                        member_names.insert(uid, nick);
                    }
                }
            }
        }

        // SYNCMSG scans forward: cur -> max. Start from cursor or 0.
        let mut all_messages: Vec<serde_json::Value> = Vec::new();
        let mut cur = cursor.unwrap_or(0);
        let max_log = last_log_id;
        let mut batch_num = 0u32;

        if fetch_all {
            eprintln!(
                "[loco-read] Fetching full history (lastLogId={}, delay={}ms)...",
                last_log_id, effective_delay
            );
        }

        loop {
            let response = match client
                .send_command(
                    "SYNCMSG",
                    bson::doc! {
                        "chatId": chat_id,
                        "cur": cur,
                        "cnt": 50_i32,
                        "max": max_log,
                    },
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    if all_messages.is_empty() {
                        return Err(e.context("SYNCMSG failed"));
                    }
                    // On disconnect, print resume cursor so user can continue
                    eprintln!("[loco-read] Connection lost: {}", e);
                    eprintln!(
                        "[loco-read] Resume with: openkakao-rs read {} --all --cursor {}",
                        chat_id, cur
                    );
                    break;
                }
            };

            if response.status() != 0 {
                if all_messages.is_empty() {
                    anyhow::bail!("SYNCMSG failed (status={})", response.status());
                }
                eprintln!(
                    "[loco-read] SYNCMSG returned status={}. Resume with: openkakao-rs read {} --all --cursor {}",
                    response.status(), chat_id, cur
                );
                break;
            }

            let is_ok = response.body.get_bool("isOK").unwrap_or(true);
            let chat_logs = response
                .body
                .get_array("chatLogs")
                .map(|a| a.to_vec())
                .unwrap_or_default();

            if chat_logs.is_empty() {
                break;
            }

            let batch_count = chat_logs.len();
            let mut max_log_in_batch = 0_i64;

            for log in &chat_logs {
                if let Some(doc) = log.as_document() {
                    let log_id = get_bson_i64(doc, &["logId"]);
                    if log_id > max_log_in_batch {
                        max_log_in_batch = log_id;
                    }

                    let author_id = get_bson_i64(doc, &["authorId"]);
                    let msg_type = get_bson_i32(doc, &["type"]);
                    let message = get_bson_str(doc, &["message"]);
                    let send_at = get_bson_i64(doc, &["sendAt"]);
                    let author_nick = get_bson_str(doc, &["authorNickname"]);
                    let attachment = get_bson_str(doc, &["attachment"]);

                    // Apply --since filter at fetch time
                    if let Some(ts) = since_ts {
                        if send_at < ts {
                            continue;
                        }
                    }

                    all_messages.push(serde_json::json!({
                        "log_id": log_id,
                        "author_id": author_id,
                        "author_nickname": author_nick,
                        "message_type": msg_type,
                        "message": message,
                        "attachment": attachment,
                        "send_at": send_at,
                    }));
                }
            }

            batch_num += 1;
            eprintln!(
                "[loco-read] Batch {}: {} msgs (total: {}, cursor: {})",
                batch_num,
                batch_count,
                all_messages.len(),
                max_log_in_batch
            );

            if is_ok || max_log_in_batch == 0 {
                break;
            }
            cur = max_log_in_batch;

            // Rate limit between batches
            if effective_delay > 0 && !is_ok {
                tokio::time::sleep(std::time::Duration::from_millis(effective_delay)).await;
            }
        }

        // When not fetching all, only keep the last `count` messages
        if !fetch_all && all_messages.len() > count as usize {
            let skip = all_messages.len() - count as usize;
            all_messages = all_messages.split_off(skip);
        }

        // Sort by send_at ascending
        all_messages.sort_by_key(|m| m.get("send_at").and_then(|v| v.as_i64()).unwrap_or(0));

        if json {
            println!("{}", serde_json::to_string_pretty(&all_messages)?);
        } else {
            for msg in &all_messages {
                let send_at = msg.get("send_at").and_then(|v| v.as_i64()).unwrap_or(0);
                let time_str = format_time(send_at);
                let nick = msg
                    .get("author_nickname")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let author_id = msg.get("author_id").and_then(|v| v.as_i64()).unwrap_or(0);
                let msg_type = msg
                    .get("message_type")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let message = msg.get("message").and_then(|v| v.as_str()).unwrap_or("");

                let display_nick = if !nick.is_empty() {
                    nick.to_string()
                } else if let Some(name) = member_names.get(&author_id) {
                    name.clone()
                } else {
                    format!("{}", author_id)
                };

                let content = match msg_type {
                    1 => message.to_string(),
                    2 => "[사진]".to_string(),
                    3 => "[동영상]".to_string(),
                    5 => "[연락처]".to_string(),
                    12 => "[음성메시지]".to_string(),
                    14 => "[이모티콘]".to_string(),
                    26 => "[파일]".to_string(),
                    27 => "[멀티사진]".to_string(),
                    71 | 72 => "[투표]".to_string(),
                    _ => {
                        if message.is_empty() {
                            format!("[type={}]", msg_type)
                        } else {
                            message.to_string()
                        }
                    }
                };

                if color_enabled() {
                    println!("{} {}: {}", time_str.dimmed(), display_nick.bold(), content);
                } else {
                    println!("{} {}: {}", time_str, display_nick, content);
                }
            }
            // Print resume hint with last cursor
            let last_cursor = all_messages
                .last()
                .and_then(|m| m.get("log_id").and_then(|v| v.as_i64()))
                .unwrap_or(0);
            eprintln!(
                "({} messages, last_cursor={})",
                all_messages.len(),
                last_cursor
            );
        }

        Ok(())
    })
}

async fn fetch_loco_member_profiles_with_client(
    client: &mut loco::client::LocoClient,
    chat_id: i64,
) -> Result<LocoGetMemSnapshot> {
    let response = client
        .send_command("GETMEM", bson::doc! { "chatId": chat_id })
        .await?;

    if response.status() != 0 {
        anyhow::bail!("GETMEM failed (status={})", response.status());
    }

    let members = response
        .body
        .get_array("members")
        .map(|a| a.to_vec())
        .unwrap_or_default();
    let token = response.body.get_i64("token").ok();

    Ok(LocoGetMemSnapshot {
        token,
        members: members
            .iter()
            .filter_map(|member| member.as_document().map(LocoMemberProfile::from_getmem_doc))
            .collect::<Vec<_>>(),
    })
}

async fn fetch_loco_member_profiles_only_with_client(
    client: &mut loco::client::LocoClient,
    chat_id: i64,
) -> Result<Vec<LocoMemberProfile>> {
    Ok(fetch_loco_member_profiles_with_client(client, chat_id)
        .await?
        .members)
}

fn merge_unique_string(values: &mut Vec<String>, candidate: &str) {
    if candidate.is_empty() || values.iter().any(|value| value == candidate) {
        return;
    }
    values.push(candidate.to_string());
}

fn merge_unique_i64(values: &mut Vec<i64>, candidate: i64) {
    if candidate <= 0 || values.contains(&candidate) {
        return;
    }
    values.push(candidate);
}

fn merge_preferred_string(current: &mut String, candidate: &str) {
    if current.is_empty() && !candidate.is_empty() {
        *current = candidate.to_string();
    }
}

async fn build_local_friend_graph_with_client(
    client: &mut loco::client::LocoClient,
    login_data: &bson::Document,
    self_user_id: i64,
) -> Result<LocalFriendGraphSnapshot> {
    let chats = fetch_loco_chat_listings_with_client(client, login_data, true).await?;
    let mut graph = std::collections::BTreeMap::<i64, LocalFriendGraphEntry>::new();
    let mut failed_chat_ids = Vec::new();
    let mut chat_meta = Vec::new();

    for chat in &chats {
        match fetch_loco_member_profiles_with_client(client, chat.chat_id).await {
            Ok(getmem) => {
                chat_meta.push(LocalFriendGraphChatMeta {
                    chat_id: chat.chat_id,
                    title: chat.title.clone(),
                    getmem_token: getmem.token,
                    member_count: getmem.members.len(),
                });

                for member in getmem.members {
                    let entry =
                        graph
                            .entry(member.user_id)
                            .or_insert_with(|| LocalFriendGraphEntry {
                                user_id: member.user_id,
                                account_id: member.account_id,
                                nickname: member.nickname.clone(),
                                country_iso: member.country_iso.clone(),
                                status_message: member.status_message.clone(),
                                profile_image_url: member.profile_image_url.clone(),
                                full_profile_image_url: member.full_profile_image_url.clone(),
                                original_profile_image_url: member
                                    .original_profile_image_url
                                    .clone(),
                                access_permits: Vec::new(),
                                suspicion: member.suspicion.clone(),
                                suspended: member.suspended,
                                memorial: member.memorial,
                                member_type: member.member_type,
                                chat_ids: Vec::new(),
                                chat_titles: Vec::new(),
                                is_self: member.user_id == self_user_id,
                                hidden_like: false,
                                hidden_block_type: None,
                            });

                    if entry.account_id == 0 && member.account_id != 0 {
                        entry.account_id = member.account_id;
                    }
                    merge_preferred_string(&mut entry.nickname, &member.nickname);
                    merge_preferred_string(&mut entry.country_iso, &member.country_iso);
                    merge_preferred_string(&mut entry.status_message, &member.status_message);
                    merge_preferred_string(&mut entry.profile_image_url, &member.profile_image_url);
                    merge_preferred_string(
                        &mut entry.full_profile_image_url,
                        &member.full_profile_image_url,
                    );
                    merge_preferred_string(
                        &mut entry.original_profile_image_url,
                        &member.original_profile_image_url,
                    );
                    merge_preferred_string(&mut entry.suspicion, &member.suspicion);
                    if member.suspended {
                        entry.suspended = true;
                    }
                    if member.memorial {
                        entry.memorial = true;
                    }
                    if entry.member_type == 0 && member.member_type != 0 {
                        entry.member_type = member.member_type;
                    }
                    merge_unique_i64(&mut entry.chat_ids, chat.chat_id);
                    merge_unique_string(&mut entry.chat_titles, &chat.title);
                    merge_unique_string(&mut entry.access_permits, &member.access_permit);
                }
            }
            Err(err) => {
                eprintln!("[friends/local] GETMEM {} failed: {}", chat.chat_id, err);
                failed_chat_ids.push(chat.chat_id);
            }
        }
    }

    let entries = graph.into_values().collect::<Vec<_>>();
    Ok(LocalFriendGraphSnapshot {
        user_count: entries.len(),
        chat_count: chats.len(),
        failed_chat_ids,
        chat_meta,
        entries,
    })
}

fn merge_blocked_members_into_local_graph(
    snapshot: &mut LocalFriendGraphSnapshot,
    blocked: LocoBlockedSnapshot,
) {
    let mut graph = snapshot
        .entries
        .drain(..)
        .map(|entry| (entry.user_id, entry))
        .collect::<std::collections::BTreeMap<_, _>>();

    for member in blocked.members {
        let entry = graph
            .entry(member.user_id)
            .or_insert_with(|| LocalFriendGraphEntry {
                user_id: member.user_id,
                account_id: 0,
                nickname: member.nickname.clone(),
                country_iso: String::new(),
                status_message: String::new(),
                profile_image_url: member.profile_image_url.clone(),
                full_profile_image_url: member.full_profile_image_url.clone(),
                original_profile_image_url: String::new(),
                access_permits: Vec::new(),
                suspicion: member.suspicion.clone(),
                suspended: member.suspended,
                memorial: false,
                member_type: -1,
                chat_ids: Vec::new(),
                chat_titles: Vec::new(),
                is_self: false,
                hidden_like: true,
                hidden_block_type: Some(member.block_type),
            });

        merge_preferred_string(&mut entry.nickname, &member.nickname);
        merge_preferred_string(&mut entry.profile_image_url, &member.profile_image_url);
        merge_preferred_string(
            &mut entry.full_profile_image_url,
            &member.full_profile_image_url,
        );
        merge_preferred_string(&mut entry.suspicion, &member.suspicion);
        if member.suspended {
            entry.suspended = true;
        }
        entry.hidden_like = true;
        entry.hidden_block_type = Some(member.block_type);
    }

    snapshot.user_count = graph.len();
    snapshot.entries = graph.into_values().collect();
}

fn build_local_friend_graph() -> Result<LocalFriendGraphSnapshot> {
    let creds = get_creds()?;
    let self_user_id = creds.user_id;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = loco_connect_with_auto_refresh(&mut client).await?;
        build_local_friend_graph_with_client(&mut client, &login_data, self_user_id).await
    })
}

fn local_graph_hint_summary(
    snapshot: &LocalFriendGraphSnapshot,
    cached_requests: &[ProfileCacheHint],
) -> LocalFriendGraphHintSummary {
    let by_user_id = snapshot
        .entries
        .iter()
        .map(|entry| (entry.user_id, entry))
        .collect::<HashMap<_, _>>();

    let candidate_matches = cached_requests
        .iter()
        .filter(|hint| !hint.user_ids.is_empty())
        .map(|hint| {
            let matched = hint
                .user_ids
                .iter()
                .filter_map(|user_id| by_user_id.get(user_id).copied())
                .collect::<Vec<_>>();

            let mut candidate_chat_ids = Vec::new();
            let mut candidate_access_permits = Vec::new();
            for entry in &matched {
                for chat_id in &entry.chat_ids {
                    merge_unique_i64(&mut candidate_chat_ids, *chat_id);
                }
                for permit in &entry.access_permits {
                    merge_unique_string(&mut candidate_access_permits, permit);
                }
            }

            LocalFriendGraphHintMatch {
                entry_id: hint.entry_id,
                kind: hint.kind.clone(),
                requested_user_ids: hint.user_ids.clone(),
                matched_user_ids: matched.iter().map(|entry| entry.user_id).collect(),
                candidate_chat_ids,
                candidate_access_permits,
            }
        })
        .collect::<Vec<_>>();

    LocalFriendGraphHintSummary {
        user_count: snapshot.user_count,
        chat_count: snapshot.chat_count,
        failed_chat_ids: snapshot.failed_chat_ids.clone(),
        chat_meta: snapshot.chat_meta.clone(),
        candidate_matches,
    }
}

fn push_unique_candidate_body(
    bodies: &mut Vec<serde_json::Value>,
    seen: &mut HashSet<String>,
    body: serde_json::Value,
) {
    if let Ok(key) = serde_json::to_string(&body) {
        if seen.insert(key) {
            bodies.push(body);
        }
    }
}

fn build_syncmainpf_candidate(
    snapshot: &LocalFriendGraphSnapshot,
    cached_requests: &[ProfileCacheHint],
    user_id: i64,
) -> Option<SyncMainPfCandidate> {
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.user_id == user_id)?;

    let mut source_entry_ids = cached_requests
        .iter()
        .filter(|hint| hint.user_ids.contains(&user_id))
        .map(|hint| hint.entry_id)
        .collect::<Vec<_>>();
    source_entry_ids.sort_unstable();
    source_entry_ids.dedup();

    let pfids = [entry.user_id, entry.account_id]
        .into_iter()
        .filter(|value| *value > 0)
        .collect::<Vec<_>>();
    let string_pfids = {
        let mut values = Vec::new();
        for candidate in [
            Some(entry.user_id.to_string()),
            (entry.account_id > 0).then(|| entry.account_id.to_string()),
        ]
        .into_iter()
        .flatten()
        {
            if !values.contains(&candidate) {
                values.push(candidate);
            }
        }
        values
    };
    let chat_ids = if entry.chat_ids.is_empty() {
        vec![None]
    } else {
        entry.chat_ids.iter().copied().map(Some).collect::<Vec<_>>()
    };
    let access_permits = if entry.access_permits.is_empty() {
        vec![None]
    } else {
        entry
            .access_permits
            .iter()
            .cloned()
            .map(Some)
            .collect::<Vec<_>>()
    };

    let mut bodies = Vec::new();
    let mut uplinkprof_bodies = Vec::new();
    let mut seen = HashSet::new();
    let mut uplink_seen = HashSet::new();

    if entry.is_self {
        for pfid in &pfids {
            push_unique_candidate_body(
                &mut bodies,
                &mut seen,
                serde_json::json!({
                    "ct": "me",
                    "pfid": pfid,
                }),
            );
        }
        for pfid in &string_pfids {
            push_unique_candidate_body(
                &mut bodies,
                &mut seen,
                serde_json::json!({
                    "ct": "me",
                    "pfid": pfid,
                }),
            );
        }
    }

    for pfid in &pfids {
        for chat_id in &chat_ids {
            for access_permit in &access_permits {
                for ct in ["d", "p"] {
                    let mut body = serde_json::Map::new();
                    body.insert("ct".into(), serde_json::json!(ct));
                    body.insert("pfid".into(), serde_json::json!(pfid));
                    if let Some(chat_id) = chat_id {
                        body.insert("chatId".into(), serde_json::json!(chat_id));
                    }
                    if let Some(access_permit) = access_permit {
                        body.insert("accessPermit".into(), serde_json::json!(access_permit));
                    }
                    push_unique_candidate_body(
                        &mut bodies,
                        &mut seen,
                        serde_json::Value::Object(body),
                    );
                }
            }
        }
    }

    for pfid in &string_pfids {
        for chat_id in &chat_ids {
            for access_permit in &access_permits {
                for ct in ["d", "p"] {
                    let mut body = serde_json::Map::new();
                    body.insert("ct".into(), serde_json::json!(ct));
                    body.insert("pfid".into(), serde_json::json!(pfid));
                    if let Some(chat_id) = chat_id {
                        body.insert("chatId".into(), serde_json::json!(chat_id));
                    }
                    if let Some(access_permit) = access_permit {
                        body.insert("accessPermit".into(), serde_json::json!(access_permit));
                    }
                    push_unique_candidate_body(
                        &mut bodies,
                        &mut seen,
                        serde_json::Value::Object(body),
                    );
                }
            }
        }
    }

    for pfid in &pfids {
        push_unique_candidate_body(
            &mut uplinkprof_bodies,
            &mut uplink_seen,
            serde_json::json!({ "pfid": pfid }),
        );
        for relation in ["n", "r"] {
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "pfid": pfid, "r": relation }),
            );
        }
        for access_permit in access_permits.iter().flatten() {
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "pfid": pfid, "F": access_permit }),
            );
            for relation in ["n", "r"] {
                push_unique_candidate_body(
                    &mut uplinkprof_bodies,
                    &mut uplink_seen,
                    serde_json::json!({ "pfid": pfid, "F": access_permit, "r": relation }),
                );
            }
        }

        for profile_type in 0..=4 {
            for key in ["t", "profileType"] {
                push_unique_candidate_body(
                    &mut uplinkprof_bodies,
                    &mut uplink_seen,
                    serde_json::json!({ "pfid": pfid, key: profile_type }),
                );
                push_unique_candidate_body(
                    &mut uplinkprof_bodies,
                    &mut uplink_seen,
                    serde_json::json!({ "pfid": pfid, key: profile_type, "mp": "y" }),
                );
                for relation in ["n", "r"] {
                    push_unique_candidate_body(
                        &mut uplinkprof_bodies,
                        &mut uplink_seen,
                        serde_json::json!({ "pfid": pfid, key: profile_type, "r": relation }),
                    );
                    push_unique_candidate_body(
                        &mut uplinkprof_bodies,
                        &mut uplink_seen,
                        serde_json::json!({ "pfid": pfid, key: profile_type, "r": relation, "mp": "y" }),
                    );
                }
                for access_permit in access_permits.iter().flatten() {
                    push_unique_candidate_body(
                        &mut uplinkprof_bodies,
                        &mut uplink_seen,
                        serde_json::json!({ "pfid": pfid, "F": access_permit, key: profile_type }),
                    );
                    push_unique_candidate_body(
                        &mut uplinkprof_bodies,
                        &mut uplink_seen,
                        serde_json::json!({ "pfid": pfid, "F": access_permit, key: profile_type, "mp": "y" }),
                    );
                    for relation in ["n", "r"] {
                        push_unique_candidate_body(
                            &mut uplinkprof_bodies,
                            &mut uplink_seen,
                            serde_json::json!({ "pfid": pfid, "F": access_permit, key: profile_type, "r": relation }),
                        );
                        push_unique_candidate_body(
                            &mut uplinkprof_bodies,
                            &mut uplink_seen,
                            serde_json::json!({ "pfid": pfid, "F": access_permit, key: profile_type, "r": relation, "mp": "y" }),
                        );
                    }
                }
            }
        }
    }

    for pfid in &string_pfids {
        push_unique_candidate_body(
            &mut uplinkprof_bodies,
            &mut uplink_seen,
            serde_json::json!({ "pfid": pfid }),
        );
        for access_permit in access_permits.iter().flatten() {
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "pfid": pfid, "F": access_permit }),
            );
        }
    }

    Some(SyncMainPfCandidate {
        user_id: entry.user_id,
        account_id: entry.account_id,
        is_self: entry.is_self,
        source_entry_ids,
        bodies,
        uplinkprof_bodies,
    })
}

fn build_syncmainpf_probe_variants(candidate: &SyncMainPfCandidate) -> Vec<serde_json::Value> {
    let mut variants = Vec::new();
    let mut seen = HashSet::new();

    for body in &candidate.bodies {
        push_unique_candidate_body(&mut variants, &mut seen, body.clone());

        for profile_type in 0..=4 {
            let with_profile_type = match body {
                serde_json::Value::Object(map) => {
                    let mut body = map.clone();
                    body.insert("profileType".into(), serde_json::json!(profile_type));
                    serde_json::Value::Object(body)
                }
                _ => continue,
            };
            push_unique_candidate_body(&mut variants, &mut seen, with_profile_type.clone());
            let with_t = match &with_profile_type {
                serde_json::Value::Object(map) => {
                    let mut body = map.clone();
                    body.insert("t".into(), serde_json::json!(profile_type));
                    serde_json::Value::Object(body)
                }
                _ => continue,
            };
            push_unique_candidate_body(&mut variants, &mut seen, with_t.clone());
            let with_mp = match &with_t {
                serde_json::Value::Object(map) => {
                    let mut body = map.clone();
                    body.insert("mp".into(), serde_json::json!("y"));
                    serde_json::Value::Object(body)
                }
                _ => continue,
            };
            push_unique_candidate_body(&mut variants, &mut seen, with_mp.clone());

            for relation in ["n", "r"] {
                let with_relation = match &with_profile_type {
                    serde_json::Value::Object(map) => {
                        let mut body = map.clone();
                        body.insert("r".into(), serde_json::json!(relation));
                        serde_json::Value::Object(body)
                    }
                    _ => continue,
                };
                push_unique_candidate_body(&mut variants, &mut seen, with_relation);
                let with_t_relation = match &with_t {
                    serde_json::Value::Object(map) => {
                        let mut body = map.clone();
                        body.insert("r".into(), serde_json::json!(relation));
                        serde_json::Value::Object(body)
                    }
                    _ => continue,
                };
                push_unique_candidate_body(&mut variants, &mut seen, with_t_relation);
                let with_mp_relation = match &with_mp {
                    serde_json::Value::Object(map) => {
                        let mut body = map.clone();
                        body.insert("r".into(), serde_json::json!(relation));
                        serde_json::Value::Object(body)
                    }
                    _ => continue,
                };
                push_unique_candidate_body(&mut variants, &mut seen, with_mp_relation);
            }
        }
    }

    variants
}

async fn probe_syncmainpf_variants(
    variants: &[serde_json::Value],
) -> Result<Vec<SyncMainPfProbeResult>> {
    let raw = probe_method_variants("SYNCMAINPF", variants).await?;
    Ok(raw
        .into_iter()
        .map(|result| SyncMainPfProbeResult {
            body: result.body,
            packet_status_code: result.packet_status_code,
            body_status: result.body_status,
            push_count: result.push_count,
            push_methods: result.push_methods,
        })
        .collect())
}

fn build_uplinkprof_probe_variants(candidate: &SyncMainPfCandidate) -> Vec<serde_json::Value> {
    candidate.uplinkprof_bodies.clone()
}

async fn probe_method_variants(
    method: &str,
    variants: &[serde_json::Value],
) -> Result<Vec<MethodProbeResult>> {
    let creds = get_creds()?;
    let mut client = loco::client::LocoClient::new(creds);
    reconnect_loco_probe_client(&mut client).await?;

    let mut results = Vec::new();
    for variant in variants {
        let object = variant
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("{method} probe body must be a JSON object"))?;
        let body = bson::to_document(object)?;
        let result = match client
            .send_command_collect(method, body.clone(), Duration::from_secs(2))
            .await
        {
            Ok(result) => result,
            Err(error) if should_retry_loco_probe_error(&error) => {
                reconnect_loco_probe_client(&mut client).await?;
                client
                    .send_command_collect(method, body, Duration::from_secs(2))
                    .await?
            }
            Err(error) => return Err(error),
        };
        let packet_status_code = result
            .response
            .as_ref()
            .map(|p| p.status_code)
            .unwrap_or(-1);
        let body_status = result.response.as_ref().and_then(|packet| {
            packet
                .body
                .get_i32("status")
                .ok()
                .or_else(|| packet.body.get_i64("status").ok().map(|value| value as i32))
        });
        let push_methods = result
            .pushes
            .iter()
            .map(|packet| packet.method.clone())
            .collect::<Vec<_>>();
        results.push(MethodProbeResult {
            method: method.to_string(),
            body: variant.clone(),
            packet_status_code,
            body_status,
            push_count: result.pushes.len(),
            push_methods,
        });
    }

    Ok(results)
}

fn should_retry_loco_probe_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("early eof")
        || message.contains("connection reset by peer")
        || message.contains("broken pipe")
        || message.contains("os error 54")
}

async fn reconnect_loco_probe_client(client: &mut loco::client::LocoClient) -> Result<()> {
    let mut last_error = None;
    for _ in 0..3 {
        match loco_connect_with_auto_refresh(client).await {
            Ok(_) => return Ok(()),
            Err(error) if error.to_string().contains("status=-300") => {
                last_error = Some(error);
                continue;
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("LOCO probe reconnect failed")))
}

fn fetch_loco_member_profiles(chat_id: i64) -> Result<Vec<LocoMemberProfile>> {
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;
        fetch_loco_member_profiles_only_with_client(&mut client, chat_id).await
    })
}

fn cmd_loco_members(chat_id: i64, full: bool, json: bool) -> Result<()> {
    let profiles = fetch_loco_member_profiles(chat_id)?;

    if json {
        if full {
            println!("{}", serde_json::to_string_pretty(&profiles)?);
        } else {
            let members = profiles
                .iter()
                .map(LocoMemberProfile::as_chat_member)
                .collect::<Vec<_>>();
            println!("{}", serde_json::to_string_pretty(&members)?);
        }
        return Ok(());
    }

    if full {
        print_section_title(&format!(
            "Members of chat {} ({} members)",
            chat_id,
            profiles.len()
        ));
        let rows = profiles
            .iter()
            .map(|profile| {
                vec![
                    profile.nickname.clone(),
                    truncate(&profile.status_message, 30),
                    profile.country_iso.clone(),
                    if profile.suspended {
                        "yes".into()
                    } else {
                        "no".into()
                    },
                    profile.user_id.to_string(),
                ]
            })
            .collect::<Vec<_>>();
        print_table(&["Name", "Status", "Country", "Suspended", "User ID"], rows);
        return Ok(());
    }

    print_section_title(&format!(
        "Members of chat {} ({} members)",
        chat_id,
        profiles.len()
    ));
    for profile in &profiles {
        if color_enabled() {
            println!(
                "  {} {}",
                format!("{}", profile.user_id).dimmed(),
                profile.nickname.bold()
            );
        } else {
            println!("  {} {}", profile.user_id, profile.nickname);
        }
    }

    Ok(())
}

async fn fetch_loco_blocked_snapshot(
    client: &mut loco::client::LocoClient,
) -> Result<LocoBlockedSnapshot> {
    let sync_result = client
        .send_command_collect(
            "BLSYNC",
            bson::doc! { "r": 0_i32, "pr": 0_i32 },
            Duration::from_secs(3),
        )
        .await?;

    let sync_packet = sync_result
        .response
        .as_ref()
        .or_else(|| {
            sync_result
                .pushes
                .iter()
                .find(|packet| packet.method == "BLSYNC")
        })
        .ok_or_else(|| anyhow::anyhow!("BLSYNC returned neither a direct response nor a push"))?;

    let revision = get_bson_i64(&sync_packet.body, &["r", "revision"]);
    let plus_revision = get_bson_i64(&sync_packet.body, &["pr", "plusRevision"]);
    let ids = get_bson_i64_array(&sync_packet.body, &["l", "blockIds"]);
    let types = get_bson_i32_array(&sync_packet.body, &["ts", "blockTypes"]);
    let plus_ids = get_bson_i64_array(&sync_packet.body, &["pl", "plusBlockIds"]);
    let plus_types = get_bson_i32_array(&sync_packet.body, &["pts", "plusBlockTypes"]);

    if ids.is_empty() && plus_ids.is_empty() {
        return Ok(LocoBlockedSnapshot {
            revision,
            plus_revision,
            members: Vec::new(),
        });
    }

    let member_body = bson::doc! {
        "l": bson::Bson::Array(ids.iter().copied().map(bson::Bson::Int64).collect()),
        "pl": bson::Bson::Array(plus_ids.iter().copied().map(bson::Bson::Int64).collect()),
    };
    let member_result = client
        .send_command_collect("BLMEMBER", member_body, Duration::from_secs(3))
        .await?;
    let member_packet = member_result
        .response
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("BLMEMBER did not return a direct response"))?;

    if member_packet.status() != 0 {
        anyhow::bail!("BLMEMBER failed (status={})", member_packet.status());
    }

    let mut members = Vec::new();
    if let Ok(entries) = member_packet.body.get_array("l") {
        for (idx, entry) in entries.iter().enumerate() {
            if let Some(doc) = entry.as_document() {
                members.push(LocoBlockedMember {
                    user_id: get_bson_i64(doc, &["userId"]),
                    nickname: get_bson_str(doc, &["nickName", "nickname"]),
                    profile_image_url: get_bson_str(doc, &["profileImageUrl"]),
                    full_profile_image_url: get_bson_str(doc, &["fullProfileImageUrl"]),
                    suspended: get_bson_bool(doc, &["suspended"]),
                    suspicion: get_bson_str(doc, &["suspicion"]),
                    block_type: types.get(idx).copied().unwrap_or(0),
                    is_plus: false,
                });
            }
        }
    }
    if let Ok(entries) = member_packet.body.get_array("pl") {
        for (idx, entry) in entries.iter().enumerate() {
            if let Some(doc) = entry.as_document() {
                members.push(LocoBlockedMember {
                    user_id: get_bson_i64(doc, &["userId"]),
                    nickname: get_bson_str(doc, &["nickName", "nickname"]),
                    profile_image_url: get_bson_str(doc, &["profileImageUrl"]),
                    full_profile_image_url: get_bson_str(doc, &["fullProfileImageUrl"]),
                    suspended: get_bson_bool(doc, &["suspended"]),
                    suspicion: get_bson_str(doc, &["suspicion"]),
                    block_type: plus_types.get(idx).copied().unwrap_or(0),
                    is_plus: true,
                });
            }
        }
    }

    Ok(LocoBlockedSnapshot {
        revision,
        plus_revision,
        members,
    })
}

fn cmd_loco_blocked(json: bool) -> Result<()> {
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

        let snapshot = fetch_loco_blocked_snapshot(&mut client).await?;

        if json {
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            return Ok(());
        }

        print_section_title(&format!(
            "LOCO blocked members ({})",
            snapshot.members.len()
        ));
        println!(
            "  revision={} plus_revision={}",
            snapshot.revision, snapshot.plus_revision
        );

        let rows = snapshot
            .members
            .iter()
            .map(|member| {
                vec![
                    member.nickname.clone(),
                    member.block_type.to_string(),
                    if member.is_plus {
                        "plus".into()
                    } else {
                        "user".into()
                    },
                    if member.suspended {
                        "yes".into()
                    } else {
                        "no".into()
                    },
                    member.user_id.to_string(),
                ]
            })
            .collect::<Vec<_>>();
        print_table(&["Name", "Type", "Scope", "Suspended", "User ID"], rows);
        Ok(())
    })
}

fn kakao_container_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("Library/Containers/com.kakao.KakaoTalkMac/Data")
}

fn kakao_cache_db_path() -> PathBuf {
    kakao_container_dir().join("Library/Caches/Cache.db")
}

fn kakao_preferences_dir() -> PathBuf {
    kakao_container_dir().join("Library/Preferences")
}

fn parse_i64_list(raw: &str) -> Vec<i64> {
    raw.trim_matches(&['[', ']'][..])
        .split(',')
        .filter_map(|part| part.trim().parse::<i64>().ok())
        .collect()
}

fn parse_profile_cache_hint(
    entry_id: i64,
    request_key: &str,
    data_on_fs: bool,
) -> ProfileCacheHint {
    let mut kind = "other".to_string();
    let mut user_ids = Vec::new();
    let mut chat_id = None;
    let mut access_permit = None;
    let mut category = None;

    if let Ok(url) = reqwest::Url::parse(request_key) {
        let path = url.path();
        kind = match path {
            "/mac/profile3/friend.json" => "friend".to_string(),
            "/mac/profile3/friends.json" => "friends".to_string(),
            "/mac/profile/designated_friends.json" => "designated-friends".to_string(),
            _ => path.rsplit('/').next().unwrap_or("other").to_string(),
        };

        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "id" => {
                    if let Ok(user_id) = value.parse::<i64>() {
                        user_ids.push(user_id);
                    }
                }
                "ids" => {
                    user_ids.extend(parse_i64_list(&value));
                }
                "chatId" => {
                    if let Ok(parsed) = value.parse::<i64>() {
                        chat_id = Some(parsed);
                    }
                }
                "accessPermit" => {
                    access_permit = Some(value.to_string());
                }
                "category" => {
                    category = Some(value.to_string());
                }
                _ => {}
            }
        }
    }

    ProfileCacheHint {
        entry_id,
        kind,
        request_key: request_key.to_string(),
        user_ids,
        chat_id,
        access_permit,
        category,
        data_on_fs,
    }
}

fn load_profile_cache_hints(limit: usize) -> Result<Vec<ProfileCacheHint>> {
    let cache_db = kakao_cache_db_path();
    let conn = rusqlite::Connection::open_with_flags(
        &cache_db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("failed to open {}", cache_db.display()))?;

    let sql = r#"
        SELECT
            r.entry_ID,
            r.request_key,
            COALESCE(d.isDataOnFS, 0)
        FROM cfurl_cache_response r
        LEFT JOIN cfurl_cache_receiver_data d ON d.entry_ID = r.entry_ID
        WHERE r.request_key LIKE '%/mac/profile3/friend.json%'
           OR r.request_key LIKE '%/mac/profile3/friends.json%'
           OR r.request_key LIKE '%/mac/profile/designated_friends.json%'
        ORDER BY r.entry_ID DESC
        LIMIT ?1
    "#;
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([limit as i64], |row| {
        let entry_id: i64 = row.get(0)?;
        let request_key: String = row.get(1)?;
        let data_on_fs: i64 = row.get(2)?;
        Ok(parse_profile_cache_hint(
            entry_id,
            &request_key,
            data_on_fs != 0,
        ))
    })?;

    let mut hints = Vec::new();
    for row in rows {
        hints.push(row?);
    }
    Ok(hints)
}

fn plist_i64(value: &plist::Value) -> Option<i64> {
    match value {
        plist::Value::Integer(num) => num.as_signed(),
        plist::Value::Real(num) => Some(*num as i64),
        _ => None,
    }
}

fn plist_bool(value: &plist::Value) -> Option<bool> {
    match value {
        plist::Value::Boolean(value) => Some(*value),
        _ => None,
    }
}

fn load_profile_revision_hints() -> Result<ProfileRevisionHints> {
    let prefs_dir = kakao_preferences_dir();
    let mut hints = ProfileRevisionHints::default();

    for entry in std::fs::read_dir(&prefs_dir)
        .with_context(|| format!("failed to read {}", prefs_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("plist") {
            continue;
        }

        let plist = match plist::Value::from_file(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(dict) = plist.as_dictionary() else {
            continue;
        };

        for (key, value) in dict {
            if key.starts_with("PROFILELISTREVISION:") {
                if let Some(revision) = plist_i64(value).filter(|value| *value > 0) {
                    hints.profile_list_revision = Some(
                        hints
                            .profile_list_revision
                            .map_or(revision, |cur| cur.max(revision)),
                    );
                }
            } else if key.starts_with("DESIGNATEDFRIENDSREVISION:") {
                if let Some(revision) = plist_i64(value).filter(|value| *value > 0) {
                    hints.designated_friends_revision = Some(
                        hints
                            .designated_friends_revision
                            .map_or(revision, |cur| cur.max(revision)),
                    );
                }
            } else if key == "kLocoBlockFriendsSyncKey" {
                hints.block_friends_sync_enabled = plist_bool(value);
            } else if key == "kLocoBlockChannelsSyncKey" {
                hints.block_channels_sync_enabled = plist_bool(value);
            }
        }
    }

    Ok(hints)
}

fn metadata_modified_unix(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
}

fn collect_kakao_app_state_files(
    dir: &std::path::Path,
    relative_to: &std::path::Path,
    files: &mut Vec<KakaoAppStateFile>,
    depth: usize,
) -> Result<()> {
    if depth == 0 || !dir.exists() {
        return Ok(());
    }

    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let relative = path
            .strip_prefix(relative_to)
            .unwrap_or(&path)
            .display()
            .to_string();

        if metadata.is_dir() {
            files.push(KakaoAppStateFile {
                path: relative.clone(),
                kind: "dir".into(),
                size: 0,
                modified_unix: metadata_modified_unix(&metadata),
            });
            collect_kakao_app_state_files(&path, relative_to, files, depth.saturating_sub(1))?;
        } else if metadata.is_file() {
            files.push(KakaoAppStateFile {
                path: relative,
                kind: "file".into(),
                size: metadata.len(),
                modified_unix: metadata_modified_unix(&metadata),
            });
        }
    }

    Ok(())
}

fn load_kakao_app_state_snapshot() -> Result<KakaoAppStateSnapshot> {
    let root = kakao_container_dir().join("Library/Application Support/com.kakao.KakaoTalkMac");
    let preferences_dir = kakao_preferences_dir();
    let cache_db = kakao_cache_db_path();
    let mut files = Vec::new();

    collect_kakao_app_state_files(&root, &root, &mut files, 2)?;
    collect_kakao_app_state_files(&preferences_dir, &preferences_dir, &mut files, 1)?;
    if cache_db.exists() {
        let metadata = std::fs::metadata(&cache_db)
            .with_context(|| format!("failed to stat {}", cache_db.display()))?;
        files.push(KakaoAppStateFile {
            path: cache_db.display().to_string(),
            kind: "file".into(),
            size: metadata.len(),
            modified_unix: metadata_modified_unix(&metadata),
        });
    }

    files.sort_by(|a, b| {
        b.modified_unix
            .cmp(&a.modified_unix)
            .then_with(|| a.path.cmp(&b.path))
    });

    Ok(KakaoAppStateSnapshot {
        root: root.display().to_string(),
        preferences_dir: preferences_dir.display().to_string(),
        cache_db: cache_db.display().to_string(),
        files,
    })
}

fn load_profile_hints_baseline(path: &str) -> Result<ProfileHintsBaseline> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path))
}

fn diff_kakao_app_state(
    before: &KakaoAppStateSnapshot,
    after: &KakaoAppStateSnapshot,
) -> Vec<KakaoAppStateDiffEntry> {
    let before_map = before
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<HashMap<_, _>>();
    let after_map = after
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<HashMap<_, _>>();
    let mut paths = before_map
        .keys()
        .chain(after_map.keys())
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();

    let mut diff = Vec::new();
    for path in paths {
        let before = before_map.get(&path).copied();
        let after = after_map.get(&path).copied();
        let change = match (before, after) {
            (None, Some(_)) => Some("added"),
            (Some(_), None) => Some("removed"),
            (Some(before), Some(after))
                if before.size != after.size
                    || before.modified_unix != after.modified_unix
                    || before.kind != after.kind =>
            {
                Some("changed")
            }
            _ => None,
        };
        if let Some(change) = change {
            diff.push(KakaoAppStateDiffEntry {
                path,
                change: change.into(),
                before_size: before.map(|file| file.size),
                after_size: after.map(|file| file.size),
                before_modified_unix: before.and_then(|file| file.modified_unix),
                after_modified_unix: after.and_then(|file| file.modified_unix),
            });
        }
    }

    diff.sort_by(|a, b| a.path.cmp(&b.path));
    diff
}

fn cmd_profile_hints(
    app_state: bool,
    app_state_diff: Option<String>,
    local_graph: bool,
    user_id: Option<i64>,
    probe_syncmainpf: bool,
    probe_uplinkprof: bool,
    json: bool,
) -> Result<()> {
    if app_state_diff.is_some() && !app_state {
        anyhow::bail!("--app-state-diff requires --app-state");
    }
    if (probe_syncmainpf || probe_uplinkprof) && (!local_graph || user_id.is_none()) {
        anyhow::bail!(
            "--probe-syncmainpf/--probe-uplinkprof require both --local-graph and --user-id"
        );
    }

    let cached_requests = load_profile_cache_hints(12)?;
    let app_state_snapshot = if app_state {
        Some(load_kakao_app_state_snapshot()?)
    } else {
        None
    };
    let app_state_diff_entries = match (&app_state_snapshot, app_state_diff.as_deref()) {
        (Some(current), Some(path)) => {
            let baseline = load_profile_hints_baseline(path)?;
            let Some(previous) = baseline.app_state else {
                anyhow::bail!("baseline snapshot does not contain app_state");
            };
            Some(diff_kakao_app_state(&previous, current))
        }
        _ => None,
    };
    let local_graph_snapshot = if local_graph {
        Some(build_local_friend_graph()?)
    } else {
        None
    };
    let local_graph_summary = local_graph_snapshot
        .as_ref()
        .map(|graph| local_graph_hint_summary(graph, &cached_requests));
    let syncmainpf_candidates = match (&local_graph_snapshot, user_id) {
        (Some(graph), Some(user_id)) => {
            build_syncmainpf_candidate(graph, &cached_requests, user_id)
                .into_iter()
                .collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };
    let syncmainpf_probe_results = if probe_syncmainpf {
        let variants = syncmainpf_candidates
            .iter()
            .flat_map(build_syncmainpf_probe_variants)
            .collect::<Vec<_>>();
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { probe_syncmainpf_variants(&variants).await })?
    } else {
        Vec::new()
    };
    let uplinkprof_probe_results = if probe_uplinkprof {
        let variants = syncmainpf_candidates
            .iter()
            .flat_map(build_uplinkprof_probe_variants)
            .collect::<Vec<_>>();
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { probe_method_variants("UPLINKPROF", &variants).await })?
    } else {
        Vec::new()
    };
    let snapshot = ProfileHintsSnapshot {
        revisions: load_profile_revision_hints()?,
        cached_requests,
        app_state: app_state_snapshot,
        app_state_diff: app_state_diff_entries,
        local_graph: local_graph_summary,
        syncmainpf_candidates,
        syncmainpf_probe_results,
        uplinkprof_probe_results,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
        return Ok(());
    }

    print_section_title("Profile hints");
    println!(
        "  profile_list_revision: {}",
        snapshot
            .revisions
            .profile_list_revision
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  designated_friends_revision: {}",
        snapshot
            .revisions
            .designated_friends_revision
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  block_friends_sync: {}",
        snapshot
            .revisions
            .block_friends_sync_enabled
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  block_channels_sync: {}",
        snapshot
            .revisions
            .block_channels_sync_enabled
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into())
    );
    if let Some(local_graph) = &snapshot.local_graph {
        println!(
            "  local_graph: users={} chats={} failed_chats={}",
            local_graph.user_count,
            local_graph.chat_count,
            local_graph.failed_chat_ids.len()
        );
        let token_preview = local_graph
            .chat_meta
            .iter()
            .filter_map(|chat| {
                chat.getmem_token.map(|token| {
                    format!(
                        "{}:{} ({})",
                        chat.chat_id,
                        token,
                        if chat.title.is_empty() {
                            "-"
                        } else {
                            chat.title.as_str()
                        }
                    )
                })
            })
            .take(5)
            .collect::<Vec<_>>();
        if !token_preview.is_empty() {
            println!("  local_graph_tokens: {}", token_preview.join(", "));
        }
    }
    if let Some(app_state) = &snapshot.app_state {
        println!("  app_state_files: {}", app_state.files.len());
        let recent = app_state
            .files
            .iter()
            .take(5)
            .map(|file| format!("{} [{} bytes]", file.path, file.size))
            .collect::<Vec<_>>();
        if !recent.is_empty() {
            println!("  app_state_recent: {}", recent.join(", "));
        }
    }
    if let Some(diff) = &snapshot.app_state_diff {
        println!("  app_state_diff: {} changed entries", diff.len());
    }
    if let Some(candidate) = snapshot.syncmainpf_candidates.first() {
        println!(
            "  syncmainpf_candidates: {}  uplinkprof_candidates: {}",
            candidate.bodies.len(),
            candidate.uplinkprof_bodies.len()
        );
    }
    println!();

    let rows = snapshot
        .cached_requests
        .iter()
        .map(|hint| {
            let ids = if hint.user_ids.is_empty() {
                "-".to_string()
            } else if hint.user_ids.len() == 1 {
                hint.user_ids[0].to_string()
            } else {
                format!(
                    "{} (+{})",
                    hint.user_ids[0],
                    hint.user_ids.len().saturating_sub(1)
                )
            };
            let access = hint
                .access_permit
                .as_deref()
                .map(|value| value.chars().take(8).collect::<String>())
                .unwrap_or_else(|| "-".into());
            let local_match = snapshot
                .local_graph
                .as_ref()
                .and_then(|summary| {
                    summary
                        .candidate_matches
                        .iter()
                        .find(|candidate| candidate.entry_id == hint.entry_id)
                })
                .map(|matched| {
                    if matched.matched_user_ids.is_empty() {
                        "-".to_string()
                    } else {
                        format!(
                            "{} chat(s), {} permit(s)",
                            matched.candidate_chat_ids.len(),
                            matched.candidate_access_permits.len()
                        )
                    }
                })
                .unwrap_or_else(|| "-".into());
            vec![
                hint.entry_id.to_string(),
                hint.kind.clone(),
                ids,
                hint.chat_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".into()),
                access,
                hint.category.clone().unwrap_or_else(|| "-".into()),
                if hint.data_on_fs {
                    "fs".into()
                } else {
                    "inline".into()
                },
                local_match,
            ]
        })
        .collect::<Vec<_>>();

    print_table(
        &[
            "Entry",
            "Kind",
            "User IDs",
            "Chat ID",
            "Permit",
            "Category",
            "Body",
            "Local graph",
        ],
        rows,
    );

    if let Some(candidate) = snapshot.syncmainpf_candidates.first() {
        println!();
        print_section_title(&format!(
            "SYNCMAINPF candidate bodies for {}",
            candidate.user_id
        ));
        println!(
            "  account_id: {}  self: {}  source_entry_ids: {}",
            candidate.account_id,
            candidate.is_self,
            if candidate.source_entry_ids.is_empty() {
                "-".to_string()
            } else {
                candidate
                    .source_entry_ids
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        for body in &candidate.bodies {
            println!("  {}", serde_json::to_string(body)?);
        }

        println!();
        print_section_title(&format!(
            "UPLINKPROF candidate bodies for {}",
            candidate.user_id
        ));
        for body in &candidate.uplinkprof_bodies {
            println!("  {}", serde_json::to_string(body)?);
        }
    }

    if !snapshot.syncmainpf_probe_results.is_empty() {
        println!();
        print_section_title("SYNCMAINPF probe results");
        for result in &snapshot.syncmainpf_probe_results {
            let body_status = result
                .body_status
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into());
            let pushes = if result.push_methods.is_empty() {
                "-".to_string()
            } else {
                result.push_methods.join(",")
            };
            println!(
                "  packet_status={} body_status={} pushes={} methods={} body={}",
                result.packet_status_code,
                body_status,
                result.push_count,
                pushes,
                serde_json::to_string(&result.body)?
            );
        }
    }

    if !snapshot.uplinkprof_probe_results.is_empty() {
        println!();
        print_section_title("UPLINKPROF probe results");
        for result in &snapshot.uplinkprof_probe_results {
            let body_status = result
                .body_status
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into());
            let pushes = if result.push_methods.is_empty() {
                "-".to_string()
            } else {
                result.push_methods.join(",")
            };
            println!(
                "  packet_status={} body_status={} pushes={} methods={} body={}",
                result.packet_status_code,
                body_status,
                result.push_count,
                pushes,
                serde_json::to_string(&result.body)?
            );
        }
    }

    Ok(())
}

fn cmd_loco_chatinfo(chat_id: i64, json: bool) -> Result<()> {
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

        // Special: chat_id=0 → find or create MemoChat ("나와의 채팅")
        if chat_id == 0 {
            eprintln!("Finding MemoChat (나와의 채팅)...");

            // First scan LOGINLIST chatDatas for existing MemoChat
            let login_data = client.full_connect_with_retry(3).await?;
            if let Ok(chat_datas) = login_data.get_array("chatDatas") {
                for cd in chat_datas {
                    if let Some(doc) = cd.as_document() {
                        let ctype = doc.get_str("t").unwrap_or("?");
                        if ctype == "MemoChat" {
                            let cid = doc
                                .get_i64("c")
                                .or_else(|_| doc.get_i32("c").map(|v| v as i64))
                                .unwrap_or(0);
                            println!("Memo chat ID: {}", cid);
                            return Ok(());
                        }
                    }
                }
            }

            // Not found — create one via CREATE with memoChat=true (node-kakao pattern)
            eprintln!("No existing MemoChat found, creating...");
            let resp = client
                .send_command(
                    "CREATE",
                    bson::doc! {
                        "memberIds": bson::Bson::Array(vec![]),
                        "memoChat": true,
                    },
                )
                .await?;

            let status = resp.status();
            if status == 0 {
                let memo_id = resp
                    .body
                    .get_i64("chatId")
                    .or_else(|_| resp.body.get_i32("chatId").map(|v| v as i64))
                    .unwrap_or(0);
                println!("MemoChat created! ID: {}", memo_id);
            } else {
                eprintln!("CREATE MemoChat failed (status={})", status);
                eprintln!("Response: {:?}", resp.body);
            }
            return Ok(());
        }

        let response = client
            .send_command("CHATINFO", bson::doc! { "chatId": chat_id })
            .await?;

        if response.status() != 0 {
            anyhow::bail!("CHATINFO failed (status={})", response.status());
        }

        if json {
            // Convert BSON body to JSON
            let json_val: serde_json::Value = bson::from_document(response.body.clone())?;
            println!("{}", serde_json::to_string_pretty(&json_val)?);
        } else {
            print_section_title(&format!("Chat info: {}", chat_id));
            for (k, v) in response.body.iter() {
                let v_str = format!("{:?}", v);
                if v_str.len() > 100 {
                    println!("  {}: {}...", k, &v_str[..100]);
                } else {
                    println!("  {}: {}", k, v_str);
                }
            }
        }

        Ok(())
    })
}

fn parse_loco_probe_body(body: Option<&str>) -> Result<bson::Document> {
    let Some(raw) = body else {
        return Ok(bson::Document::new());
    };

    let value: serde_json::Value =
        serde_json::from_str(raw).context("probe body must be valid JSON")?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("probe body must be a JSON object"))?;

    bson::to_document(object).context("failed to convert probe JSON to BSON document")
}

fn cmd_loco_probe(method: &str, body: Option<&str>, json: bool) -> Result<()> {
    let creds = get_creds()?;
    let method = method.to_uppercase();
    let request_body = parse_loco_probe_body(body)?;
    let request_json = serde_json::to_value(&request_body)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

        let result = client
            .send_command_collect(&method, request_body.clone(), Duration::from_secs(3))
            .await?;
        let response_json = result
            .response
            .as_ref()
            .map(|response| {
                Ok::<_, anyhow::Error>(serde_json::json!({
                    "method": response.method,
                    "packet_id": response.packet_id,
                    "status_code": response.status_code,
                    "status": response.status(),
                    "body_type": response.body_type,
                    "body": serde_json::to_value(&response.body)?,
                }))
            })
            .transpose()?;
        let pushes_json = result
            .pushes
            .iter()
            .map(|packet| {
                Ok::<_, anyhow::Error>(serde_json::json!({
                    "method": packet.method,
                    "packet_id": packet.packet_id,
                    "status_code": packet.status_code,
                    "status": packet.status(),
                    "body_type": packet.body_type,
                    "body": serde_json::to_value(&packet.body)?,
                }))
            })
            .collect::<Result<Vec<_>>>()?;

        let payload = serde_json::json!({
            "request": {
                "method": &method,
                "body": request_json,
            },
            "response_present": response_json.is_some(),
            "push_count": pushes_json.len(),
            "empty_within_timeout": response_json.is_none() && pushes_json.is_empty(),
            "response": response_json,
            "pushes": pushes_json,
        });

        if json {
            println!("{}", serde_json::to_string_pretty(&payload)?);
        } else {
            print_section_title(&format!("LOCO probe: {}", method));
            if let Some(response) = &result.response {
                println!("  status: {}", response.status());
                println!("  packet: {}", response.packet_id);
                println!("{}", serde_json::to_string_pretty(&payload["response"])?);
            } else {
                println!("  response: <none> (no direct response within timeout)");
            }
            if !result.pushes.is_empty() {
                println!("  pushes: {}", result.pushes.len());
                println!("{}", serde_json::to_string_pretty(&payload["pushes"])?);
            } else if result.response.is_none() {
                println!("  pushes: <none> (no push packets within timeout)");
            }
        }

        Ok(())
    })
}

fn cmd_watch_cache(interval: u64) -> Result<()> {
    eprintln!(
        "Watching Cache.db for fresh tokens (interval={}s)...",
        interval
    );
    eprintln!("Open KakaoTalk and use it normally. Press Ctrl-C to stop.");

    let mut last_token = extract_refresh_token()?.unwrap_or_default();
    let mut last_oauth = get_credential_candidates(1)?
        .first()
        .map(|c| c.oauth_token.clone())
        .unwrap_or_default();

    if !last_token.is_empty() {
        eprintln!(
            "  Current refresh_token: {}...",
            last_token.chars().take(8).collect::<String>()
        );
    }
    if !last_oauth.is_empty() {
        eprintln!(
            "  Current oauth_token:   {}...",
            last_oauth.chars().take(8).collect::<String>()
        );
    }

    loop {
        std::thread::sleep(std::time::Duration::from_secs(interval));

        // Check refresh_token
        if let Ok(Some(rt)) = extract_refresh_token() {
            if rt != last_token {
                if color_enabled() {
                    eprintln!("{}", "NEW refresh_token detected!".green().bold());
                } else {
                    eprintln!("NEW refresh_token detected!");
                }
                eprintln!("  {}...", rt.chars().take(60).collect::<String>());
                last_token = rt.clone();

                // Try renewal immediately
                if let Ok(creds) = get_creds() {
                    match try_renew_token(&creds, &rt) {
                        Ok(Some(new_token)) => {
                            if color_enabled() {
                                eprintln!("{}", "Token renewal SUCCEEDED!".green().bold());
                            } else {
                                eprintln!("Token renewal SUCCEEDED!");
                            }
                            eprintln!(
                                "  New access_token: {}...",
                                new_token.chars().take(8).collect::<String>()
                            );
                            // Save the new credentials
                            let mut new_creds = creds.clone();
                            new_creds.oauth_token = new_token;
                            new_creds.refresh_token = Some(rt);
                            if let Ok(path) = save_credentials(&new_creds) {
                                eprintln!("  Saved to {}", path.display());
                            }
                        }
                        Ok(None) => eprintln!("  Renewal returned no access_token."),
                        Err(e) => eprintln!("  Renewal failed: {}", e),
                    }
                }
            }
        }

        // Check oauth_token
        if let Ok(candidates) = get_credential_candidates(1) {
            if let Some(cand) = candidates.first() {
                if cand.oauth_token != last_oauth {
                    if color_enabled() {
                        eprintln!("{}", "NEW oauth_token detected!".green().bold());
                    } else {
                        eprintln!("NEW oauth_token detected!");
                    }
                    eprintln!(
                        "  {}...",
                        cand.oauth_token.chars().take(8).collect::<String>()
                    );
                    last_oauth = cand.oauth_token.clone();
                }
            }
        }

        eprint!(".");
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn confirm() -> Result<bool> {
    use std::io::Write;
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y"))
}

fn require_permission(enabled: bool, purpose: &str, hint: &str) -> Result<()> {
    if enabled {
        return Ok(());
    }

    anyhow::bail!("{} requires explicit opt-in. {}", purpose, hint)
}

fn get_rest_client() -> Result<KakaoRestClient> {
    get_rest_ready_client()
}

fn cmd_doctor(json: bool, test_loco: bool, config: &OpenKakaoConfig) -> Result<()> {
    use std::path::PathBuf;

    struct Check {
        name: String,
        status: CheckStatus,
        detail: String,
    }

    enum CheckStatus {
        Ok,
        Warn,
        Fail,
    }

    let mut checks: Vec<Check> = Vec::new();
    let mut installed_version: Option<String> = None;
    let mut saved_app_version: Option<String> = None;
    let recovery = recovery_snapshot()?;
    let safety = safety_snapshot(
        config
            .safety
            .min_unattended_send_interval_secs
            .unwrap_or(10),
        config.safety.min_hook_interval_secs.unwrap_or(2),
        config.safety.min_webhook_interval_secs.unwrap_or(2),
    )?;

    checks.push(Check {
        name: "State file".into(),
        status: CheckStatus::Ok,
        detail: recovery.path.clone(),
    });
    checks.push(Check {
        name: "Auth recovery state".into(),
        status: if recovery.auth_cooldown_remaining_secs.is_some()
            || recovery.consecutive_failures > 0
        {
            CheckStatus::Warn
        } else {
            CheckStatus::Ok
        },
        detail: format!(
            "failures={}, last_failure={}, auth_cooldown={}, last_success={} via {}",
            recovery.consecutive_failures,
            recovery.last_failure_kind.as_deref().unwrap_or("none"),
            format_remaining(
                recovery.auth_cooldown_remaining_secs,
                recovery.cooldown_until.as_deref()
            ),
            recovery
                .last_success_transport
                .as_deref()
                .unwrap_or("never"),
            recovery.last_recovery_source.as_deref().unwrap_or("none")
        ),
    });
    checks.push(Check {
        name: "Safety guards".into(),
        status: if safety.last_guard_reason.is_some() {
            CheckStatus::Warn
        } else {
            CheckStatus::Ok
        },
        detail: format!(
            "send={}s, hook={}s, webhook={}s, hook_timeout={}s, webhook_timeout={}s, insecure_webhooks={}, last_guard={}",
            config.safety.min_unattended_send_interval_secs.unwrap_or(10),
            config.safety.min_hook_interval_secs.unwrap_or(2),
            config.safety.min_webhook_interval_secs.unwrap_or(2),
            config.safety.hook_timeout_secs.unwrap_or(20),
            config.safety.webhook_timeout_secs.unwrap_or(10),
            if config.safety.allow_insecure_webhooks { "allowed" } else { "blocked" },
            safety.last_guard_reason.as_deref().unwrap_or("none")
        ),
    });

    // 1. KakaoTalk.app installed version
    let app_plist = PathBuf::from("/Applications/KakaoTalk.app/Contents/Info.plist");
    if app_plist.exists() {
        match plist::from_file::<_, plist::Dictionary>(&app_plist) {
            Ok(dict) => {
                let version = dict
                    .get("CFBundleShortVersionString")
                    .and_then(|v| v.as_string())
                    .unwrap_or("unknown");
                installed_version = Some(version.to_string());
                let bundle_id = dict
                    .get("CFBundleIdentifier")
                    .and_then(|v| v.as_string())
                    .unwrap_or("unknown");
                checks.push(Check {
                    name: "KakaoTalk.app".into(),
                    status: CheckStatus::Ok,
                    detail: format!("v{} ({})", version, bundle_id),
                });
            }
            Err(e) => {
                checks.push(Check {
                    name: "KakaoTalk.app".into(),
                    status: CheckStatus::Warn,
                    detail: format!("Installed but cannot read Info.plist: {}", e),
                });
            }
        }
    } else {
        checks.push(Check {
            name: "KakaoTalk.app".into(),
            status: CheckStatus::Fail,
            detail: "Not found in /Applications".into(),
        });
    }

    // 2. KakaoTalk process running
    let pgrep_output = std::process::Command::new("pgrep")
        .args(["-x", "KakaoTalk"])
        .output();
    match pgrep_output {
        Ok(output) if output.status.success() => {
            let pids = String::from_utf8_lossy(&output.stdout).trim().to_string();
            checks.push(Check {
                name: "KakaoTalk process".into(),
                status: CheckStatus::Ok,
                detail: format!("Running (PID: {})", pids.replace('\n', ", ")),
            });
        }
        _ => {
            checks.push(Check {
                name: "KakaoTalk process".into(),
                status: CheckStatus::Warn,
                detail: "Not running. Start KakaoTalk to refresh tokens.".into(),
            });
        }
    }

    // 3. Cache.db existence and freshness
    let home = dirs::home_dir().unwrap_or_default();
    let cache_db =
        home.join("Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Caches/Cache.db");
    if cache_db.exists() {
        match std::fs::metadata(&cache_db) {
            Ok(meta) => {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .map(|d| {
                        if d.as_secs() < 60 {
                            format!("{}s ago", d.as_secs())
                        } else if d.as_secs() < 3600 {
                            format!("{}m ago", d.as_secs() / 60)
                        } else if d.as_secs() < 86400 {
                            format!("{}h ago", d.as_secs() / 3600)
                        } else {
                            format!("{}d ago", d.as_secs() / 86400)
                        }
                    })
                    .unwrap_or_else(|| "unknown".into());
                let size_kb = meta.len() / 1024;
                let status = if meta
                    .modified()
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|d| d.as_secs() > 86400)
                {
                    CheckStatus::Warn
                } else {
                    CheckStatus::Ok
                };
                checks.push(Check {
                    name: "Cache.db".into(),
                    status,
                    detail: format!("{}KB, modified {}", size_kb, modified),
                });
            }
            Err(e) => {
                checks.push(Check {
                    name: "Cache.db".into(),
                    status: CheckStatus::Warn,
                    detail: format!("Exists but unreadable: {}", e),
                });
            }
        }
    } else {
        checks.push(Check {
            name: "Cache.db".into(),
            status: CheckStatus::Fail,
            detail: "Not found. Has KakaoTalk been used on this Mac?".into(),
        });
    }

    // 4. Saved credentials file
    match credentials::credentials_path() {
        Ok(path) => {
            if path.exists() {
                match credentials::load_credentials() {
                    Ok(Some(creds)) => {
                        saved_app_version = Some(creds.app_version.clone());
                        checks.push(Check {
                            name: "Saved credentials".into(),
                            status: CheckStatus::Ok,
                            detail: format!(
                                "user_id={}, version={}, token={}...",
                                creds.user_id,
                                creds.app_version,
                                creds.oauth_token.chars().take(8).collect::<String>()
                            ),
                        });
                    }
                    Ok(None) => {
                        checks.push(Check {
                            name: "Saved credentials".into(),
                            status: CheckStatus::Warn,
                            detail: "File exists but empty/invalid".into(),
                        });
                    }
                    Err(e) => {
                        checks.push(Check {
                            name: "Saved credentials".into(),
                            status: CheckStatus::Warn,
                            detail: format!("Parse error: {}", e),
                        });
                    }
                }
            } else {
                checks.push(Check {
                    name: "Saved credentials".into(),
                    status: CheckStatus::Warn,
                    detail: format!(
                        "Not found. Run 'openkakao-rs login --save'. ({})",
                        path.display()
                    ),
                });
            }
        }
        Err(e) => {
            checks.push(Check {
                name: "Saved credentials".into(),
                status: CheckStatus::Fail,
                detail: format!("Cannot determine path: {}", e),
            });
        }
    }

    // 4b. Version drift: compare installed KakaoTalk version vs saved credentials version
    match (&installed_version, &saved_app_version) {
        (Some(installed), Some(saved)) => {
            if installed == saved {
                checks.push(Check {
                    name: "Version match".into(),
                    status: CheckStatus::Ok,
                    detail: format!("Installed and saved both v{}", installed),
                });
            } else {
                checks.push(Check {
                    name: "Version drift".into(),
                    status: CheckStatus::Warn,
                    detail: format!(
                        "Installed v{} != saved v{}. Run `relogin --fresh-xvc` to re-authenticate.",
                        installed, saved
                    ),
                });
            }
        }
        _ => {
            // Can't compare if either is missing — already covered by their own checks
        }
    }

    // 5. Token validity via REST API (non-interactive: pick first credential, no prompt)
    let creds_result: Result<KakaoCredentials> = {
        // Try saved credentials first, then Cache.db extraction (no interactive prompts)
        if let Ok(Some(saved)) = load_credentials() {
            Ok(saved)
        } else {
            let candidates = get_credential_candidates(4).unwrap_or_default();
            candidates
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No credentials found"))
        }
    };
    match &creds_result {
        Ok(creds) => match KakaoRestClient::new(creds.clone()) {
            Ok(client) => match client.verify_token() {
                Ok(true) => {
                    checks.push(Check {
                        name: "REST API token".into(),
                        status: CheckStatus::Ok,
                        detail: format!("Valid (user_id={})", creds.user_id),
                    });
                }
                Ok(false) => {
                    checks.push(Check {
                        name: "REST API token".into(),
                        status: CheckStatus::Fail,
                        detail: "Token rejected. Open KakaoTalk, browse chats, then re-login."
                            .into(),
                    });
                }
                Err(e) => {
                    checks.push(Check {
                        name: "REST API token".into(),
                        status: CheckStatus::Fail,
                        detail: format!("Request failed: {}", e),
                    });
                }
            },
            Err(e) => {
                checks.push(Check {
                    name: "REST API token".into(),
                    status: CheckStatus::Fail,
                    detail: format!("Client init failed: {}", e),
                });
            }
        },
        Err(e) => {
            checks.push(Check {
                name: "REST API token".into(),
                status: CheckStatus::Fail,
                detail: format!("No credentials: {}", e),
            });
        }
    }

    // 6. LOCO booking connectivity (optional)
    if test_loco {
        if let Ok(creds) = &creds_result {
            let rt = tokio::runtime::Runtime::new()?;
            let loco_creds = creds.clone();
            match rt.block_on(async {
                let client = loco::client::LocoClient::new(loco_creds);
                client.booking().await
            }) {
                Ok(config) => {
                    let hosts = config
                        .get_document("ticket")
                        .ok()
                        .and_then(|t| t.get_array("lsl").ok())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_else(|| "none".into());
                    let ports = config
                        .get_document("wifi")
                        .ok()
                        .and_then(|w| w.get_array("ports").ok())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_i32())
                                .map(|p| p.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_else(|| "none".into());
                    checks.push(Check {
                        name: "LOCO booking (GETCONF)".into(),
                        status: CheckStatus::Ok,
                        detail: format!("hosts=[{}], ports=[{}]", hosts, ports),
                    });
                }
                Err(e) => {
                    checks.push(Check {
                        name: "LOCO booking (GETCONF)".into(),
                        status: CheckStatus::Fail,
                        detail: format!("Connection failed: {}", e),
                    });
                }
            }
        } else {
            checks.push(Check {
                name: "LOCO booking (GETCONF)".into(),
                status: CheckStatus::Fail,
                detail: "Skipped (no credentials)".into(),
            });
        }
    }

    // 7. Protocol constants
    checks.push(Check {
        name: "Protocol constants".into(),
        status: CheckStatus::Ok,
        detail: format!(
            "handshake_key_type=16, encrypt_type=3 (AES-128-GCM), RSA=2048-bit e=3, booking={}:{}",
            "booking-loco.kakao.com", 443
        ),
    });

    // Output
    if json {
        let items: Vec<serde_json::Value> = checks
            .iter()
            .map(|c| {
                serde_json::json!({
                    "check": c.name,
                    "status": match c.status {
                        CheckStatus::Ok => "ok",
                        CheckStatus::Warn => "warn",
                        CheckStatus::Fail => "fail",
                    },
                    "detail": c.detail,
                })
            })
            .collect();
        let out = serde_json::json!({
            "checks": items,
            "recovery_state": recovery,
            "safety_state": safety,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("openkakao-rs doctor (v{})", VERSION);
        println!();
        for c in &checks {
            let (icon, color_fn): (&str, fn(&str) -> String) = match c.status {
                CheckStatus::Ok => {
                    if color_enabled() {
                        ("OK", |s: &str| format!("{}", s.green()))
                    } else {
                        ("OK", |s: &str| s.to_string())
                    }
                }
                CheckStatus::Warn => {
                    if color_enabled() {
                        ("WARN", |s: &str| format!("{}", s.yellow()))
                    } else {
                        ("WARN", |s: &str| s.to_string())
                    }
                }
                CheckStatus::Fail => {
                    if color_enabled() {
                        ("FAIL", |s: &str| format!("{}", s.red()))
                    } else {
                        ("FAIL", |s: &str| s.to_string())
                    }
                }
            };
            println!("  [{}] {}: {}", color_fn(icon), c.name, c.detail);
        }

        if !test_loco {
            println!();
            println!("  Tip: run with --loco to also test LOCO booking connectivity.");
        }
        println!(
            "  Tip: run 'openkakao-rs auth-status --json' for the raw persisted recovery state."
        );
    }

    Ok(())
}

/// Sanitize a filename to prevent path traversal attacks.
/// Strips directory components and replaces dangerous characters.
fn sanitize_filename(name: &str) -> String {
    // Take only the final path component (strip ../ or /etc)
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");
    // Remove any remaining null bytes or path separators
    let sanitized: String = base
        .chars()
        .filter(|c| *c != '\0' && *c != '/' && *c != '\\')
        .collect();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "download".to_string()
    } else {
        sanitized
    }
}

/// Parse attachment JSON to extract download URL and filename.
/// Returns (url, filename) or None if unparseable.
fn parse_attachment_url(attachment: &str, msg_type: i32) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(attachment).ok()?;

    // Try direct "url" field first
    if let Some(url) = v.get("url").and_then(|u| u.as_str()) {
        if !url.is_empty() {
            let filename = v
                .get("name")
                .and_then(|n| n.as_str())
                .filter(|n| !n.is_empty() && *n != "(Emoticons)")
                .map(String::from)
                .or_else(|| {
                    // Try to extract filename from "k" field
                    v.get("k")
                        .and_then(|k| k.as_str())
                        .and_then(|k| k.rsplit('/').next())
                        .filter(|n| n.contains('.'))
                        .map(String::from)
                })
                .unwrap_or_else(|| {
                    let ext = media_extension(msg_type);
                    format!("media.{}", ext)
                });
            return Some((url.to_string(), filename));
        }
    }

    // Try "k" field (photo/video key): https://dn-m.talk.kakao.com/talkm/{k}
    if let Some(k) = v.get("k").and_then(|k| k.as_str()) {
        if !k.is_empty() {
            let url = format!("https://dn-m.talk.kakao.com/talkm/{}", k);
            // Use the key's last segment as filename base
            let key_name = k.rsplit('/').next().unwrap_or(k);
            let ext = media_extension(msg_type);
            let filename = if key_name.contains('.') {
                key_name.to_string()
            } else {
                format!("{}.{}", key_name, ext)
            };
            return Some((url, filename));
        }
    }

    None
}

fn media_extension(msg_type: i32) -> &'static str {
    match msg_type {
        2 | 27 => "jpg",
        3 => "mp4",
        12 => "m4a",
        14 => "gif",
        26 => "bin",
        _ => "dat",
    }
}

/// Download a media file from KakaoTalk CDN.
fn download_media_file(creds: &KakaoCredentials, url: &str, path: &Path) -> Result<u64> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let a_header = if creds.a_header.is_empty() {
        format!("mac/{}/ko", creds.app_version)
    } else {
        creds.a_header.clone()
    };
    let user_agent = if creds.user_agent.is_empty() {
        format!("KT/{} Mc/10.15.7 ko", creds.app_version)
    } else {
        creds.user_agent.clone()
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    // Validate URL domain before sending credentials
    let parsed_url = reqwest::Url::parse(url)?;
    let host = parsed_url.host_str().unwrap_or("");
    if !host.ends_with(".kakao.com") && !host.ends_with(".kakaocdn.net") {
        anyhow::bail!("Refusing to send credentials to non-Kakao domain: {}", host);
    }

    let mut response = client
        .get(url)
        .header("A", &a_header)
        .header("User-Agent", &user_agent)
        .header(
            "Authorization",
            format!("{}-{}", creds.oauth_token, creds.device_uuid),
        )
        .send()?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP {}: {}", response.status(), url);
    }

    let mut file = std::fs::File::create(path)?;
    let bytes = std::io::copy(&mut response, &mut file)?;
    Ok(bytes)
}

fn cmd_download(chat_id: i64, log_id: i64, output_dir: Option<&str>) -> Result<()> {
    let creds = get_creds()?;
    let out_dir = output_dir.unwrap_or("downloads");

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds.clone());
        eprintln!("Connecting via LOCO...");
        loco_connect_with_auto_refresh(&mut client).await?;

        // Get lastLogId via CHATONROOM (required as max for SYNCMSG)
        let room_info = client
            .send_command("CHATONROOM", bson::doc! { "chatId": chat_id })
            .await?;
        if room_info.status() != 0 {
            anyhow::bail!("CHATONROOM failed (status={})", room_info.status());
        }
        let last_log_id = room_info.body.get_i64("l").unwrap_or(0);

        // Scan via SYNCMSG pagination to find the target message.
        // SYNCMSG scans forward from cur=0 toward max=lastLogId.
        let mut cur = 0_i64;
        let mut target_doc: Option<bson::Document> = None;

        eprintln!("[download] Scanning for logId={}...", log_id);
        loop {
            let response = client
                .send_command(
                    "SYNCMSG",
                    bson::doc! {
                        "chatId": chat_id,
                        "cur": cur,
                        "cnt": 50_i32,
                        "max": last_log_id,
                    },
                )
                .await?;

            if response.status() != 0 {
                anyhow::bail!("SYNCMSG failed (status={})", response.status());
            }

            let chat_logs = response
                .body
                .get_array("chatLogs")
                .map(|a| a.to_vec())
                .unwrap_or_default();

            let is_ok = response.body.get_bool("isOK").unwrap_or(true);

            if chat_logs.is_empty() {
                break;
            }

            let mut max_in_batch = 0_i64;
            for log in &chat_logs {
                if let Some(doc) = log.as_document() {
                    let lid = get_bson_i64(doc, &["logId"]);
                    if lid > max_in_batch {
                        max_in_batch = lid;
                    }
                    if lid == log_id {
                        target_doc = Some(doc.clone());
                    }
                }
            }

            if target_doc.is_some() || is_ok || max_in_batch == 0 {
                break;
            }

            // Skip ahead if we've already passed the target
            if max_in_batch > log_id {
                break;
            }

            cur = max_in_batch;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        let target_log = match &target_doc {
            Some(doc) => doc,
            None => {
                anyhow::bail!("Message logId={} not found in chat {}", log_id, chat_id);
            }
        };

        let msg_type = get_bson_i32(target_log, &["type"]);
        let attachment = get_bson_str(target_log, &["attachment"]);

        if attachment.is_empty() {
            anyhow::bail!("Message logId={} has no attachment", log_id);
        }

        match parse_attachment_url(&attachment, msg_type) {
            Some((url, filename)) => {
                let dir = Path::new(out_dir).join(chat_id.to_string());
                let save_name = format!("{}_{}", log_id, sanitize_filename(&filename));
                let save_path = dir.join(&save_name);

                eprintln!("Downloading: {}", url);
                let bytes = download_media_file(&creds, &url, &save_path)?;
                println!("Saved: {} ({} bytes)", save_path.display(), bytes);
            }
            None => {
                anyhow::bail!(
                    "Cannot parse attachment URL from message logId={}. Raw: {}",
                    log_id,
                    truncate(&attachment, 100)
                );
            }
        }

        Ok(())
    })
}

fn get_creds() -> Result<KakaoCredentials> {
    resolve_base_credentials()
}

fn print_loco_error_hint(status: i64) {
    match status {
        -950 => {
            eprintln!("  Error: Authentication rejected (-950).");
            eprintln!("  Likely causes:");
            eprintln!("    1. Token expired: open KakaoTalk, browse chats, then 'openkakao-rs login --save'.");
            eprintln!("    2. Session conflict: another client may have invalidated this session.");
            eprintln!("  Will attempt auto-refresh if possible.");
        }
        -999 => {
            eprintln!("  Error: Upgrade required (-999).");
            eprintln!(
                "  The app version string is too old. Update KakaoTalk and re-extract credentials."
            );
        }
        -400 => {
            eprintln!("  Error: Bad request (-400). Missing required parameter.");
        }
        -300 => {
            eprintln!("  Error: Unsupported request or device mismatch (-300).");
            eprintln!(
                "  This method/body combination is likely not valid for the macOS LOCO surface."
            );
        }
        -203 => {
            eprintln!("  Error: Missing required parameter (-203).");
            eprintln!(
                "  This LOCO method likely exists, but the required body shape is incomplete."
            );
        }
        -301 => {
            eprintln!("  Error: Account restricted (-301). Your account may be under review.");
            eprintln!("  WARNING: Do not retry aggressively. Wait and check KakaoTalk app.");
        }
        -1 => {
            eprintln!("  Error: Connection failed or no status in response.");
            eprintln!("  Run 'openkakao-rs doctor --loco' to check connectivity.");
        }
        _ => {
            eprintln!(
                "  Unknown LOCO error (status={}). Run 'openkakao-rs doctor' for diagnostics.",
                status
            );
        }
    }
}

fn print_section_title(title: &str) {
    if color_enabled() {
        println!("{}", title.bold().cyan());
    } else {
        println!("{}", title);
    }
}

/// Get i64 from BSON doc, trying multiple field names (abbreviated + full).
fn get_bson_i64(doc: &bson::Document, keys: &[&str]) -> i64 {
    for k in keys {
        if let Ok(v) = doc.get_i64(k) {
            return v;
        }
        if let Ok(v) = doc.get_i32(k) {
            return v as i64;
        }
    }
    0
}

/// Get i32 from BSON doc, trying multiple field names.
fn get_bson_i32(doc: &bson::Document, keys: &[&str]) -> i32 {
    for k in keys {
        if let Ok(v) = doc.get_i32(k) {
            return v;
        }
        if let Ok(v) = doc.get_i64(k) {
            return v as i32;
        }
    }
    0
}

/// Get bool from BSON doc, trying multiple field names.
fn get_bson_bool(doc: &bson::Document, keys: &[&str]) -> bool {
    for k in keys {
        if let Ok(v) = doc.get_bool(k) {
            return v;
        }
    }
    false
}

/// Get string from BSON doc, trying multiple field names.
fn get_bson_str(doc: &bson::Document, keys: &[&str]) -> String {
    for k in keys {
        if let Ok(v) = doc.get_str(k) {
            return v.to_string();
        }
    }
    String::new()
}

/// Get i64 array from BSON doc, trying multiple field names.
fn get_bson_i64_array(doc: &bson::Document, keys: &[&str]) -> Vec<i64> {
    for k in keys {
        if let Ok(arr) = doc.get_array(k) {
            return arr
                .iter()
                .filter_map(|v| v.as_i64().or_else(|| v.as_i32().map(|n| n as i64)))
                .collect();
        }
    }
    Vec::new()
}

/// Get i32 array from BSON doc, trying multiple field names.
fn get_bson_i32_array(doc: &bson::Document, keys: &[&str]) -> Vec<i32> {
    for k in keys {
        if let Ok(arr) = doc.get_array(k) {
            return arr
                .iter()
                .filter_map(|v| v.as_i32().or_else(|| v.as_i64().map(|n| n as i32)))
                .collect();
        }
    }
    Vec::new()
}

/// Get string array from BSON doc, trying multiple field names.
fn get_bson_str_array(doc: &bson::Document, keys: &[&str]) -> Vec<String> {
    for k in keys {
        if let Ok(arr) = doc.get_array(k) {
            return arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
    }
    Vec::new()
}

fn type_label(kind: &str) -> &'static str {
    match kind {
        "DirectChat" => "DM",
        "MultiChat" => "Group",
        "MemoChat" => "Memo",
        "OpenDirectChat" => "OpenDM",
        "OpenMultiChat" => "OpenGroup",
        _ => "Unknown",
    }
}

/// Returns true if this chat type is an open chat (higher ban risk).
fn is_open_chat(chat_type: &str) -> bool {
    matches!(chat_type, "OpenDirectChat" | "OpenMultiChat")
}

/// Get the chat type string from a CHATONROOM response.
fn extract_chat_type(room_info: &bson::Document) -> String {
    room_info
        .get_document("chatInfo")
        .ok()
        .and_then(|ci| ci.get_str("type").ok())
        .or_else(|| room_info.get_str("t").ok())
        .unwrap_or("Unknown")
        .to_string()
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut truncated = s.chars().take(max_chars).collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

/// Parse `--since YYYY-MM-DD` into epoch seconds (start of day in local timezone).
fn parse_since_date(since: Option<&str>) -> Result<Option<i64>> {
    let Some(s) = since else { return Ok(None) };
    let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("Invalid --since date '{}'. Expected YYYY-MM-DD.", s))?;
    let dt = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;
    let local_dt = Local
        .from_local_datetime(&dt)
        .single()
        .ok_or_else(|| anyhow::anyhow!("Ambiguous local time for {}", s))?;
    Ok(Some(local_dt.timestamp()))
}

fn format_time(epoch: i64) -> String {
    if epoch <= 0 {
        return String::new();
    }

    let Some(dt) = Local.timestamp_opt(epoch, 0).single() else {
        return String::new();
    };

    let now = Local::now();
    if dt.date_naive() == now.date_naive() {
        return dt.format("%H:%M").to_string();
    }

    if dt.year() == now.year() {
        return dt.format("%m/%d %H:%M").to_string();
    }

    dt.format("%Y/%m/%d").to_string()
}

fn member_name_map(members: &[ChatMember], my_user_id: i64) -> HashMap<i64, String> {
    let mut out = HashMap::new();
    for m in members {
        out.insert(m.user_id, m.display_name());
    }
    out.insert(my_user_id, "Me".to_string());
    out
}

fn print_table(headers: &[&str], rows: Vec<Vec<String>>) {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in &rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx >= widths.len() {
                widths.push(cell.chars().count());
            } else {
                widths[idx] = widths[idx].max(cell.chars().count());
            }
        }
    }

    if color_enabled() {
        let header_line = headers
            .iter()
            .enumerate()
            .map(|(idx, h)| format!("{:width$}", h.bold(), width = widths[idx]))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{header_line}");
    } else {
        let header_line = headers
            .iter()
            .enumerate()
            .map(|(idx, h)| format!("{:width$}", h, width = widths[idx]))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{header_line}");
    }

    let separator = widths
        .iter()
        .map(|w| "-".repeat(*w))
        .collect::<Vec<_>>()
        .join("  ");
    if color_enabled() {
        println!("{}", separator.dimmed());
    } else {
        println!("{separator}");
    }

    for row in rows {
        let line = row
            .iter()
            .enumerate()
            .map(|(idx, cell)| format!("{:width$}", cell, width = widths[idx]))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outgoing_messages_include_prefix_by_default() {
        assert_eq!(
            format_outgoing_message("hello", false),
            "🤖 [Sent via openkakao] hello"
        );
    }

    #[test]
    fn outgoing_messages_can_disable_prefix() {
        assert_eq!(format_outgoing_message("hello", true), "hello");
    }

    #[test]
    fn send_accepts_global_and_local_flags_after_subcommand() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "--unattended",
            "--allow-non-interactive-send",
            "send",
            "123",
            "hello",
            "--no-prefix",
            "-y",
        ])
        .expect("send should accept global and local flags");

        assert!(cli.no_prefix);
        assert!(cli.unattended);
        assert!(cli.allow_non_interactive_send);
        match cli.command {
            Commands::Send {
                chat_id,
                message,
                yes,
                ..
            } => {
                assert_eq!(chat_id, 123);
                assert_eq!(message, "hello");
                assert!(yes);
            }
            other => panic!("expected send command, got {other:?}"),
        }
    }

    #[test]
    fn unattended_flag_is_available_globally() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "--unattended",
            "--allow-watch-side-effects",
            "watch",
            "--hook-cmd",
            "cat",
        ])
        .expect("global unattended flag should parse");

        assert!(cli.unattended);
        assert!(cli.allow_watch_side_effects);
    }

    #[test]
    fn permission_gate_rejects_missing_opt_in() {
        let err = require_permission(false, "non-interactive send", "set the flags").unwrap_err();
        assert!(
            err.to_string().contains("set the flags"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn watch_hook_filters_match_expected_events() {
        let config = WatchHookConfig {
            command: Some("cat".to_string()),
            webhook_url: None,
            webhook_headers: Vec::new(),
            webhook_signing_secret: None,
            chat_ids: vec![42],
            keywords: vec!["urgent".to_string()],
            message_types: vec![1],
            fail_fast: false,
            min_hook_interval_secs: 2,
            min_webhook_interval_secs: 2,
            hook_timeout_secs: 20,
            webhook_timeout_secs: 10,
        };
        let event = WatchMessageEvent {
            event_type: "message",
            received_at: "2026-03-08T00:00:00Z".to_string(),
            method: "MSG".to_string(),
            chat_id: 42,
            chat_name: "test".to_string(),
            log_id: 7,
            author_id: 9,
            author_nickname: "alice".to_string(),
            message_type: 1,
            message: "urgent: ping me".to_string(),
            attachment: String::new(),
        };

        assert!(watch_hook_matches(&config, &event));

        let wrong_chat = WatchMessageEvent {
            chat_id: 99,
            ..event.clone()
        };
        assert!(!watch_hook_matches(&config, &wrong_chat));

        let wrong_keyword = WatchMessageEvent {
            message: "casual update".to_string(),
            ..event.clone()
        };
        assert!(!watch_hook_matches(&config, &wrong_keyword));
    }

    #[test]
    fn watch_accepts_hook_flags() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "--unattended",
            "--allow-watch-side-effects",
            "watch",
            "--hook-cmd",
            "cat >/tmp/openkakao-hook.json",
            "--webhook-url",
            "https://example.com/openkakao",
            "--webhook-header",
            "Authorization: Bearer token",
            "--webhook-signing-secret",
            "super-secret",
            "--hook-chat-id",
            "123",
            "--hook-keyword",
            "urgent",
            "--hook-type",
            "1",
            "--hook-fail-fast",
        ])
        .expect("watch should accept hook flags");

        assert!(cli.unattended);
        assert!(cli.allow_watch_side_effects);
        match cli.command {
            Commands::Watch {
                hook_cmd,
                webhook_url,
                webhook_header,
                webhook_signing_secret,
                hook_chat_id,
                hook_keyword,
                hook_type,
                hook_fail_fast,
                ..
            } => {
                assert_eq!(hook_cmd.as_deref(), Some("cat >/tmp/openkakao-hook.json"));
                assert_eq!(
                    webhook_url.as_deref(),
                    Some("https://example.com/openkakao")
                );
                assert_eq!(
                    webhook_header,
                    vec!["Authorization: Bearer token".to_string()]
                );
                assert_eq!(webhook_signing_secret.as_deref(), Some("super-secret"));
                assert_eq!(hook_chat_id, vec![123]);
                assert_eq!(hook_keyword, vec!["urgent".to_string()]);
                assert_eq!(hook_type, vec![1]);
                assert!(hook_fail_fast);
            }
            other => panic!("expected watch command, got {other:?}"),
        }
    }

    #[test]
    fn read_accepts_transport_flags() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "read",
            "123",
            "--rest",
            "--delay-ms",
            "250",
            "--force",
        ])
        .expect("read should accept transport flags");

        match cli.command {
            Commands::Read {
                chat_id,
                rest,
                delay_ms,
                force,
                ..
            } => {
                assert_eq!(chat_id, 123);
                assert!(rest);
                assert_eq!(delay_ms, 250);
                assert!(force);
            }
            other => panic!("expected read command, got {other:?}"),
        }
    }

    #[test]
    fn chats_accepts_rest_flag() {
        let cli = Cli::try_parse_from(["openkakao-rs", "chats", "--rest", "--unread"])
            .expect("chats should accept --rest");

        match cli.command {
            Commands::Chats { rest, unread, .. } => {
                assert!(rest);
                assert!(unread);
            }
            other => panic!("expected chats command, got {other:?}"),
        }
    }

    #[test]
    fn members_accepts_rest_flag() {
        let cli = Cli::try_parse_from(["openkakao-rs", "members", "123", "--rest", "--full"])
            .expect("members should accept --rest and --full");

        match cli.command {
            Commands::Members {
                chat_id,
                rest,
                full,
            } => {
                assert_eq!(chat_id, 123);
                assert!(rest);
                assert!(full);
            }
            other => panic!("expected members command, got {other:?}"),
        }
    }

    #[test]
    fn profile_accepts_chat_id_flag() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "profile",
            "100000002",
            "--chat-id",
            "900000000000001",
        ])
        .expect("profile should accept --chat-id");

        match cli.command {
            Commands::Profile {
                user_id,
                chat_id,
                local,
            } => {
                assert_eq!(user_id, 100000002);
                assert_eq!(chat_id, Some(900000000000001));
                assert!(!local);
            }
            other => panic!("expected profile command, got {other:?}"),
        }
    }

    #[test]
    fn friends_accepts_local_flag() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "friends",
            "--local",
            "-s",
            "Alice",
            "--chat-id",
            "900000000000003",
            "--user-id",
            "100000003",
        ])
        .expect("friends should accept --local");

        match cli.command {
            Commands::Friends {
                local,
                search,
                favorites,
                hidden,
                chat_id,
                user_id,
            } => {
                assert!(local);
                assert_eq!(search.as_deref(), Some("Alice"));
                assert!(!favorites);
                assert!(!hidden);
                assert_eq!(chat_id, Some(900000000000003));
                assert_eq!(user_id, Some(100000003));
            }
            other => panic!("expected friends command, got {other:?}"),
        }
    }

    #[test]
    fn profile_accepts_local_flag() {
        let cli = Cli::try_parse_from(["openkakao-rs", "profile", "100000002", "--local"])
            .expect("profile should accept --local");

        match cli.command {
            Commands::Profile {
                user_id,
                chat_id,
                local,
            } => {
                assert_eq!(user_id, 100000002);
                assert_eq!(chat_id, None);
                assert!(local);
            }
            other => panic!("expected profile command, got {other:?}"),
        }
    }

    #[test]
    fn chatinfo_command_is_available() {
        let cli = Cli::try_parse_from(["openkakao-rs", "chatinfo", "123"])
            .expect("chatinfo should be available");

        match cli.command {
            Commands::Chatinfo { chat_id } => assert_eq!(chat_id, 123),
            other => panic!("expected chatinfo command, got {other:?}"),
        }
    }

    #[test]
    fn probe_command_is_available() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "probe",
            "BLSYNC",
            "--body",
            "{\"r\":0,\"pr\":0}",
        ])
        .expect("probe should be available");

        match cli.command {
            Commands::Probe { method, body } => {
                assert_eq!(method, "BLSYNC");
                assert_eq!(body.as_deref(), Some("{\"r\":0,\"pr\":0}"));
            }
            other => panic!("expected probe command, got {other:?}"),
        }
    }

    #[test]
    fn profile_hints_command_is_available() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "profile-hints",
            "--local-graph",
            "--user-id",
            "100000003",
            "--probe-syncmainpf",
            "--probe-uplinkprof",
        ])
        .expect("profile-hints should be available");

        match cli.command {
            Commands::ProfileHints {
                app_state,
                app_state_diff,
                local_graph,
                user_id,
                probe_syncmainpf,
                probe_uplinkprof,
            } => {
                assert!(!app_state);
                assert!(app_state_diff.is_none());
                assert!(local_graph);
                assert_eq!(user_id, Some(100000003));
                assert!(probe_syncmainpf);
                assert!(probe_uplinkprof);
            }
            other => panic!("expected profile-hints command, got {other:?}"),
        }
    }

    #[test]
    fn profile_hints_accepts_app_state_diff() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "profile-hints",
            "--app-state",
            "--app-state-diff",
            "/tmp/profile-hints-before.json",
        ])
        .expect("profile-hints should accept --app-state-diff");

        match cli.command {
            Commands::ProfileHints {
                app_state,
                app_state_diff,
                ..
            } => {
                assert!(app_state);
                assert_eq!(
                    app_state_diff.as_deref(),
                    Some("/tmp/profile-hints-before.json")
                );
            }
            other => panic!("expected profile-hints command, got {other:?}"),
        }
    }

    #[test]
    fn probe_retry_helper_covers_common_socket_failures() {
        assert!(should_retry_loco_probe_error(&anyhow::anyhow!("early eof")));
        assert!(should_retry_loco_probe_error(&anyhow::anyhow!(
            "Connection reset by peer (os error 54)"
        )));
        assert!(should_retry_loco_probe_error(&anyhow::anyhow!(
            "broken pipe"
        )));
        assert!(!should_retry_loco_probe_error(&anyhow::anyhow!(
            "status=-203"
        )));
    }

    #[test]
    fn parse_friend_profile_cache_hint_extracts_ids_and_access_permit() {
        let hint = parse_profile_cache_hint(
            136,
            "https://katalk.kakao.com/mac/profile3/friend.json?accessPermit=example-access-permit-token&chatId=900000000000002&id=100000002",
            true,
        );

        assert_eq!(hint.kind, "friend");
        assert_eq!(hint.user_ids, vec![100000002]);
        assert_eq!(hint.chat_id, Some(900000000000002));
        assert_eq!(
            hint.access_permit.as_deref(),
            Some("example-access-permit-token")
        );
        assert!(hint.data_on_fs);
    }

    #[test]
    fn parse_friends_profile_cache_hint_extracts_ids_list() {
        let hint = parse_profile_cache_hint(
            88,
            "https://katalk.kakao.com/mac/profile3/friends.json?category=action&ids=%5B100000004%2C100000005%2C100000006%5D",
            false,
        );

        assert_eq!(hint.kind, "friends");
        assert_eq!(hint.user_ids, vec![100000004, 100000005, 100000006]);
        assert_eq!(hint.category.as_deref(), Some("action"));
        assert_eq!(hint.chat_id, None);
        assert_eq!(hint.access_permit, None);
    }

    #[test]
    fn parse_loco_member_profile_from_getmem_doc() {
        let doc = bson::doc! {
            "userId": 100000002_i64,
            "accountId": 200000001_i64,
            "nickName": "Alice",
            "countryIso": "kr",
            "statusMessage": "hello",
            "profileImageUrl": "https://example.com/p.jpg",
            "fullProfileImageUrl": "https://example.com/p-full.jpg",
            "originalProfileImageUrl": "https://example.com/p-original.jpg",
            "accessPermit": "permit-token",
            "suspicion": "",
            "suspended": false,
            "memorial": false,
            "type": 0_i32,
            "ut": 100_i64,
        };

        let profile = LocoMemberProfile::from_getmem_doc(&doc);
        assert_eq!(
            profile,
            LocoMemberProfile {
                user_id: 100000002,
                account_id: 200000001,
                nickname: "Alice".into(),
                country_iso: "kr".into(),
                status_message: "hello".into(),
                profile_image_url: "https://example.com/p.jpg".into(),
                full_profile_image_url: "https://example.com/p-full.jpg".into(),
                original_profile_image_url: "https://example.com/p-original.jpg".into(),
                access_permit: "permit-token".into(),
                suspicion: String::new(),
                suspended: false,
                memorial: false,
                member_type: 0,
                ut: 100,
            }
        );
        assert_eq!(profile.as_chat_member().display_name(), "Alice");
    }

    #[test]
    fn legacy_loco_read_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-rs", "loco-read", "123", "--all"])
            .expect("legacy loco-read should remain available");

        match cli.command {
            Commands::LocoRead { chat_id, all, .. } => {
                assert_eq!(chat_id, 123);
                assert!(all);
            }
            other => panic!("expected loco-read command, got {other:?}"),
        }
    }

    #[test]
    fn legacy_loco_chats_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-rs", "loco-chats", "--all"])
            .expect("legacy loco-chats should remain available");

        match cli.command {
            Commands::LocoChats { show_all } => {
                assert!(show_all);
            }
            other => panic!("expected loco-chats command, got {other:?}"),
        }
    }

    #[test]
    fn legacy_loco_members_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-rs", "loco-members", "123"])
            .expect("legacy loco-members should remain available");

        match cli.command {
            Commands::LocoMembers { chat_id } => assert_eq!(chat_id, 123),
            other => panic!("expected loco-members command, got {other:?}"),
        }
    }

    #[test]
    fn legacy_loco_chatinfo_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-rs", "loco-chatinfo", "123"])
            .expect("legacy loco-chatinfo should remain available");

        match cli.command {
            Commands::LocoChatinfo { chat_id } => assert_eq!(chat_id, 123),
            other => panic!("expected loco-chatinfo command, got {other:?}"),
        }
    }

    #[test]
    fn legacy_loco_probe_remains_available() {
        let cli = Cli::try_parse_from(["openkakao-rs", "loco-probe", "BLSYNC"])
            .expect("legacy loco-probe should remain available");

        match cli.command {
            Commands::LocoProbe { method, body } => {
                assert_eq!(method, "BLSYNC");
                assert!(body.is_none());
            }
            other => panic!("expected loco-probe command, got {other:?}"),
        }
    }

    #[test]
    fn webhook_header_requires_name_and_value() {
        assert_eq!(
            parse_webhook_header("Authorization: Bearer test").unwrap(),
            ("Authorization".to_string(), "Bearer test".to_string())
        );
        assert!(parse_webhook_header("MissingSeparator").is_err());
        assert!(parse_webhook_header("Header: ").is_err());
    }

    #[test]
    fn webhook_signature_is_stable_for_known_input() {
        let signature = build_webhook_signature("secret", "1700000000", br#"{"ok":true}"#).unwrap();
        assert_eq!(
            signature,
            "sha256=c1afc7c2df3db0690d7d75954610ed1a1d959ce96355ccb8c0a8bc09fd0cfc27"
        );
    }

    #[test]
    fn webhook_url_requires_https_or_loopback_http() {
        assert!(validate_webhook_url("https://example.com/hook", false).is_ok());
        assert!(validate_webhook_url("http://localhost:3000/hook", false).is_ok());
        assert!(validate_webhook_url("http://127.0.0.1:4000/hook", false).is_ok());
        assert!(validate_webhook_url("http://example.com/hook", false).is_err());
        assert!(validate_webhook_url("http://example.com/hook", true).is_ok());
    }

    #[test]
    fn outbound_message_must_not_be_blank() {
        assert!(validate_outbound_message("hello").is_ok());
        assert!(validate_outbound_message("   ").is_err());
    }
}
