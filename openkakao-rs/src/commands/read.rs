use std::collections::HashMap;

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::loco;
use crate::loco_helpers::loco_connect_with_auto_refresh;
use crate::rest::KakaoRestClient;
use crate::util::{
    color_enabled, extract_chat_type, format_time, get_bson_i32, get_bson_i64, get_bson_str,
    get_creds, is_open_chat, member_name_map, parse_since_date, type_label,
};

#[derive(Debug, Clone)]
pub struct ReadCommandOptions {
    pub count: usize,
    pub cursor: Option<i64>,
    pub since: Option<String>,
    pub all: bool,
    pub delay_ms: u64,
    pub force: bool,
    pub rest: bool,
    pub json: bool,
}

pub fn cmd_read_rest(
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

pub fn cmd_read(chat_id: i64, options: ReadCommandOptions) -> Result<()> {
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

pub fn cmd_loco_read(
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
