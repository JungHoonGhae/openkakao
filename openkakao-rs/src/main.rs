mod auth;
mod credentials;
mod error;
mod export;
mod loco;
mod model;
mod rest;

use std::collections::{HashMap, HashSet};
use std::io;
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
    /// Re-login via login.json using cached credentials
    Relogin {
        /// Generate fresh X-VC values instead of using cached one
        #[arg(long)]
        fresh_xvc: bool,
    },
    /// Test LOCO protocol connection (booking -> checkin -> login)
    LocoTest,
    /// Send a message via LOCO protocol
    Send {
        chat_id: i64,
        message: String,
        #[arg(long, help = "Allow sending to open chats (higher ban risk)")]
        force: bool,
    },
    /// Watch real-time messages via LOCO protocol
    Watch {
        #[arg(long, help = "Filter by chat ID")]
        chat_id: Option<i64>,
        #[arg(long, help = "Show raw BSON body")]
        raw: bool,
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
        #[arg(long, default_value_t = 100, help = "Delay between batches in ms (rate limit)")]
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
        } => cmd_read(chat_id, count, cursor.or(before), since.as_deref(), all, json)?,
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
        Commands::Relogin { fresh_xvc } => cmd_relogin(json, fresh_xvc)?,
        Commands::LocoTest => cmd_loco_test()?,
        Commands::Send { chat_id, message, force } => cmd_send(chat_id, &message, force)?,
        Commands::Watch { chat_id, raw } => cmd_watch(chat_id, raw)?,
        Commands::LocoChats { show_all } => cmd_loco_chats(show_all, json)?,
        Commands::LocoRead {
            chat_id,
            count,
            cursor,
            since,
            all,
            delay_ms,
            force,
        } => cmd_loco_read(chat_id, count, cursor, since.as_deref(), all, delay_ms, force, json)?,
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

