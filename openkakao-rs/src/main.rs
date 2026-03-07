mod auth;
mod credentials;
mod error;
mod export;
mod loco;
mod model;
mod rest;

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use chrono::{Datelike, Local, TimeZone};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::auth::{
    extract_login_params, extract_refresh_token, get_credential_candidates,
    get_credentials_interactive,
};
use crate::credentials::{load_credentials, save_credentials};
use crate::export::ExportFormat;
use crate::model::{json_i64, json_string, ChatMember, KakaoCredentials};
use crate::rest::KakaoRestClient;

static NO_COLOR: AtomicBool = AtomicBool::new(false);

fn color_enabled() -> bool {
    !NO_COLOR.load(Ordering::Relaxed)
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(name = "openkakao-rs")]
#[command(about = "OpenKakao Rust CLI", long_about = None)]
#[command(version = VERSION)]
struct Cli {
    #[arg(long, global = true, help = "Output as JSON")]
    json: bool,
    #[arg(long, global = true, help = "Disable colored output")]
    no_color: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Verify token validity
    Auth,
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
    },
    /// List members of a chat room
    Members { chat_id: i64 },
    /// Show account settings
    Settings,
    /// Get link preview (OG tags) for a URL
    Scrap { url: String },
    /// Show a friend's profile
    Profile { user_id: i64 },
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
    /// Test LOCO protocol connection (booking -> checkin -> login)
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
    /// List chat rooms via LOCO protocol (full history access)
    LocoChats {
        #[arg(short = 'a', long = "all")]
        show_all: bool,
    },
    /// Read messages via LOCO protocol (full history access)
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
    /// List members of a chat room via LOCO protocol
    LocoMembers { chat_id: i64 },
    /// Get chat room info via LOCO protocol
    LocoChatinfo { chat_id: i64 },
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
    let json = cli.json;

    // Respect NO_COLOR env var (https://no-color.org/) and --no-color flag
    if cli.no_color || std::env::var("NO_COLOR").is_ok() || json {
        NO_COLOR.store(true, Ordering::Relaxed);
    }

    match cli.command {
        Commands::Auth => cmd_auth(json)?,
        Commands::Login { save } => cmd_login(save)?,
        Commands::Me => cmd_me(json)?,
        Commands::Friends {
            favorites,
            hidden,
            search,
        } => cmd_friends(favorites, hidden, search, json)?,
        Commands::Chats {
            show_all,
            unread,
            search,
            chat_type,
        } => cmd_chats(show_all, unread, search, chat_type, json)?,
        Commands::Read {
            chat_id,
            count,
            before,
            cursor,
            since,
            all,
        } => cmd_read(
            chat_id,
            count,
            cursor.or(before),
            since.as_deref(),
            all,
            json,
        )?,
        Commands::Members { chat_id } => cmd_members(chat_id, json)?,
        Commands::Settings => cmd_settings(json)?,
        Commands::Scrap { url } => cmd_scrap(&url, json)?,
        Commands::Profile { user_id } => cmd_profile(user_id, json)?,
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
        Commands::LocoTest => cmd_loco_test()?,
        Commands::Send {
            chat_id,
            message,
            force,
            yes,
        } => cmd_send(chat_id, &message, force, yes)?,
        Commands::SendPhoto {
            chat_id,
            file,
            force,
            yes,
        } => cmd_send_file(chat_id, &file, force, yes)?,
        Commands::SendFile {
            chat_id,
            file,
            force,
            yes,
        } => cmd_send_file(chat_id, &file, force, yes)?,
        Commands::Watch {
            chat_id,
            raw,
            read_receipt,
            max_reconnect,
            download_media,
            download_dir,
        } => cmd_watch(
            chat_id,
            raw,
            read_receipt,
            max_reconnect,
            download_media,
            &download_dir,
        )?,
        Commands::Download {
            chat_id,
            log_id,
            output_dir,
        } => cmd_download(chat_id, log_id, output_dir.as_deref())?,
        Commands::LocoChats { show_all } => cmd_loco_chats(show_all, json)?,
        Commands::LocoRead {
            chat_id,
            count,
            cursor,
            since,
            all,
            delay_ms,
            force,
        } => cmd_loco_read(
            chat_id,
            count,
            cursor,
            since.as_deref(),
            all,
            delay_ms,
            force,
            json,
        )?,
        Commands::LocoMembers { chat_id } => cmd_loco_members(chat_id, json)?,
        Commands::LocoChatinfo { chat_id } => cmd_loco_chatinfo(chat_id, json)?,
        Commands::WatchCache { interval } => cmd_watch_cache(interval)?,
        Commands::Doctor { loco } => cmd_doctor(json, loco)?,
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
            "token_prefix": creds.oauth_token.chars().take(40).collect::<String>(),
            "app_version": creds.app_version,
            "valid": valid,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("  User ID: {}", creds.user_id);
    println!(
        "  Token:   {}...",
        creds.oauth_token.chars().take(40).collect::<String>()
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
        creds.oauth_token.chars().take(40).collect::<String>()
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
    let client = get_rest_client()?;
    let profile = client.get_my_profile()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
        return Ok(());
    }

    print_section_title("My Profile");
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
}

