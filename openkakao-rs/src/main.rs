mod auth;
mod credentials;
mod model;
mod rest;

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::{Datelike, Local, TimeZone};
use clap::{Parser, Subcommand};
use serde_json::Value;

use crate::auth::{get_credential_candidates, get_credentials_interactive};
use crate::credentials::{load_credentials, save_credentials};
use crate::model::{json_i64, json_string, ChatMember, KakaoCredentials};
use crate::rest::KakaoRestClient;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(name = "openkakao-rs")]
#[command(about = "OpenKakao Rust CLI", long_about = None)]
#[command(version = VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Auth,
    Login {
        #[arg(long)]
        save: bool,
    },
    Me,
    Friends {
        #[arg(short = 'f', long)]
        favorites: bool,
        #[arg(long)]
        hidden: bool,
        #[arg(short = 's', long)]
        search: Option<String>,
    },
    Chats {
        #[arg(short = 'a', long = "all")]
        show_all: bool,
        #[arg(short = 'u', long)]
        unread: bool,
    },
    Read {
        chat_id: i64,
        #[arg(short = 'n', long, default_value_t = 30)]
        count: usize,
        #[arg(long)]
        before: Option<i64>,
    },
    Members {
        chat_id: i64,
    },
    Settings,
    Scrap {
        url: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Auth => cmd_auth()?,
        Commands::Login { save } => cmd_login(save)?,
        Commands::Me => cmd_me()?,
        Commands::Friends {
            favorites,
            hidden,
            search,
        } => cmd_friends(favorites, hidden, search)?,
        Commands::Chats { show_all, unread } => cmd_chats(show_all, unread)?,
        Commands::Read {
            chat_id,
            count,
            before,
        } => cmd_read(chat_id, count, before)?,
        Commands::Members { chat_id } => cmd_members(chat_id)?,
        Commands::Settings => cmd_settings()?,
        Commands::Scrap { url } => cmd_scrap(&url)?,
    }

    Ok(())
}

fn cmd_auth() -> Result<()> {
    let creds = get_creds()?;

    println!("  User ID: {}", creds.user_id);
    println!("  Token:   {}...", creds.oauth_token.chars().take(40).collect::<String>());
    println!("  Version: {}", creds.app_version);

    let client = KakaoRestClient::new(creds)?;
    if client.verify_token()? {
        println!("  Token is valid!");
    } else {
        println!("  Token is invalid or expired.");
        println!("  Hint: open KakaoTalk, open chat list once, then run 'openkakao-rs login --save'.");
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
    println!("  Token:   {}...", creds.oauth_token.chars().take(40).collect::<String>());

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

fn cmd_me() -> Result<()> {
    let client = get_rest_client()?;
    let profile = client.get_my_profile()?;

    println!("My Profile");
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

fn cmd_friends(favorites: bool, hidden: bool, search: Option<String>) -> Result<()> {
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
            f.display_name().to_lowercase().contains(&q) || f.phone_number.to_lowercase().contains(&q)
        });
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

    println!("Friends ({})", rows.len());
    print_table(&["Name", "Status", "Phone", "User ID"], rows);
    Ok(())
}

fn cmd_chats(show_all: bool, unread: bool) -> Result<()> {
    let client = get_rest_client()?;

    let mut chats = if show_all {
        client.get_all_chats()?
    } else {
        client.get_chats(None)?.0
    };

    if unread {
        chats.retain(|c| c.unread_count > 0);
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

    println!("Chats ({})", rows.len());
    print_table(&["Type", "Name", "Unread", "Chat ID"], rows);
    Ok(())
}

fn cmd_read(chat_id: i64, count: usize, before: Option<i64>) -> Result<()> {
    let creds = get_creds()?;
    let client = KakaoRestClient::new(creds.clone())?;

    let mut messages = client.get_messages(chat_id, before)?;

    let member_map = match client.get_chat_members(chat_id) {
        Ok(members) => member_name_map(&members, creds.user_id),
        Err(_) => {
            let mut fallback = HashMap::new();
            fallback.insert(creds.user_id, "Me".to_string());
            fallback
        }
    };

    if messages.len() > count {
        messages.truncate(count);
    }
    messages.reverse();

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

        println!("{} [{}]: {}", time_str, name, body);
    }

    if let Some(oldest) = messages.first().map(|m| m.log_id) {
        println!(
            "\nShowing {} messages. For older: openkakao-rs read {} --before {}",
            messages.len(),
            chat_id,
            oldest
        );
    }

    Ok(())
}

fn cmd_members(chat_id: i64) -> Result<()> {
    let client = get_rest_client()?;
    let members = client.get_chat_members(chat_id)?;

    let mut rows = Vec::new();
    for m in members {
        rows.push(vec![m.display_name(), m.user_id.to_string(), m.country_iso]);
    }

    println!("Members ({})", rows.len());
    print_table(&["Name", "User ID", "Country"], rows);
    Ok(())
}

fn cmd_settings() -> Result<()> {
    let client = get_rest_client()?;
    let settings = client.get_settings()?;

    println!("Account Settings");
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

fn cmd_scrap(url: &str) -> Result<()> {
    let client = get_rest_client()?;
    let data = client.get_scrap_preview(url)?;

    println!("Link Preview");
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

fn get_rest_client() -> Result<KakaoRestClient> {
    let creds = get_creds()?;
    KakaoRestClient::new(creds)
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

    let header_line = headers
        .iter()
        .enumerate()
        .map(|(idx, h)| format!("{:width$}", h, width = widths[idx]))
        .collect::<Vec<_>>()
        .join("  ");
    println!("{header_line}");

    let separator = widths
        .iter()
        .map(|w| "-".repeat(*w))
        .collect::<Vec<_>>()
        .join("  ");
    println!("{separator}");

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
