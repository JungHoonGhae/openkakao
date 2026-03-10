use std::collections::HashMap;

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::error::OpenKakaoError;
use crate::loco;
use crate::loco_helpers::loco_connect_with_auto_refresh;
use crate::rest::KakaoRestClient;
use crate::util::{
    build_member_name_map_from_bson, color_enabled, extract_chat_type, format_time, get_bson_i32,
    get_bson_i64, get_bson_str, get_creds, is_open_chat, member_name_map, parse_since_date,
    type_label,
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

    match cmd_loco_read(chat_id, &options) {
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

fn extract_loginlist_messages(
    login_data: &bson::Document,
    chat_id: i64,
    since_ts: Option<i64>,
    member_names: &mut HashMap<i64, String>,
) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();
    let Ok(chat_datas) = login_data.get_array("chatDatas") else {
        return messages;
    };
    for cd in chat_datas {
        let Some(doc) = cd.as_document() else {
            continue;
        };
        let cid = get_bson_i64(doc, &["c", "chatId"]);
        if cid != chat_id {
            continue;
        }
        if let Ok(k_arr) = doc.get_array("k") {
            if let Ok(i_arr) = doc.get_array("i") {
                for (uid_val, name_val) in i_arr.iter().zip(k_arr.iter()) {
                    if let (Some(uid), Some(name)) = (uid_val.as_i64(), name_val.as_str()) {
                        if uid > 0 && !name.is_empty() {
                            member_names.entry(uid).or_insert_with(|| name.to_string());
                        }
                    }
                }
            }
        }
        let logs: Vec<&bson::Document> = if let Ok(d) = doc.get_document("l") {
            vec![d]
        } else if let Ok(d) = doc.get_document("chatLog") {
            vec![d]
        } else if let Ok(a) = doc.get_array("chatLog") {
            a.iter().filter_map(|v| v.as_document()).collect()
        } else if let Ok(a) = doc.get_array("chatLogs") {
            a.iter().filter_map(|v| v.as_document()).collect()
        } else {
            vec![]
        };
        for log_doc in &logs {
            let log_id = get_bson_i64(log_doc, &["logId"]);
            if log_id == 0 {
                continue;
            }
            let author_id = get_bson_i64(log_doc, &["authorId"]);
            let msg_type = get_bson_i32(log_doc, &["type"]);
            let message = get_bson_str(log_doc, &["message"]);
            let send_at = get_bson_i64(log_doc, &["sendAt"]);
            let author_nick = get_bson_str(log_doc, &["authorNickname"]);
            let attachment = get_bson_str(log_doc, &["attachment"]);
            if let Some(ts) = since_ts {
                if send_at < ts {
                    continue;
                }
            }
            messages.push(serde_json::json!({
                "log_id": log_id,
                "author_id": author_id,
                "author_nickname": author_nick,
                "message_type": msg_type,
                "message": message,
                "attachment": attachment,
                "send_at": send_at,
            }));
        }
        if !logs.is_empty() {
            eprintln!(
                "[loco-read] Got {} messages from LOGINLIST chatLog",
                logs.len()
            );
        }
        break;
    }
    messages
}

struct SyncmsgParams<'a> {
    chat_id: i64,
    max_log: i64,
    cursor: Option<i64>,
    since_ts: Option<i64>,
    effective_delay: u64,
    has_existing_messages: bool,
    existing_ids: &'a std::collections::HashSet<i64>,
}