fn cmd_friends(favorites: bool, hidden: bool, search: Option<String>, json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let mut friends = client.get_friends()?;

    if favorites {
        friends.retain(|f| f.favorite);
    }

    if !hidden {
        friends.retain(|f| !f.hidden);
    }

    if let Some(query) = search {
        let q = query.to_lowercase();
        friends.retain(|f| {
            f.display_name().to_lowercase().contains(&q)
                || f.phone_number.to_lowercase().contains(&q)
        });
    }

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

fn cmd_chats(
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

    if json {
        println!("{}", serde_json::to_string_pretty(&chats)?);
        return Ok(());
    }

    let mut rows = Vec::new();
    for c in chats {
        let kind = type_label(&c.kind);
        let unread_str = if c.unread_count > 0 {
            c.unread_count.to_string()
        } else {
            String::new()
        };

        rows.push(vec![
            kind.to_string(),
            c.display_title(),
            unread_str,
            c.chat_id.to_string(),
        ]);
    }

    print_section_title(&format!("Chats ({})", rows.len()));
    print_table(&["Type", "Name", "Unread", "Chat ID"], rows);
    Ok(())
}

fn cmd_read(
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

fn cmd_members(chat_id: i64, json: bool) -> Result<()> {
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

fn cmd_profile(user_id: i64, json: bool) -> Result<()> {
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
    eprintln!("Extracting refresh_token from Cache.db...");

    let refresh_token = match extract_refresh_token()? {
        Some(t) => {
            eprintln!(
                "  Found refresh_token: {}...",
                t.chars().take(40).collect::<String>()
            );
            t
        }
        None => {
            eprintln!("  No refresh_token found in Cache.db.");
            eprintln!("  Hint: Open KakaoTalk app and wait for it to auto-renew, then retry.");
            return Ok(());
        }
    };

    let client = KakaoRestClient::new(creds)?;

    // Try oauth2_token.json first (node-kakao style: sends both access_token + refresh_token)
    eprintln!("Trying oauth2_token.json (access_token + refresh_token)...");
    let response = client.oauth2_token(&refresh_token)?;
    let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);

    if status == 0 {
        return print_renew_result(json, &response);
    }
    eprintln!("  oauth2_token.json → status={}", status);

    // Fallback: try legacy renew_token.json
    eprintln!("Trying renew_token.json (refresh_token only)...");
    let response = client.renew_token(&refresh_token)?;
    let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);

    if status == 0 {
        return print_renew_result(json, &response);
    }
    eprintln!("  renew_token.json → status={}", status);

    if json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        eprintln!("Token renewal failed (both endpoints).");
        eprintln!("Response: {}", serde_json::to_string_pretty(&response)?);
    }

    Ok(())
}

