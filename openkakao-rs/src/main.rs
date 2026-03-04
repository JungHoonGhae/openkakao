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
        #[arg(long)]
        before: Option<i64>,
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
    Send { chat_id: i64, message: String },
    /// Watch Cache.db for fresh tokens (poll every N seconds)
    WatchCache {
        #[arg(long, default_value_t = 10)]
        interval: u64,
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
            all,
        } => cmd_read(chat_id, count, before, all, json)?,
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
        Commands::Send { chat_id, message } => cmd_send(chat_id, &message)?,
        Commands::WatchCache { interval } => cmd_watch_cache(interval)?,
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

fn cmd_read(chat_id: i64, count: usize, before: Option<i64>, all: bool, json: bool) -> Result<()> {
    let creds = get_creds()?;
    let client = KakaoRestClient::new(creds.clone())?;

    let mut messages = if all {
        client.get_all_messages(chat_id, 100)?
    } else {
        let (msgs, _next_cursor) = client.get_messages(chat_id, before)?;
        msgs
    };

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
                "\nShowing {} messages. For older: openkakao-rs read {} --before {}",
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

    // If user_id is 0, try to fetch it from the REST profile
    if creds.user_id == 0 {
        let rest = KakaoRestClient::new(creds.clone())?;
        if let Ok(profile) = rest.get_my_profile() {
            if profile.user_id > 0 {
                creds.user_id = profile.user_id;
            }
        }
    }

    eprintln!("Testing LOCO connection for user {}...", creds.user_id);
    eprintln!(
        "  Token: {}...",
        creds.oauth_token.chars().take(40).collect::<String>()
    );

    // Get a fresh token via login.json + X-VC before LOCO
    if let Ok(Some(params)) = extract_login_params() {
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
                        save_credentials(&creds)?;
                    }
                } else {
                    eprintln!("[login] login.json returned status={}", status);
                }
            }
            Err(e) => {
                eprintln!("[login] login.json failed: {}", e);
            }
        }
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds.clone());
        let login_data = client.full_connect().await?;

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

            if status == -950 {
                println!("  Token rejected for LOCO (-950).");
            }
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

fn cmd_send(chat_id: i64, message: &str) -> Result<()> {
    eprint!(
        "Send message to chat {}? Message: \"{}\"\n[y/N] ",
        chat_id,
        truncate(message, 50)
    );
    if !confirm()? {
        println!("Cancelled.");
        return Ok(());
    }

    let creds = get_creds()?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        eprintln!("Connecting via LOCO...");
        client.full_connect().await?;

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

fn print_section_title(title: &str) {
    if color_enabled() {
        println!("{}", title.bold().cyan());
    } else {
        println!("{}", title);
    }
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

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut truncated = s.chars().take(max_chars).collect::<String>();
        truncated.push_str("...");
        truncated
    }
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
