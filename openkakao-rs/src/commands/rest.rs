use std::collections::HashMap;

use anyhow::Result;
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::export::ExportFormat;
use crate::model::{json_i64, json_string};
use crate::rest::KakaoRestClient;
use crate::util::{
    color_enabled, confirm, format_time, get_creds, get_rest_client, member_name_map,
    print_section_title, print_table, truncate, type_label,
};

pub fn cmd_me(json: bool) -> Result<()> {
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
            let snapshot = super::profile::build_local_friend_graph().map_err(|local_err| {
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

pub fn filter_friend_search<T, F>(items: &mut Vec<T>, search: Option<String>, key: F)
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

pub fn cmd_friends(
    favorites: bool,
    hidden: bool,
    search: Option<String>,
    local: bool,
    chat_id: Option<i64>,
    user_id: Option<i64>,
    json: bool,
) -> Result<()> {
    if local {
        return super::profile::cmd_friends_local(
            favorites, hidden, search, chat_id, user_id, json,
        );
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

pub fn cmd_settings(json: bool) -> Result<()> {
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

pub fn cmd_scrap(url: &str, json: bool) -> Result<()> {
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

pub fn cmd_chatinfo(chat_id: i64, json: bool) -> Result<()> {
    super::probe::cmd_loco_chatinfo(chat_id, json)
}

pub fn cmd_favorite(user_id: i64) -> Result<()> {
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

pub fn cmd_unfavorite(user_id: i64) -> Result<()> {
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

pub fn cmd_hide(user_id: i64) -> Result<()> {
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

pub fn cmd_unhide(user_id: i64) -> Result<()> {
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

pub fn cmd_profiles(json: bool) -> Result<()> {
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

pub fn cmd_keywords(json: bool) -> Result<()> {
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

pub fn cmd_unread(json: bool) -> Result<()> {
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

pub fn cmd_export(chat_id: i64, format: &str, output: Option<&str>) -> Result<()> {
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
    crate::export::export_messages(&messages, &members, my_user_id, &fmt, output)?;

    if let Some(path) = output {
        eprintln!("Exported to {}", path);
    }

    Ok(())
}

pub fn cmd_search(chat_id: i64, query: &str, json: bool) -> Result<()> {
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