async fn fetch_syncmsg_pages(
    client: &mut crate::loco::client::LocoClient,
    params: &SyncmsgParams<'_>,
) -> Result<Vec<serde_json::Value>> {
    let SyncmsgParams {
        chat_id,
        max_log,
        cursor,
        since_ts,
        effective_delay,
        has_existing_messages,
        existing_ids,
    } = params;
    let chat_id = *chat_id;
    let max_log = *max_log;
    let since_ts = *since_ts;
    let effective_delay = *effective_delay;
    let has_existing_messages = *has_existing_messages;
    let mut messages = Vec::new();
    let mut cur = cursor.unwrap_or(0);
    let mut batch_num = 0u32;
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
                if !has_existing_messages && messages.is_empty() {
                    return Err(e.context("SYNCMSG failed"));
                }
                eprintln!("[loco-read] Connection lost: {}", e);
                eprintln!(
                    "[loco-read] Resume with: openkakao-rs read {} --all --cursor {}",
                    chat_id, cur
                );
                break;
            }
        };

        if response.status() != 0 {
            if !has_existing_messages && messages.is_empty() {
                return Err(OpenKakaoError::loco("SYNCMSG", response.status()).into());
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

                if existing_ids.contains(&log_id) {
                    continue;
                }

                let author_id = get_bson_i64(doc, &["authorId"]);
                let msg_type = get_bson_i32(doc, &["type"]);
                let message = get_bson_str(doc, &["message"]);
                let send_at = get_bson_i64(doc, &["sendAt"]);
                let author_nick = get_bson_str(doc, &["authorNickname"]);
                let attachment = get_bson_str(doc, &["attachment"]);

                if let Some(ts) = since_ts {
                    if send_at < ts {
                        continue;
                    }
                }

                messages.push(serde_json::json!({
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
            messages.len(),
            max_log_in_batch
        );

        if is_ok || max_log_in_batch == 0 {
            break;
        }
        cur = max_log_in_batch;

        if effective_delay > 0 && !is_ok {
            tokio::time::sleep(std::time::Duration::from_millis(effective_delay)).await;
        }
    }
    Ok(messages)
}

/// Streaming variant of fetch_syncmsg_pages: prints each message as NDJSON immediately
/// instead of collecting into a Vec. Returns the count of messages streamed.
async fn stream_syncmsg_pages(
    client: &mut crate::loco::client::LocoClient,
    params: &SyncmsgParams<'_>,
) -> Result<usize> {
    let SyncmsgParams {
        chat_id,
        max_log,
        cursor,
        since_ts,
        effective_delay,
        has_existing_messages,
        existing_ids,
    } = params;
    let chat_id = *chat_id;
    let max_log = *max_log;
    let since_ts = *since_ts;
    let effective_delay = *effective_delay;
    let has_existing_messages = *has_existing_messages;
    let mut total_streamed = 0usize;
    let mut cur = cursor.unwrap_or(0);
    let mut batch_num = 0u32;
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
                if !has_existing_messages && total_streamed == 0 {
                    return Err(e.context("SYNCMSG failed"));
                }
                eprintln!("[loco-read] Connection lost: {}", e);
                eprintln!(
                    "[loco-read] Resume with: openkakao-rs read {} --all --cursor {}",
                    chat_id, cur
                );
                break;
            }
        };

        if response.status() != 0 {
            if !has_existing_messages && total_streamed == 0 {
                return Err(OpenKakaoError::loco("SYNCMSG", response.status()).into());
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

                if existing_ids.contains(&log_id) {
                    continue;
                }

                let author_id = get_bson_i64(doc, &["authorId"]);
                let msg_type = get_bson_i32(doc, &["type"]);
                let message = get_bson_str(doc, &["message"]);
                let send_at = get_bson_i64(doc, &["sendAt"]);
                let author_nick = get_bson_str(doc, &["authorNickname"]);
                let attachment = get_bson_str(doc, &["attachment"]);

                if let Some(ts) = since_ts {
                    if send_at < ts {
                        continue;
                    }
                }

                let msg = serde_json::json!({
                    "log_id": log_id,
                    "author_id": author_id,
                    "author_nickname": author_nick,
                    "message_type": msg_type,
                    "message": message,
                    "attachment": attachment,
                    "send_at": send_at,
                });
                println!("{}", serde_json::to_string(&msg).unwrap_or_default());
                total_streamed += 1;
            }
        }

        batch_num += 1;
        eprintln!(
            "[loco-read] Batch {}: {} msgs (streamed: {}, cursor: {})",
            batch_num,
            batch_count,
            total_streamed,
            max_log_in_batch
        );

        if is_ok || max_log_in_batch == 0 {
            break;
        }
        cur = max_log_in_batch;

        if effective_delay > 0 && !is_ok {
            tokio::time::sleep(std::time::Duration::from_millis(effective_delay)).await;
        }
    }
    Ok(total_streamed)
}

