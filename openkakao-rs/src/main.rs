mod auth;
mod auth_flow;
mod commands;
mod config;
mod credentials;
mod error;
mod export;
mod loco;
mod media;
mod message_db;
mod model;
mod rest;
mod state;
mod util;

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::auth::{extract_refresh_token, get_credential_candidates};
use crate::auth_flow::{
    attempt_relogin, attempt_renew, connect_loco_with_reauth, select_best_credential,
    set_auth_policy, AuthPolicy, RecoveryAttempt,
};
use crate::commands::watch::{WebhookFormat, WatchOptions};
use crate::config::load_config;
use crate::credentials::save_credentials;
use crate::export::ExportFormat;
use crate::model::{json_i64, json_string, ChatMember, KakaoCredentials};
use crate::rest::KakaoRestClient;
use crate::state::recovery_snapshot;
use crate::util::{
    color_enabled, confirm, extract_chat_type, format_outgoing_message, format_time, get_bson_bool,
    get_bson_i32, get_bson_i32_array, get_bson_i64, get_bson_i64_array, get_bson_str,
    get_bson_str_array, get_creds, get_rest_client, is_open_chat, member_name_map,
    parse_loco_status_from_error, parse_since_date, print_loco_error_hint,
    print_section_title, print_table, truncate,
    type_label, NO_COLOR, VERSION,
};

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
    candidate_getmem_tokens: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct SyncMainPfCandidate {
    user_id: i64,
    account_id: i64,
    is_self: bool,
    source_entry_ids: Vec<i64>,
    getmem_tokens: Vec<i64>,
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
    /// Show chat statistics (message counts, activity, top participants)
    Stats {
        chat_id: i64,
        #[arg(long, help = "Number of recent messages to analyze (default: all available)")]
        limit: Option<usize>,
        #[arg(long, help = "Only count messages after this date (YYYY-MM-DD)")]
        since: Option<String>,
    },
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
        #[arg(
            long = "webhook-format",
            help = "Webhook payload format: raw (default), slack, discord"
        )]
        webhook_format: Option<String>,
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
    /// Sync messages to local SQLite cache for offline search
    Cache {
        chat_id: i64,
        #[arg(long, help = "Max messages to sync (default: all)")]
        limit: Option<usize>,
    },
    /// Search locally cached messages
    CacheSearch {
        query: String,
        #[arg(long, help = "Limit search to this chat")]
        chat_id: Option<i64>,
        #[arg(short = 'n', long, default_value_t = 30)]
        count: usize,
    },
    /// Show local cache statistics
    CacheStats,
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
        Commands::Stats {
            chat_id,
            limit,
            since,
        } => commands::analytics::cmd_stats(chat_id, limit, since.as_deref(), json)?,
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
            commands::send::cmd_send(
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
        } => commands::send::cmd_send_file(
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
        } => commands::send::cmd_send_file(
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
            webhook_format,
            hook_chat_id,
            hook_keyword,
            hook_type,
            hook_fail_fast,
        } => commands::watch::cmd_watch(WatchOptions {
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
            webhook_format: WebhookFormat::from_str_opt(webhook_format.as_deref())?,
        })?,
        Commands::Download {
            chat_id,
            log_id,
            output_dir,
        } => commands::download::cmd_download(chat_id, log_id, output_dir.as_deref())?,
        Commands::Cache { chat_id, limit } => commands::analytics::cmd_cache(chat_id, limit, json)?,
        Commands::CacheSearch {
            query,
            chat_id,
            count,
        } => commands::analytics::cmd_cache_search(&query, chat_id, count, json)?,
        Commands::CacheStats => commands::analytics::cmd_cache_stats(json)?,
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
        Commands::Doctor { loco } => commands::doctor::cmd_doctor(json, loco, &config)?,
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
    let hint_chat_ids = load_profile_cache_hints(12)
        .ok()
        .map(|hints| collect_hint_chat_ids(&hints, user_id))
        .filter(|ids| !ids.is_empty());
    let snapshot = build_local_friend_graph_for_chat_ids(hint_chat_ids.as_deref())?;
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


