mod auth;
mod auth_flow;
mod commands;
mod config;
mod credentials;
mod error;
mod export;
mod loco;
mod loco_helpers;
mod media;
mod message_db;
mod model;
mod rest;
mod state;
mod util;

use std::io;
use std::sync::atomic::Ordering;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

use crate::auth_flow::{set_auth_policy, AuthPolicy};
use crate::commands::read::ReadCommandOptions;
use crate::commands::watch::{WatchOptions, WebhookFormat};
use crate::config::load_config;
use crate::util::{format_outgoing_message, NO_COLOR, VERSION};

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
        #[arg(
            long,
            help = "Number of recent messages to analyze (default: all available)"
        )]
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
        Commands::Auth => commands::auth::cmd_auth(json)?,
        Commands::AuthStatus => commands::auth::cmd_auth_status(json)?,
        Commands::Login { save } => commands::auth::cmd_login(save)?,
        Commands::Me => commands::rest::cmd_me(json)?,
        Commands::Friends {
            favorites,
            hidden,
            search,
            local,
            chat_id,
            user_id,
        } => commands::rest::cmd_friends(favorites, hidden, search, local, chat_id, user_id, json)?,
        Commands::Chats {
            show_all,
            unread,
            search,
            chat_type,
            rest,
        } => commands::chats::cmd_chats(show_all, unread, search, chat_type, rest, json)?,
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
        } => commands::read::cmd_read(
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
        } => commands::members::cmd_members(chat_id, rest, full, json)?,
        Commands::Chatinfo { chat_id } => commands::rest::cmd_chatinfo(chat_id, json)?,
        Commands::Settings => commands::rest::cmd_settings(json)?,
        Commands::Scrap { url } => commands::rest::cmd_scrap(&url, json)?,
        Commands::Profile {
            user_id,
            chat_id,
            local,
        } => commands::profile::cmd_profile(user_id, chat_id, local, json)?,
        Commands::Favorite { user_id } => commands::rest::cmd_favorite(user_id)?,
        Commands::Unfavorite { user_id } => commands::rest::cmd_unfavorite(user_id)?,
        Commands::Hide { user_id } => commands::rest::cmd_hide(user_id)?,
        Commands::Unhide { user_id } => commands::rest::cmd_unhide(user_id)?,
        Commands::Profiles => commands::rest::cmd_profiles(json)?,
        Commands::Keywords => commands::rest::cmd_keywords(json)?,
        Commands::Unread => commands::rest::cmd_unread(json)?,
        Commands::Export {
            chat_id,
            format,
            output,
        } => commands::rest::cmd_export(chat_id, &format, output.as_deref())?,
        Commands::Search { chat_id, query } => commands::rest::cmd_search(chat_id, &query, json)?,
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
        Commands::Renew => commands::auth::cmd_renew(json)?,
        Commands::Relogin {
            fresh_xvc,
            password,
            email,
        } => commands::auth::cmd_relogin(json, fresh_xvc, password, email)?,
        Commands::LocoTest => {
            eprintln!("[deprecated] 'loco-test' is now hidden. Prefer 'doctor --loco'.");
            commands::auth::cmd_loco_test()?
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
            commands::chats::cmd_loco_chats(show_all, false, None, None, json)?
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
            commands::read::cmd_loco_read(
                chat_id,
                &commands::read::ReadCommandOptions {
                    count: count as usize,
                    cursor,
                    since,
                    all,
                    delay_ms,
                    force,
                    rest: false,
                    json,
                },
            )?
        }
        Commands::LocoMembers { chat_id } => {
            eprintln!(
                "[deprecated] 'loco-members' is now hidden. Prefer 'members' (LOCO by default)."
            );
            commands::members::cmd_loco_members(chat_id, false, json)?
        }
        Commands::LocoChatinfo { chat_id } => {
            eprintln!("[deprecated] 'loco-chatinfo' is now hidden. Prefer 'chatinfo'.");
            commands::probe::cmd_loco_chatinfo(chat_id, json)?
        }
        Commands::LocoBlocked => commands::members::cmd_loco_blocked(json)?,
        Commands::Probe { method, body } => {
            commands::probe::cmd_loco_probe(&method, body.as_deref(), json)?
        }
        Commands::ProfileHints {
            app_state,
            app_state_diff,
            local_graph,
            user_id,
            probe_syncmainpf,
            probe_uplinkprof,
        } => commands::profile::cmd_profile_hints(
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
            commands::probe::cmd_loco_probe(&method, body.as_deref(), json)?
        }
        Commands::WatchCache { interval } => commands::auth::cmd_watch_cache(interval)?,
        Commands::Doctor { loco } => commands::doctor::cmd_doctor(json, loco, &config)?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::members::LocoMemberProfile;
    use crate::commands::profile::{
        build_syncmainpf_candidate, collect_hint_chat_ids, local_graph_hint_summary,
        parse_profile_cache_hint, LocalFriendGraphChatMeta, LocalFriendGraphEntry,
        LocalFriendGraphSnapshot, ProfileCacheHint,
    };
    use crate::commands::watch::{
        build_webhook_signature, parse_webhook_header, validate_webhook_url, watch_hook_matches,
        WatchHookConfig, WatchMessageEvent,
    };
    use crate::loco_helpers::should_retry_loco_probe_error;
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