fn format_and_output_messages(
    messages: &[serde_json::Value],
    member_names: &HashMap<i64, String>,
    json: bool,
) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(messages).unwrap_or_default()
        );
        return;
    }

    for msg in messages {
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

    let last_cursor = messages
        .last()
        .and_then(|m| m.get("log_id").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    eprintln!("({} messages, last_cursor={})", messages.len(), last_cursor);
}

pub fn cmd_loco_read(chat_id: i64, opts: &ReadCommandOptions) -> Result<()> {
    let since_ts = parse_since_date(opts.since.as_deref())?;
    let count = opts.count as i32;
    let cursor = opts.cursor;
    let fetch_all = opts.all;
    let delay_ms = opts.delay_ms;
    let force = opts.force;
    let json = opts.json;
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        client.sync_chat_ids = vec![(chat_id, 0)];
        let login_data = loco_connect_with_auto_refresh(&mut client).await?;

        let room_info = client
            .send_command(
                "CHATONROOM",
                bson::doc! {
                    "chatId": chat_id,
                },
            )
            .await?;
        if room_info.status() != 0 {
            return Err(OpenKakaoError::loco("CHATONROOM", room_info.status()).into());
        }

        let chat_type = extract_chat_type(&room_info.body);
        if is_open_chat(&chat_type) {
            if fetch_all && !force {
                eprintln!(
                    "Blocked: --all on open chat ({}) has higher ban risk.",
                    type_label(&chat_type)
                );
                eprintln!("Use --force to override this safety check.");
                return Err(OpenKakaoError::SafetyBlock(
                    "Open chat full-history blocked (use --force)".into(),
                )
                .into());
            }
            eprintln!(
                "Warning: reading from {} (open chat). Using conservative rate limiting.",
                type_label(&chat_type)
            );
        }

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

        let mut member_names: HashMap<i64, String> =
            if let Ok(members) = room_info.body.get_array("m") {
                build_member_name_map_from_bson(members)
            } else {
                HashMap::new()
            };

        if fetch_all {
            eprintln!(
                "[loco-read] Fetching full history (lastLogId={}, delay={}ms)...",
                last_log_id, effective_delay
            );
        }

        // Streaming path: when json && fetch_all, print each batch as NDJSON
        // instead of buffering the entire history into memory.
        if json && fetch_all {
            let loginlist_messages =
                extract_loginlist_messages(&login_data, chat_id, since_ts, &mut member_names);
            for msg in &loginlist_messages {
                println!("{}", serde_json::to_string(msg).unwrap_or_default());
            }

            let existing_ids: std::collections::HashSet<i64> = loginlist_messages
                .iter()
                .filter_map(|m| m.get("log_id").and_then(|v| v.as_i64()))
                .collect();

            let streamed = stream_syncmsg_pages(
                &mut client,
                &SyncmsgParams {
                    chat_id,
                    max_log: last_log_id,
                    cursor,
                    since_ts,
                    effective_delay,
                    has_existing_messages: !loginlist_messages.is_empty(),
                    existing_ids: &existing_ids,
                },
            )
            .await?;

            let total = loginlist_messages.len() + streamed;
            eprintln!("({} messages streamed)", total);
            return Ok(());
        }

        let mut all_messages =
            extract_loginlist_messages(&login_data, chat_id, since_ts, &mut member_names);

        let existing_ids: std::collections::HashSet<i64> = all_messages
            .iter()
            .filter_map(|m| m.get("log_id").and_then(|v| v.as_i64()))
            .collect();
        let syncmsg_messages = fetch_syncmsg_pages(
            &mut client,
            &SyncmsgParams {
                chat_id,
                max_log: last_log_id,
                cursor,
                since_ts,
                effective_delay,
                has_existing_messages: !all_messages.is_empty(),
                existing_ids: &existing_ids,
            },
        )
        .await?;
        all_messages.extend(syncmsg_messages);

        if !fetch_all && all_messages.len() > count as usize {
            let skip = all_messages.len() - count as usize;
            all_messages = all_messages.split_off(skip);
        }

        all_messages.sort_by_key(|m| m.get("send_at").and_then(|v| v.as_i64()).unwrap_or(0));

        format_and_output_messages(&all_messages, &member_names, json);

        Ok(())
    })
}