/// Detect LOCO message type and extension from magic bytes and file extension.
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
    let mut last_error = None;
    for attempt in 0..3 {
        let response = match client
            .send_command("GETMEM", bson::doc! { "chatId": chat_id })
            .await
        {
            Ok(response) => response,
            Err(error) if should_retry_loco_probe_error(&error) && attempt < 2 => {
                last_error = Some(error);
                reconnect_loco_probe_client(client).await?;
                continue;
            }
            Err(error) => return Err(error),
        };

        if response.status() != 0 {
            anyhow::bail!("GETMEM failed (status={})", response.status());
        }

        let members = response
            .body
            .get_array("members")
            .map(|a| a.to_vec())
            .unwrap_or_default();
        let token = response.body.get_i64("token").ok();

        return Ok(LocoGetMemSnapshot {
            token,
            members: members
                .iter()
                .filter_map(|member| member.as_document().map(LocoMemberProfile::from_getmem_doc))
                .collect::<Vec<_>>(),
        });
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("GETMEM retry loop exhausted")))
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
    allowed_chat_ids: Option<&HashSet<i64>>,
) -> Result<LocalFriendGraphSnapshot> {
    let chats = fetch_loco_chat_listings_with_client(client, login_data, true)
        .await?
        .into_iter()
        .filter(|chat| {
            allowed_chat_ids
                .map(|ids| ids.contains(&chat.chat_id))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
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

fn build_local_friend_graph_for_chat_ids(
    allowed_chat_ids: Option<&[i64]>,
) -> Result<LocalFriendGraphSnapshot> {
    let creds = get_creds()?;
    let self_user_id = creds.user_id;
    let allowed_chat_ids = allowed_chat_ids.map(|ids| ids.iter().copied().collect::<HashSet<_>>());

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = loco_connect_with_auto_refresh(&mut client).await?;
        build_local_friend_graph_with_client(
            &mut client,
            &login_data,
            self_user_id,
            allowed_chat_ids.as_ref(),
        )
        .await
    })
}

fn build_local_friend_graph() -> Result<LocalFriendGraphSnapshot> {
    build_local_friend_graph_for_chat_ids(None)
}

fn collect_hint_chat_ids(cached_requests: &[ProfileCacheHint], user_id: i64) -> Vec<i64> {
    let mut chat_ids = cached_requests
        .iter()
        .filter(|hint| hint.user_ids.contains(&user_id))
        .filter_map(|hint| hint.chat_id)
        .collect::<Vec<_>>();
    chat_ids.sort_unstable();
    chat_ids.dedup();
    chat_ids
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
            let mut candidate_getmem_tokens = Vec::new();
            for entry in &matched {
                for chat_id in &entry.chat_ids {
                    merge_unique_i64(&mut candidate_chat_ids, *chat_id);
                }
                for permit in &entry.access_permits {
                    merge_unique_string(&mut candidate_access_permits, permit);
                }
            }
            for chat in &snapshot.chat_meta {
                if candidate_chat_ids.contains(&chat.chat_id) {
                    if let Some(token) = chat.getmem_token {
                        merge_unique_i64(&mut candidate_getmem_tokens, token);
                    }
                }
            }

            LocalFriendGraphHintMatch {
                entry_id: hint.entry_id,
                kind: hint.kind.clone(),
                requested_user_ids: hint.user_ids.clone(),
                matched_user_ids: matched.iter().map(|entry| entry.user_id).collect(),
                candidate_chat_ids,
                candidate_access_permits,
                candidate_getmem_tokens,
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
    let getmem_tokens = snapshot
        .chat_meta
        .iter()
        .filter(|chat| entry.chat_ids.contains(&chat.chat_id))
        .filter_map(|chat| chat.getmem_token)
        .collect::<Vec<_>>();

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

    for token in &getmem_tokens {
        for chat_id in &chat_ids {
            for access_permit in &access_permits {
                for ct in ["d", "p"] {
                    let mut token_body = serde_json::Map::new();
                    token_body.insert("ct".into(), serde_json::json!(ct));
                    token_body.insert("token".into(), serde_json::json!(token));
                    if let Some(chat_id) = chat_id {
                        token_body.insert("chatId".into(), serde_json::json!(chat_id));
                    }
                    if let Some(access_permit) = access_permit {
                        token_body.insert("accessPermit".into(), serde_json::json!(access_permit));
                    }
                    push_unique_candidate_body(
                        &mut bodies,
                        &mut seen,
                        serde_json::Value::Object(token_body),
                    );

                    let mut profile_token_body = serde_json::Map::new();
                    profile_token_body.insert("ct".into(), serde_json::json!(ct));
                    profile_token_body.insert("profileToken".into(), serde_json::json!(token));
                    if let Some(chat_id) = chat_id {
                        profile_token_body.insert("chatId".into(), serde_json::json!(chat_id));
                    }
                    if let Some(access_permit) = access_permit {
                        profile_token_body
                            .insert("accessPermit".into(), serde_json::json!(access_permit));
                    }
                    push_unique_candidate_body(
                        &mut bodies,
                        &mut seen,
                        serde_json::Value::Object(profile_token_body),
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

    for token in &getmem_tokens {
        push_unique_candidate_body(
            &mut uplinkprof_bodies,
            &mut uplink_seen,
            serde_json::json!({ "token": token }),
        );
        push_unique_candidate_body(
            &mut uplinkprof_bodies,
            &mut uplink_seen,
            serde_json::json!({ "profileToken": token }),
        );
        for access_permit in access_permits.iter().flatten() {
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "token": token, "F": access_permit }),
            );
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "profileToken": token, "F": access_permit }),
            );
        }
    }

    Some(SyncMainPfCandidate {
        user_id: entry.user_id,
        account_id: entry.account_id,
        is_self: entry.is_self,
        source_entry_ids,
        getmem_tokens,
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
        let targeted_chat_ids = user_id
            .map(|user_id| collect_hint_chat_ids(&cached_requests, user_id))
            .filter(|ids| !ids.is_empty());
        Some(build_local_friend_graph_for_chat_ids(
            targeted_chat_ids.as_deref(),
        )?)
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
        if !candidate.getmem_tokens.is_empty() {
            println!(
                "  syncmainpf_getmem_tokens: {}",
                candidate
                    .getmem_tokens
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
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
                            "{} chat(s), {} permit(s), {} token(s)",
                            matched.candidate_chat_ids.len(),
                            matched.candidate_access_permits.len(),
                            matched.candidate_getmem_tokens.len()
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
        if !candidate.getmem_tokens.is_empty() {
            println!(
                "  getmem_tokens: {}",
                candidate
                    .getmem_tokens
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
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








#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::watch::{
        build_webhook_signature, parse_webhook_header, validate_webhook_url, watch_hook_matches,
        WatchHookConfig, WatchMessageEvent,
    };
    use crate::util::{require_permission, validate_outbound_message};

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
            webhook_format: WebhookFormat::Raw,
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
    fn collect_hint_chat_ids_prefers_user_specific_chat_hints() {
        let hints = vec![
            ProfileCacheHint {
                entry_id: 1,
                kind: "friend".into(),
                request_key: String::new(),
                user_ids: vec![100000002],
                chat_id: Some(900000000000002),
                access_permit: Some("permit-a".into()),
                category: None,
                data_on_fs: true,
            },
            ProfileCacheHint {
                entry_id: 2,
                kind: "friend".into(),
                request_key: String::new(),
                user_ids: vec![100000002],
                chat_id: Some(900000000000002),
                access_permit: Some("permit-b".into()),
                category: None,
                data_on_fs: true,
            },
            ProfileCacheHint {
                entry_id: 3,
                kind: "friend".into(),
                request_key: String::new(),
                user_ids: vec![100000003],
                chat_id: Some(900000000000003),
                access_permit: Some("permit-c".into()),
                category: None,
                data_on_fs: true,
            },
        ];

        assert_eq!(
            collect_hint_chat_ids(&hints, 100000002),
            vec![900000000000002]
        );
        assert_eq!(
            collect_hint_chat_ids(&hints, 100000003),
            vec![900000000000003]
        );
        assert!(collect_hint_chat_ids(&hints, 999).is_empty());
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
    fn local_graph_summary_carries_getmem_tokens() {
        let snapshot = LocalFriendGraphSnapshot {
            user_count: 1,
            chat_count: 1,
            failed_chat_ids: Vec::new(),
            chat_meta: vec![LocalFriendGraphChatMeta {
                chat_id: 900000000000002,
                title: "Example".into(),
                getmem_token: Some(777),
                member_count: 2,
            }],
            entries: vec![LocalFriendGraphEntry {
                user_id: 100000002,
                account_id: 200000001,
                nickname: "Alice".into(),
                country_iso: "KR".into(),
                status_message: String::new(),
                profile_image_url: String::new(),
                full_profile_image_url: String::new(),
                original_profile_image_url: String::new(),
                access_permits: vec!["permit-token".into()],
                suspicion: String::new(),
                suspended: false,
                memorial: false,
                member_type: 0,
                chat_ids: vec![900000000000002],
                chat_titles: vec!["Example".into()],
                is_self: false,
                hidden_like: false,
                hidden_block_type: None,
            }],
        };
        let hints = vec![ProfileCacheHint {
            entry_id: 1,
            kind: "friend".into(),
            request_key: String::new(),
            user_ids: vec![100000002],
            chat_id: Some(900000000000002),
            access_permit: Some("permit-token".into()),
            category: None,
            data_on_fs: true,
        }];

        let summary = local_graph_hint_summary(&snapshot, &hints);
        assert_eq!(summary.candidate_matches.len(), 1);
        assert_eq!(
            summary.candidate_matches[0].candidate_getmem_tokens,
            vec![777]
        );
    }

    #[test]
    fn syncmainpf_candidates_include_getmem_token_fields() {
        let snapshot = LocalFriendGraphSnapshot {
            user_count: 1,
            chat_count: 1,
            failed_chat_ids: Vec::new(),
            chat_meta: vec![LocalFriendGraphChatMeta {
                chat_id: 900000000000002,
                title: "Example".into(),
                getmem_token: Some(777),
                member_count: 2,
            }],
            entries: vec![LocalFriendGraphEntry {
                user_id: 100000002,
                account_id: 200000001,
                nickname: "Alice".into(),
                country_iso: "KR".into(),
                status_message: String::new(),
                profile_image_url: String::new(),
                full_profile_image_url: String::new(),
                original_profile_image_url: String::new(),
                access_permits: vec!["permit-token".into()],
                suspicion: String::new(),
                suspended: false,
                memorial: false,
                member_type: 0,
                chat_ids: vec![900000000000002],
                chat_titles: vec!["Example".into()],
                is_self: false,
                hidden_like: false,
                hidden_block_type: None,
            }],
        };

        let candidate = build_syncmainpf_candidate(&snapshot, &[], 100000002)
            .expect("candidate should be built");

        assert_eq!(candidate.getmem_tokens, vec![777]);
        assert!(candidate
            .bodies
            .iter()
            .any(|body| body.get("token").and_then(|v| v.as_i64()) == Some(777)));
        assert!(candidate
            .bodies
            .iter()
            .any(|body| body.get("profileToken").and_then(|v| v.as_i64()) == Some(777)));
        assert!(candidate
            .uplinkprof_bodies
            .iter()
            .any(|body| body.get("token").and_then(|v| v.as_i64()) == Some(777)));
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

    #[test]
    fn stats_command_is_available() {
        let cli = Cli::try_parse_from([
            "openkakao-rs",
            "stats",
            "123",
            "--limit",
            "500",
            "--since",
            "2025-01-01",
        ])
        .expect("stats should accept limit and since");

        match cli.command {
            Commands::Stats {
                chat_id,
                limit,
                since,
            } => {
                assert_eq!(chat_id, 123);
                assert_eq!(limit, Some(500));
                assert_eq!(since.as_deref(), Some("2025-01-01"));
            }
            other => panic!("expected stats command, got {other:?}"),
        }
    }
}