fn cmd_read(chat_id: i64, count: usize, cursor: Option<i64>, since: Option<&str>, all: bool, json: bool) -> Result<()> {
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

fn cmd_relogin(json: bool, fresh_xvc: bool) -> Result<()> {
    let creds = get_creds()?;
    eprintln!("Extracting login.json parameters from Cache.db...");

    let params = match extract_login_params()? {
        Some(p) => {
            eprintln!("  Email: {}", p.email);
            eprintln!("  Device: {}", p.device_name);
            eprintln!(
                "  Password hash: {}...",
                p.password.chars().take(20).collect::<String>()
            );
            eprintln!("  Cached X-VC: {}", p.x_vc);
            p
        }
        None => {
            eprintln!("  No login.json parameters found in Cache.db.");
            return Ok(());
        }
    };

    let client = KakaoRestClient::new(creds.clone())?;

    let response = if fresh_xvc {
        eprintln!("Logging in with generated X-VC...");
        client.login_with_xvc(
            &params.email,
            &params.password,
            &params.device_uuid,
            &params.device_name,
        )?
    } else {
        eprintln!("Calling login.json with cached X-VC...");
        client.login_direct(
            &params.email,
            &params.password,
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
    let mut creds = get_creds()?;
    refresh_loco_credentials(&mut creds)?;

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

fn cmd_send(chat_id: i64, message: &str, force: bool) -> Result<()> {
    let mut creds = get_creds()?;
    refresh_loco_credentials(&mut creds)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        eprintln!("Connecting via LOCO...");
        let login_data = client.full_connect_with_retry(3).await?;
        let status = login_data
            .get_i64("status")
            .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
            .unwrap_or(-1);
        if status != 0 {
            print_loco_error_hint(status);
            anyhow::bail!("LOCO login failed (status={})", status);
        }

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

/// Get a fresh LOCO-compatible token via login.json + X-VC, updating creds in place.
fn refresh_loco_credentials(creds: &mut KakaoCredentials) -> Result<()> {
    // If user_id is 0, try to fetch it from the REST profile
    if creds.user_id == 0 {
        let rest = KakaoRestClient::new(creds.clone())?;
        if let Ok(profile) = rest.get_my_profile() {
            if profile.user_id > 0 {
                creds.user_id = profile.user_id;
            }
        }
    }

    let params = match extract_login_params() {
        Ok(Some(p)) => p,
        _ => return Ok(()),
    };

    eprintln!("[login] Getting fresh token via login.json + X-VC...");
    let rest = KakaoRestClient::new(creds.clone())?;
    match rest.login_with_xvc(
        &params.email,
        &params.password,
        &params.device_uuid,
        &params.device_name,
    ) {
        Ok(resp) => {
            let status = resp.get("status").and_then(Value::as_i64).unwrap_or(-1);
            if status == 0 {
                if let Some(access) = resp.get("access_token").and_then(Value::as_str) {
                    eprintln!(
                        "[login] Fresh token: {}...",
                        access.chars().take(40).collect::<String>()
                    );
                    creds.oauth_token = access.to_string();
                    if let Some(uid) = resp.get("userId").and_then(Value::as_i64) {
                        creds.user_id = uid;
                    }
                    save_credentials(creds)?;
                }
            } else {
                eprintln!("[login] login.json returned status={}", status);
            }
        }
        Err(e) => eprintln!("[login] login.json failed: {}", e),
    }

    Ok(())
}

fn cmd_watch(filter_chat_id: Option<i64>, raw: bool) -> Result<()> {
    let mut creds = get_creds()?;
    refresh_loco_credentials(&mut creds)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = client.full_connect_with_retry(3).await?;

        let status = login_data
            .get_i64("status")
            .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
            .unwrap_or(-1);
        if status != 0 {
            anyhow::bail!("LOCO login failed (status={})", status);
        }

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
        eprintln!(
            "[watch] Connected! Listening for messages... ({} chats loaded)",
            chat_count
        );
        if let Some(cid) = filter_chat_id {
            eprintln!("[watch] Filtering chat_id={}", cid);
        }
        eprintln!("[watch] Press Ctrl-C to stop.");

        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(60));
        // Skip the first immediate tick
        ping_interval.tick().await;

        loop {
            tokio::select! {
                packet_result = client.recv_packet() => {
                    match packet_result {
                        Ok(packet) => {
                            let method = &packet.method;

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

                                    // Extract sender nickname from authorNickname or author.nickName
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

                                    // Message type: 1=text, 2=photo, etc.
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
                                }
                                "DECUNREAD" | "NOTIREAD" | "SYNCLINKCR" | "SYNCLINKUP"
                                | "SYNCMSG" | "SYNCDLMSG" | "CHANGESVR" => {
                                    // Known push events, silently ignore
                                }
                                _ => {
                                    eprintln!("[watch] Push: {} (status={})", method, packet.status());
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[watch] Connection lost: {}", e);
                            break;
                        }
                    }
                }
                _ = ping_interval.tick() => {
                    if let Err(e) = client.send_packet("PING", bson::doc! {}).await {
                        eprintln!("[watch] PING failed: {}", e);
                        break;
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    eprintln!("\n[watch] Shutting down...");
                    break;
                }
            }
        }

        Ok(())
    })
}

fn cmd_loco_chats(show_all: bool, json: bool) -> Result<()> {
    let mut creds = get_creds()?;
    refresh_loco_credentials(&mut creds)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = client.full_connect_with_retry(3).await?;
        let status = login_data
            .get_i64("status")
            .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
            .unwrap_or(-1);
        if status != 0 {
            anyhow::bail!("LOCO login failed (status={})", status);
        }

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

fn cmd_loco_read(chat_id: i64, count: i32, cursor: Option<i64>, since: Option<&str>, fetch_all: bool, delay_ms: u64, force: bool, json: bool) -> Result<()> {
    let since_ts = parse_since_date(since)?;
    let mut creds = get_creds()?;
    refresh_loco_credentials(&mut creds)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = client.full_connect_with_retry(3).await?;
        let status = login_data
            .get_i64("status")
            .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
            .unwrap_or(-1);
        if status != 0 {
            print_loco_error_hint(status);
            anyhow::bail!("LOCO login failed (status={})", status);
        }

        // Get lastLogId for this chat via CHATONROOM
        let room_info = client.send_command("CHATONROOM", bson::doc! {
            "chatId": chat_id,
        }).await?;
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
            eprintln!("Note: delay raised to 500ms for open chat safety (was {}ms)", delay_ms);
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
            let response = match client.send_command("SYNCMSG", bson::doc! {
                "chatId": chat_id,
                "cur": cur,
                "cnt": 50_i32,
                "max": max_log,
            }).await {
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
                    response.status(), cur
                );
                break;
            }

            let is_ok = response.body.get_bool("isOK").unwrap_or(true);
            let chat_logs = response.body.get_array("chatLogs")
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
                batch_num, batch_count, all_messages.len(), max_log_in_batch
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
                let nick = msg.get("author_nickname").and_then(|v| v.as_str()).unwrap_or("");
                let author_id = msg.get("author_id").and_then(|v| v.as_i64()).unwrap_or(0);
                let msg_type = msg.get("message_type").and_then(|v| v.as_i64()).unwrap_or(0);
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
                    _ => if message.is_empty() {
                        format!("[type={}]", msg_type)
                    } else {
                        message.to_string()
                    },
                };

                if color_enabled() {
                    println!(
                        "{} {}: {}",
                        time_str.dimmed(),
                        display_nick.bold(),
                        content
                    );
                } else {
                    println!("{} {}: {}", time_str, display_nick, content);
                }
            }
            // Print resume hint with last cursor
            let last_cursor = all_messages.last()
                .and_then(|m| m.get("log_id").and_then(|v| v.as_i64()))
                .unwrap_or(0);
            eprintln!("({} messages, last_cursor={})", all_messages.len(), last_cursor);
        }

        Ok(())
    })
}