fn print_renew_result(json: bool, response: &Value) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(response)?);
    } else {
        eprintln!("  Token renewed successfully!");
        if let Some(access) = response.get("access_token").and_then(Value::as_str) {
            println!("New access_token: {}...", &access[..40.min(access.len())]);
        }
        if let Some(refresh) = response.get("refresh_token").and_then(Value::as_str) {
            println!(
                "New refresh_token: {}...",
                &refresh[..40.min(refresh.len())]
            );
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

    let params = match extract_login_params()? {
        Some(p) => {
            eprintln!("  Email: {}", p.email);
            eprintln!("  Device: {}", p.device_name);
            if password_override.is_some() {
                eprintln!("  Password: (using --password override)");
            } else {
                eprintln!(
                    "  Password: {}... (cached, may be expired)",
                    p.password.chars().take(10).collect::<String>()
                );
            }
            p
        }
        None => {
            eprintln!("  No login.json parameters found in Cache.db.");
            return Ok(());
        }
    };

    let password = password_override.as_deref().unwrap_or(&params.password);
    let email = email_override.as_deref().unwrap_or(&params.email);
    let client = KakaoRestClient::new(creds.clone())?;

    let response = if fresh_xvc {
        eprintln!("Logging in with generated X-VC...");
        client.login_with_xvc(email, password, &params.device_uuid, &params.device_name)?
    } else {
        eprintln!("Calling login.json with cached X-VC...");
        client.login_direct(
            email,
            password,
            &params.device_uuid,
            &params.device_name,
            &params.x_vc,
        )?
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }

    let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);
    eprintln!("  Status: {}", status);

    if status == 0 {
        if let Some(access) = response.get("access_token").and_then(Value::as_str) {
            eprintln!(
                "  access_token: {}...",
                access.chars().take(40).collect::<String>()
            );

            // Auto-save the fresh token
            let mut new_creds = creds.clone();
            new_creds.oauth_token = access.to_string();
            if let Some(user_id) = response.get("userId").and_then(Value::as_i64) {
                new_creds.user_id = user_id;
            }
            if let Some(refresh) = response.get("refresh_token").and_then(Value::as_str) {
                new_creds.refresh_token = Some(refresh.to_string());
                eprintln!(
                    "  refresh_token: {}...",
                    refresh.chars().take(40).collect::<String>()
                );
            }
            save_credentials(&new_creds)?;
            eprintln!("  Credentials saved.");
        }
    } else {
        let msg = response
            .get("message")
            .or_else(|| response.get("msg"))
            .and_then(Value::as_str)
            .unwrap_or("");
        eprintln!("  Login failed: {} (status={})", msg, status);
    }

    Ok(())
}