fn cmd_loco_members(chat_id: i64, json: bool) -> Result<()> {
    let mut creds = get_creds()?;
    refresh_loco_credentials(&mut creds)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = client.full_connect_with_retry(3).await?;
        let status = login_data
            .get_i64("status")
            .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
            .unwrap_or(-1);
        if status != 0 {
            anyhow::bail!("LOCO login failed (status={})", status);
        }

        let response = client
            .send_command("GETMEM", bson::doc! { "chatId": chat_id })
            .await?;

        if response.status() != 0 {
            anyhow::bail!("GETMEM failed (status={})", response.status());
        }

        let members = response.body.get_array("members")
            .map(|a| a.to_vec())
            .unwrap_or_default();

        if json {
            let mut result = Vec::new();
            for m in &members {
                if let Some(doc) = m.as_document() {
                    let uid = doc.get_i64("userId")
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
            print_section_title(&format!("Members of chat {} ({} members)", chat_id, members.len()));
            for m in &members {
                if let Some(doc) = m.as_document() {
                    let uid = doc.get_i64("userId")
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
    let mut creds = get_creds()?;
    refresh_loco_credentials(&mut creds)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = client.full_connect_with_retry(3).await?;
        let status = login_data
            .get_i64("status")
            .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
            .unwrap_or(-1);
        if status != 0 {
            anyhow::bail!("LOCO login failed (status={})", status);
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

    // 1. KakaoTalk.app installed version
    let app_plist = PathBuf::from("/Applications/KakaoTalk.app/Contents/Info.plist");
    if app_plist.exists() {
        match plist::from_file::<_, plist::Dictionary>(&app_plist) {
            Ok(dict) => {
                let version = dict
                    .get("CFBundleShortVersionString")
                    .and_then(|v| v.as_string())
                    .unwrap_or("unknown");
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
            let pids = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
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
    let cache_db = home
        .join("Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Caches/Cache.db");
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
            "handshake_key_type=16, encrypt_type=2 (AES-128-CFB), RSA=2048-bit e=3, booking={}:{}",
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
            println!(
                "  [{}] {}: {}",
                color_fn(icon),
                c.name,
                c.detail
            );
        }

        if !test_loco {
            println!();
            println!("  Tip: run with --loco to also test LOCO booking connectivity.");
        }
    }

    Ok(())
}

fn get_creds() -> Result<KakaoCredentials> {
    let mut candidates = Vec::new();
    candidates.extend(get_credential_candidates(8)?);
    if let Some(saved) = load_credentials()? {
        candidates.push(saved);
    }

    if candidates.is_empty() {
        return get_credentials_interactive();
    }

    select_best_credential(candidates)
}

fn print_loco_error_hint(status: i64) {
    match status {
        -950 => {
            println!("  Error: Token rejected for LOCO (-950).");
            println!("  Likely causes:");
            println!("    1. Encryption mismatch: server may expect AES-128-GCM (encrypt_type=3)");
            println!("       but we send AES-128-CFB (encrypt_type=2). See IMPROVEMENT_PLAN.md.");
            println!("    2. Token expired: open KakaoTalk, browse chats, then 'openkakao-rs login --save'.");
            println!("    3. Missing fields: prtVer, pcst, or rp may be required.");
        }
        -999 => {
            println!("  Error: Upgrade required (-999).");
            println!("  The app version string is too old. Update KakaoTalk and re-extract credentials.");
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
            println!("  Unknown LOCO error (status={}). Run 'openkakao-rs doctor' for diagnostics.", status);
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
            return arr.iter()
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