fn cmd_loco_test() -> Result<()> {
    let creds = get_creds()?;

    eprintln!("Testing LOCO connection for user {}...", creds.user_id);
    eprintln!(
        "  Token: {}...",
        creds.oauth_token.chars().take(40).collect::<String>()
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

fn cmd_send(chat_id: i64, message: &str, force: bool, skip_confirm: bool) -> Result<()> {
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

fn cmd_send_file(chat_id: i64, file_path: &str, force: bool, skip_confirm: bool) -> Result<()> {
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
    let login_data = client.full_connect_with_retry(3).await?;
    let status = login_data
        .get_i64("status")
        .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
        .unwrap_or(-1);

    if status == 0 {
        return Ok(login_data);
    }

    // On -950, attempt auto-refresh
    if status == -950 {
        print_loco_error_hint(status);
        match attempt_token_refresh_and_reconnect(client).await {
            Ok(data) => return Ok(data),
            Err(e) => {
                eprintln!("[token] Auto-refresh failed: {}", e);
                anyhow::bail!(
                    "LOCO login failed (status={}) and auto-refresh failed",
                    status
                );
            }
        }
    }

    print_loco_error_hint(status);
    anyhow::bail!("LOCO login failed (status={})", status)
}

/// Attempt token refresh and LOCO reconnect after -950 auth failure.
/// Returns the new login_data on success.
async fn attempt_token_refresh_and_reconnect(
    client: &mut loco::client::LocoClient,
) -> Result<bson::Document> {
    eprintln!("[token] Attempting token refresh after auth failure...");

    // Try login.json + X-VC to get fresh token
    let params = extract_login_params()?
        .ok_or_else(|| anyhow::anyhow!("No login params available for token refresh"))?;

    let rest = KakaoRestClient::new(client.credentials.clone())?;
    let resp = rest.login_with_xvc(
        &params.email,
        &params.password,
        &params.device_uuid,
        &params.device_name,
    )?;

    let status = resp.get("status").and_then(Value::as_i64).unwrap_or(-1);
    if status != 0 {
        anyhow::bail!("Token refresh failed (login.json status={})", status);
    }

    let new_token = resp
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("No access_token in refresh response"))?;

    eprintln!(
        "[token] Got fresh token: {}...",
        new_token.chars().take(20).collect::<String>()
    );

    // Update client and persist
    client.update_token(new_token.to_string());
    let mut updated_creds = client.credentials.clone();
    if let Some(uid) = resp.get("userId").and_then(Value::as_i64) {
        updated_creds.user_id = uid;
        client.credentials.user_id = uid;
    }
    save_credentials(&updated_creds)?;

    // Reconnect with new token
    let login_data = client.full_connect_with_retry(3).await?;
    let new_status = login_data
        .get_i64("status")
        .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
        .unwrap_or(-1);

    if new_status != 0 {
        anyhow::bail!(
            "LOCO login still fails after token refresh (status={})",
            new_status
        );
    }

    eprintln!("[token] Reconnected successfully with fresh token.");
    Ok(login_data)
}

fn cmd_watch(
    filter_chat_id: Option<i64>,
    raw: bool,
    read_receipt: bool,
    max_reconnect: u32,
    download_media: bool,
    download_dir: &str,
) -> Result<()> {
    let creds = get_creds()?;

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
                    if err_msg.contains("-950") || err_msg.contains("-999") {
                        eprintln!("[watch] Auth error, cannot reconnect: {}", e);
                        return Err(e);
                    }
                    if max_reconnect == 0 || reconnect_count >= max_reconnect {
                        return Err(e);
                    }
                    reconnect_count += 1;
                    let delay = std::cmp::min(2u64.pow(reconnect_count), 32);
                    eprintln!(
                        "[watch] Connect failed: {}. Reconnecting in {}s ({}/{})...",
                        e, delay, reconnect_count, max_reconnect
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
                if let Some(cid) = filter_chat_id {
                    eprintln!("[watch] Filtering chat_id={}", cid);
                }
                eprintln!("[watch] Press Ctrl-C to stop.");
            }
            // Reset reconnect count on successful connection
            reconnect_count = 0;

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

                                if raw {
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

                                        if let Some(filter) = filter_chat_id {
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

                                        let content = match msg_type {
                                            1 => packet.body
                                                .get_str("msg")
                                                .unwrap_or("")
                                                .to_string(),
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
                                            _ => packet.body
                                                .get_str("msg")
                                                .map(String::from)
                                                .unwrap_or_else(|_| format!("[type={}]", msg_type)),
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

                                        // Send read receipt if enabled
                                        if read_receipt {
                                            let log_id = packet.body
                                                .get_i64("logId")
                                                .or_else(|_| packet.body.get_i32("logId").map(|v| v as i64))
                                                .unwrap_or(0);
                                            if log_id > 0 {
                                                let _ = client.send_packet("NOTIREAD", bson::doc! {
                                                    "chatId": chat_id,
                                                    "watermark": log_id,
                                                }).await;
                                            }
                                        }

                                        // Auto-download media if enabled
                                        if download_media && matches!(msg_type, 2 | 3 | 12 | 14 | 26 | 27) {
                                            let attachment = packet.body
                                                .get_str("attachment")
                                                .unwrap_or("")
                                                .to_string();
                                            if !attachment.is_empty() {
                                                let dl_creds = client.credentials.clone();
                                                let dl_dir = download_dir.to_string();
                                                let log_id = packet.body
                                                    .get_i64("logId")
                                                    .or_else(|_| packet.body.get_i32("logId").map(|v| v as i64))
                                                    .unwrap_or(0);
                                                tokio::task::spawn_blocking(move || {
                                                    if let Some((url, filename)) = parse_attachment_url(&attachment, msg_type) {
                                                        let dir = Path::new(&dl_dir).join(chat_id.to_string());
                                                        let save_name = format!("{}_{}", log_id, filename);
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
                                    eprintln!("[watch] Auth error: {}", e);
                                    return Err(e);
                                }
                                if max_reconnect == 0 {
                                    eprintln!("[watch] Connection lost: {}", e);
                                    return Err(e);
                                }
                                reconnect_count += 1;
                                if reconnect_count > max_reconnect {
                                    eprintln!(
                                        "[watch] Connection lost after {} reconnect attempts: {}",
                                        max_reconnect, e
                                    );
                                    return Err(e);
                                }
                                let delay = std::cmp::min(2u64.pow(reconnect_count), 32);
                                eprintln!(
                                    "[watch] Connection lost: {}. Reconnecting in {}s ({}/{})...",
                                    e, delay, reconnect_count, max_reconnect
                                );
                                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                                client.disconnect();
                                continue 'reconnect;
                            }
                        }
                    }
                    _ = ping_interval.tick() => {
                        if let Err(e) = client.send_packet("PING", bson::doc! {}).await {
                            eprintln!("[watch] PING failed: {}", e);
                            if max_reconnect == 0 {
                                return Err(anyhow::anyhow!("PING failed: {}", e));
                            }
                            reconnect_count += 1;
                            if reconnect_count > max_reconnect {
                                return Err(anyhow::anyhow!(
                                    "PING failed after {} reconnects: {}", max_reconnect, e
                                ));
                            }
                            let delay = std::cmp::min(2u64.pow(reconnect_count), 32);
                            eprintln!(
                                "[watch] Reconnecting in {}s ({}/{})...",
                                delay, reconnect_count, max_reconnect
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

fn cmd_loco_chats(show_all: bool, json: bool) -> Result<()> {
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = loco_connect_with_auto_refresh(&mut client).await?;

        // Also fetch LCHATLIST for additional data
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

        // Merge: use LOGINLIST chatDatas as primary, LCHATLIST as supplement
        let chat_datas = if lchat_status == 0 {
            response.body.get_array("chatDatas").ok()
        } else {
            None
        };
        let chat_datas = chat_datas
            .or_else(|| login_data.get_array("chatDatas").ok());

        let Some(chat_datas) = chat_datas else {
            println!("No chat data available.");
            return Ok(());
        };

        if json {
            let mut chats = Vec::new();
            for cd in chat_datas {
                if let Some(doc) = cd.as_document() {
                    let chat_id = get_bson_i64(doc, &["c", "chatId"]);
                    let chat_type = get_bson_str(doc, &["t", "type"]);
                    let last_log_id = get_bson_i64(doc, &["s", "lastLogId"]);
                    let last_seen = get_bson_i64(doc, &["ll", "lastSeenLogId"]);
                    let unread = if last_log_id > last_seen { last_log_id - last_seen } else { 0 };
                    let active_member_count = get_bson_i32(doc, &["a", "activeMembersCount"]);

                    // Name: from chatInfo or from "k" (member names joined)
                    let name = doc.get_document("chatInfo").ok()
                        .and_then(|ci| ci.get_str("name").ok())
                        .map(String::from)
                        .unwrap_or_default();
                    let name = if name.is_empty() {
                        get_bson_str_array(doc, &["k"]).join(", ")
                    } else {
                        name
                    };

                    let mut chat = serde_json::json!({
                        "chat_id": chat_id,
                        "type": chat_type,
                        "name": name,
                        "active_members": active_member_count,
                        "last_log_id": last_log_id,
                        "last_seen_log_id": last_seen,
                        "has_unread": unread > 0,
                    });

                    // Add last message preview if available
                    let chat_logs = doc.get_array("chatLogs").ok()
                        .or_else(|| doc.get_array("l").ok());
                    if let Some(logs) = chat_logs {
                        if let Some(last) = logs.last().and_then(|l| l.as_document()) {
                            chat["last_message"] = serde_json::json!({
                                "message": last.get_str("message").or_else(|_| last.get_str("m")).unwrap_or(""),
                                "type": last.get_i32("type").or_else(|_| last.get_i32("t")).unwrap_or(0),
                                "send_at": get_bson_i64(last, &["sendAt", "s"]),
                            });
                        }
                    }

                    chats.push(chat);
                }
            }
            println!("{}", serde_json::to_string_pretty(&chats)?);
        } else {
            let mut count = 0;
            for cd in chat_datas {
                if let Some(doc) = cd.as_document() {
                    let chat_id = get_bson_i64(doc, &["c", "chatId"]);
                    let chat_type = get_bson_str(doc, &["t", "type"]);
                    let label = type_label(&chat_type);
                    let active_members = get_bson_i32(doc, &["a", "activeMembersCount"]);
                    let last_log_id = get_bson_i64(doc, &["s", "lastLogId"]);
                    let last_seen = get_bson_i64(doc, &["ll", "lastSeenLogId"]);
                    let has_unread = last_log_id > last_seen;

                    // Name from chatInfo or member names
                    let name = doc.get_document("chatInfo").ok()
                        .and_then(|ci| ci.get_str("name").ok())
                        .map(String::from)
                        .unwrap_or_default();
                    let name = if name.is_empty() {
                        get_bson_str_array(doc, &["k"]).join(", ")
                    } else {
                        name
                    };

                    // Last message preview from chatLogs or "l"
                    let chat_logs = doc.get_array("chatLogs").ok()
                        .or_else(|| doc.get_array("l").ok());
                    let (preview, last_time) = if let Some(logs) = chat_logs {
                        if let Some(last) = logs.last().and_then(|l| l.as_document()) {
                            let msg = last.get_str("message").or_else(|_| last.get_str("m")).unwrap_or("");
                            let t = get_bson_i64(last, &["sendAt", "s"]);
                            (msg.to_string(), t)
                        } else {
                            (String::new(), 0)
                        }
                    } else {
                        (String::new(), 0)
                    };

                    let time_from_doc = get_bson_i64(doc, &["o"]);
                    let display_time = if last_time > 0 { last_time } else { time_from_doc };

                    if !show_all && !has_unread && preview.is_empty() && name.is_empty() {
                        continue;
                    }

                    count += 1;
                    let time_str = format_time(display_time);

                    if color_enabled() {
                        println!(
                            "{} {} {} ({} members){}",
                            format!("[{}]", label).cyan(),
                            format!("{}", chat_id).dimmed(),
                            if name.is_empty() {
                                truncate(&preview, 30).to_string()
                            } else {
                                name.clone()
                            },
                            active_members,
                            if has_unread { " *".red().to_string() } else { String::new() }
                        );
                        if !preview.is_empty() || display_time > 0 {
                            println!(
                                "  {} {}",
                                time_str.dimmed(),
                                truncate(&preview, 60)
                            );
                        }
                    } else {
                        let unread_marker = if has_unread { " *" } else { "" };
                        println!(
                            "[{}] {} {} ({} members){}",
                            label, chat_id,
                            if name.is_empty() { truncate(&preview, 30).to_string() } else { name },
                            active_members, unread_marker,
                        );
                        if !preview.is_empty() || display_time > 0 {
                            println!("  {} {}", time_str, truncate(&preview, 60));
                        }
                    }
                }
            }
            eprintln!("({} chats shown)", count);
        }

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
                        "[loco-read] Resume with: openkakao-rs loco-read {} --all --cursor {}",
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
                    "[loco-read] SYNCMSG returned status={}. Resume with --cursor {}",
                    response.status(),
                    cur
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

fn cmd_loco_members(chat_id: i64, json: bool) -> Result<()> {
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

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

        if json {
            let mut result = Vec::new();
            for m in &members {
                if let Some(doc) = m.as_document() {
                    let uid = doc
                        .get_i64("userId")
                        .or_else(|_| doc.get_i32("userId").map(|v| v as i64))
                        .unwrap_or(0);
                    let nick = doc.get_str("nickName").unwrap_or("");
                    let country = doc.get_str("countryIso").unwrap_or("");
                    result.push(serde_json::json!({
                        "user_id": uid,
                        "nickname": nick,
                        "country": country,
                    }));
                }
            }
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            print_section_title(&format!(
                "Members of chat {} ({} members)",
                chat_id,
                members.len()
            ));
            for m in &members {
                if let Some(doc) = m.as_document() {
                    let uid = doc
                        .get_i64("userId")
                        .or_else(|_| doc.get_i32("userId").map(|v| v as i64))
                        .unwrap_or(0);
                    let nick = doc.get_str("nickName").unwrap_or("???");
                    if color_enabled() {
                        println!("  {} {}", format!("{}", uid).dimmed(), nick.bold());
                    } else {
                        println!("  {} {}", uid, nick);
                    }
                }
            }
        }

        Ok(())
    })
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
            last_token.chars().take(40).collect::<String>()
        );
    }
    if !last_oauth.is_empty() {
        eprintln!(
            "  Current oauth_token:   {}...",
            last_oauth.chars().take(40).collect::<String>()
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
                                new_token.chars().take(40).collect::<String>()
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
                        cand.oauth_token.chars().take(40).collect::<String>()
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

fn get_rest_client() -> Result<KakaoRestClient> {
    let creds = get_creds()?;
    let client = KakaoRestClient::new(creds)?;

    // Verify token upfront; if invalid, try re-extracting from Cache.db
    match client.verify_token() {
        Ok(true) => return Ok(client),
        Ok(false) => {
            eprintln!("[auth] Token invalid, re-extracting from Cache.db...");
        }
        Err(_) => return Ok(client), // network error, proceed with current token
    }

    // Try fresh extraction
    let fresh = get_credential_candidates(8)?;
    if fresh.is_empty() {
        return Ok(client); // no fresh candidates, use what we have
    }

    match select_best_credential(fresh) {
        Ok(new_creds) => {
            eprintln!("[auth] Refreshed token.");
            KakaoRestClient::new(new_creds)
        }
        Err(_) => Ok(client),
    }
}

fn cmd_doctor(json: bool, test_loco: bool) -> Result<()> {
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
                                creds.oauth_token.chars().take(20).collect::<String>()
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
        println!("{}", serde_json::to_string_pretty(&items)?);
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
    }

    Ok(())
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
                let save_name = format!("{}_{}", log_id, filename);
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
    // Try saved credentials first (fast, no Cache.db access)
    if let Some(saved) = load_credentials()? {
        return Ok(saved);
    }

    // Fall back to Cache.db extraction (may block if KakaoTalk locks the directory)
    let candidates = get_credential_candidates(8)?;
    if !candidates.is_empty() {
        return select_best_credential(candidates);
    }

    get_credentials_interactive()
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
            println!("  Error: Upgrade required (-999).");
            println!(
                "  The app version string is too old. Update KakaoTalk and re-extract credentials."
            );
        }
        -400 => {
            println!("  Error: Bad request (-400). Missing required parameter.");
        }
        -301 => {
            println!("  Error: Account restricted (-301). Your account may be under review.");
            println!("  WARNING: Do not retry aggressively. Wait and check KakaoTalk app.");
        }
        -1 => {
            println!("  Error: Connection failed or no status in response.");
            println!("  Run 'openkakao-rs doctor --loco' to check connectivity.");
        }
        _ => {
            println!(
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

/// Get string from BSON doc, trying multiple field names.
fn get_bson_str(doc: &bson::Document, keys: &[&str]) -> String {
    for k in keys {
        if let Ok(v) = doc.get_str(k) {
            return v.to_string();
        }
    }
    String::new()
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

fn select_best_credential(candidates: Vec<KakaoCredentials>) -> Result<KakaoCredentials> {
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for c in candidates {
        if seen.insert(c.oauth_token.clone()) {
            unique.push(c);
        }
    }

    let first = unique
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No credentials candidate"))?;

    for creds in unique {
        let client = match KakaoRestClient::new(creds.clone()) {
            Ok(client) => client,
            Err(_) => continue,
        };

        match client.verify_token() {
            Ok(true) => return Ok(creds),
            Ok(false) => continue,
            Err(_) => continue,
        }
    }

    eprintln!("[auth] No valid token candidate found; using newest cached token.");
    Ok(first)
}
