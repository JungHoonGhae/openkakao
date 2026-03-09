use std::collections::HashMap;

use anyhow::Result;

use crate::loco_helpers::loco_connect_with_auto_refresh;
use crate::message_db;
use crate::util::{
    extract_chat_type, format_time, get_bson_i32, get_bson_i64, get_bson_str, get_creds,
    message_type_label, parse_since_date, print_section_title, print_table, truncate, type_label,
};

pub fn cmd_stats(
    chat_id: i64,
    limit: Option<usize>,
    since: Option<&str>,
    json: bool,
) -> Result<()> {
    let since_ts = parse_since_date(since)?;
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = crate::loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

        let room_info = client
            .send_command("CHATONROOM", bson::doc! { "chatId": chat_id })
            .await?;
        if room_info.status() != 0 {
            anyhow::bail!("CHATONROOM failed (status={})", room_info.status());
        }

        let chat_type = extract_chat_type(&room_info.body);
        let chat_title = room_info
            .body
            .get_document("chatInfo")
            .ok()
            .and_then(|ci| ci.get_str("name").ok())
            .unwrap_or("Unknown")
            .to_string();

        // Build member name map
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

        let last_log_id = room_info.body.get_i64("l").unwrap_or(0);
        if last_log_id == 0 {
            anyhow::bail!("No messages in this chat");
        }

        // Fetch messages via SYNCMSG
        let mut cur = 0i64;
        let mut total_messages = 0usize;
        let mut author_counts: HashMap<i64, usize> = HashMap::new();
        let mut type_counts: HashMap<i32, usize> = HashMap::new();
        let mut hourly_counts: [usize; 24] = [0; 24];
        let mut daily_counts: HashMap<String, usize> = HashMap::new();
        let mut first_ts: i64 = i64::MAX;
        let mut last_ts: i64 = 0;
        let max_messages = limit.unwrap_or(usize::MAX);

        eprintln!("[stats] Scanning messages...");

        loop {
            if total_messages >= max_messages {
                break;
            }

            let response = client
                .send_command(
                    "SYNCMSG",
                    bson::doc! {
                        "chatId": chat_id,
                        "cur": cur,
                        "cnt": 100_i32,
                        "max": last_log_id,
                    },
                )
                .await?;

            if response.status() != 0 {
                break;
            }

            let msgs = match response.body.get_array("chatLogs") {
                Ok(msgs) => msgs.clone(),
                Err(_) => break,
            };

            if msgs.is_empty() {
                break;
            }

            for msg in &msgs {
                if total_messages >= max_messages {
                    break;
                }
                let Some(doc) = msg.as_document() else {
                    continue;
                };

                let log_id = get_bson_i64(doc, &["logId"]);
                let author_id = get_bson_i64(doc, &["authorId"]);
                let msg_type = get_bson_i32(doc, &["type"]);
                let send_at = get_bson_i64(doc, &["sendAt"]);

                // Apply since filter
                if let Some(min_ts) = since_ts {
                    if send_at > 0 && send_at < min_ts {
                        cur = log_id;
                        continue;
                    }
                }

                total_messages += 1;
                *author_counts.entry(author_id).or_insert(0) += 1;
                *type_counts.entry(msg_type).or_insert(0) += 1;

                if send_at > 0 {
                    if send_at < first_ts {
                        first_ts = send_at;
                    }
                    if send_at > last_ts {
                        last_ts = send_at;
                    }

                    if let Some(dt) =
                        chrono::TimeZone::timestamp_opt(&chrono::Local, send_at, 0).single()
                    {
                        let hour: usize = dt.format("%H").to_string().parse().unwrap_or(0);
                        hourly_counts[hour] += 1;
                        let day_key = dt.format("%Y-%m-%d").to_string();
                        *daily_counts.entry(day_key).or_insert(0) += 1;
                    }
                }

                cur = log_id;
            }

            let is_ok = response.body.get_bool("isOK").unwrap_or(true);
            if is_ok {
                break;
            }

            if total_messages.is_multiple_of(500) {
                eprintln!("[stats] {} messages scanned...", total_messages);
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        if total_messages == 0 {
            eprintln!("No messages found.");
            return Ok(());
        }

        // Compute stats
        let mut author_stats: Vec<(i64, String, usize)> = author_counts
            .iter()
            .map(|(&uid, &count)| {
                let name = member_names
                    .get(&uid)
                    .cloned()
                    .unwrap_or_else(|| format!("User#{}", uid));
                (uid, name, count)
            })
            .collect();
        author_stats.sort_by(|a, b| b.2.cmp(&a.2));

        let mut type_stats: Vec<(i32, &str, usize)> = type_counts
            .iter()
            .map(|(&t, &count)| (t, message_type_label(t), count))
            .collect();
        type_stats.sort_by(|a, b| b.2.cmp(&a.2));

        // Find peak hour
        let peak_hour = hourly_counts
            .iter()
            .enumerate()
            .max_by_key(|(_, &count)| count)
            .map(|(h, _)| h)
            .unwrap_or(0);

        // Average daily messages
        let active_days = daily_counts.len();
        let avg_daily = if active_days > 0 {
            total_messages as f64 / active_days as f64
        } else {
            0.0
        };

        if json {
            let output = serde_json::json!({
                "chat_id": chat_id,
                "chat_title": chat_title,
                "chat_type": chat_type,
                "total_messages": total_messages,
                "time_range": {
                    "first": format_time(first_ts),
                    "last": format_time(last_ts),
                    "first_epoch": first_ts,
                    "last_epoch": last_ts,
                },
                "participants": author_stats.iter().map(|(uid, name, count)| {
                    serde_json::json!({
                        "user_id": uid,
                        "name": name,
                        "message_count": count,
                        "percentage": format!("{:.1}%", *count as f64 / total_messages as f64 * 100.0),
                    })
                }).collect::<Vec<_>>(),
                "message_types": type_stats.iter().map(|(t, label, count)| {
                    serde_json::json!({
                        "type_code": t,
                        "label": label,
                        "count": count,
                    })
                }).collect::<Vec<_>>(),
                "hourly_distribution": hourly_counts.iter().enumerate().map(|(h, &c)| {
                    serde_json::json!({"hour": h, "count": c})
                }).collect::<Vec<_>>(),
                "active_days": active_days,
                "avg_daily_messages": format!("{:.1}", avg_daily),
                "peak_hour": peak_hour,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
            return Ok(());
        }

        // Text output
        print_section_title(&format!(
            "Chat Stats: {} ({})",
            chat_title,
            type_label(&chat_type)
        ));
        println!();

        println!("  Total messages: {}", total_messages);
        println!(
            "  Time range:     {} → {}",
            format_time(first_ts),
            format_time(last_ts)
        );
        println!("  Active days:    {}", active_days);
        println!("  Avg daily:      {:.1} msgs/day", avg_daily);
        println!("  Peak hour:      {:02}:00–{:02}:59", peak_hour, peak_hour);
        println!();

        // Top participants
        print_section_title("Participants");
        let top_n = std::cmp::min(author_stats.len(), 15);
        let rows: Vec<Vec<String>> = author_stats[..top_n]
            .iter()
            .map(|(_, name, count)| {
                let pct = *count as f64 / total_messages as f64 * 100.0;
                let bar_len = (pct / 100.0 * 20.0) as usize;
                let bar = "█".repeat(bar_len);
                vec![
                    name.clone(),
                    count.to_string(),
                    format!("{:.1}%", pct),
                    bar,
                ]
            })
            .collect();
        print_table(&["Name", "Msgs", "%", ""], rows);
        println!();

        // Message types
        print_section_title("Message Types");
        let type_rows: Vec<Vec<String>> = type_stats
            .iter()
            .map(|(_, label, count)| {
                let pct = *count as f64 / total_messages as f64 * 100.0;
                vec![label.to_string(), count.to_string(), format!("{:.1}%", pct)]
            })
            .collect();
        print_table(&["Type", "Count", "%"], type_rows);
        println!();

        // Hourly distribution
        print_section_title("Hourly Activity");
        let max_hourly = *hourly_counts.iter().max().unwrap_or(&1);
        for (hour, &count) in hourly_counts.iter().enumerate() {
            let bar_len = if max_hourly > 0 {
                (count as f64 / max_hourly as f64 * 30.0) as usize
            } else {
                0
            };
            let bar = "▓".repeat(bar_len);
            println!("  {:02}:00  {:>5}  {}", hour, count, bar);
        }

        Ok(())
    })
}

pub fn cmd_cache(chat_id: i64, limit: Option<usize>, json: bool) -> Result<()> {
    let creds = get_creds()?;
    let db = message_db::MessageDb::open()?;

    let existing_cursor = db.get_sync_cursor(chat_id)?.unwrap_or(0);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = crate::loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

        let room_info = client
            .send_command("CHATONROOM", bson::doc! { "chatId": chat_id })
            .await?;
        if room_info.status() != 0 {
            anyhow::bail!("CHATONROOM failed (status={})", room_info.status());
        }

        let last_log_id = room_info.body.get_i64("l").unwrap_or(0);
        if last_log_id == 0 {
            eprintln!("No messages in this chat.");
            return Ok(());
        }

        // Build member name map
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

        let mut cur = existing_cursor;
        let mut synced = 0usize;
        let max_messages = limit.unwrap_or(usize::MAX);

        if existing_cursor > 0 {
            eprintln!("[cache] Resuming sync from logId={}", existing_cursor);
        } else {
            eprintln!("[cache] Starting full sync...");
        }

        loop {
            if synced >= max_messages {
                break;
            }

            let response = client
                .send_command(
                    "SYNCMSG",
                    bson::doc! {
                        "chatId": chat_id,
                        "cur": cur,
                        "cnt": 100_i32,
                        "max": last_log_id,
                    },
                )
                .await?;

            if response.status() != 0 {
                break;
            }

            let msgs = match response.body.get_array("chatLogs") {
                Ok(msgs) => msgs.clone(),
                Err(_) => break,
            };

            if msgs.is_empty() {
                break;
            }

            let mut batch: Vec<message_db::CachedMessage> = Vec::new();
            for msg in &msgs {
                if synced >= max_messages {
                    break;
                }
                let Some(doc) = msg.as_document() else {
                    continue;
                };

                let log_id = get_bson_i64(doc, &["logId"]);
                let author_id = get_bson_i64(doc, &["authorId"]);
                let msg_type = get_bson_i32(doc, &["type"]);
                let send_at = get_bson_i64(doc, &["sendAt"]);
                let message = doc.get_str("msg").unwrap_or("").to_string();
                let attachment = doc.get_str("attachment").unwrap_or("").to_string();
                let author_name = member_names.get(&author_id).cloned().unwrap_or_default();

                batch.push(message_db::CachedMessage {
                    chat_id,
                    log_id,
                    author_id,
                    author_name,
                    message_type: msg_type,
                    message,
                    attachment,
                    send_at,
                });

                cur = log_id;
                synced += 1;
            }

            db.upsert_messages(&batch)?;

            let is_ok = response.body.get_bool("isOK").unwrap_or(true);
            if is_ok {
                break;
            }

            if synced.is_multiple_of(500) {
                eprintln!("[cache] {} messages synced...", synced);
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        db.update_sync_cursor(chat_id, cur)?;

        if json {
            let output = serde_json::json!({
                "chat_id": chat_id,
                "synced": synced,
                "cursor": cur,
                "total_cached": db.total_count()?,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            eprintln!("[cache] Synced {} new messages (cursor={})", synced, cur);
            eprintln!("[cache] Total cached: {} messages", db.total_count()?);
        }

        Ok(())
    })
}

pub fn cmd_cache_search(query: &str, chat_id: Option<i64>, count: usize, json: bool) -> Result<()> {
    let db = message_db::MessageDb::open()?;

    let results = if let Some(cid) = chat_id {
        db.search(cid, query, count)?
    } else {
        db.search_all(query, count)?
    };

    if json {
        let output: Vec<serde_json::Value> = results
            .iter()
            .map(|m| {
                serde_json::json!({
                    "chat_id": m.chat_id,
                    "log_id": m.log_id,
                    "author_id": m.author_id,
                    "author_name": m.author_name,
                    "message_type": m.message_type,
                    "message": m.message,
                    "send_at": m.send_at,
                    "time": format_time(m.send_at),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if results.is_empty() {
        eprintln!("No cached messages matching '{}'.", query);
        return Ok(());
    }

    eprintln!(
        "Found {} cached messages matching '{}':",
        results.len(),
        query
    );
    println!();

    for m in &results {
        let time = format_time(m.send_at);
        let author = if m.author_name.is_empty() {
            format!("User#{}", m.author_id)
        } else {
            m.author_name.clone()
        };
        let type_tag = if m.message_type != 1 {
            format!(" [{}]", message_type_label(m.message_type))
        } else {
            String::new()
        };

        println!(
            "  {} [chat:{}] {}{}: {}",
            time,
            m.chat_id,
            author,
            type_tag,
            truncate(&m.message, 100)
        );
    }

    Ok(())
}

pub fn cmd_cache_stats(json: bool) -> Result<()> {
    let db = message_db::MessageDb::open()?;
    let total = db.total_count()?;
    let chat_stats = db.chat_stats()?;

    if json {
        let output = serde_json::json!({
            "total_messages": total,
            "chats": chat_stats.iter().map(|(cid, count, last_ts)| {
                serde_json::json!({
                    "chat_id": cid,
                    "message_count": count,
                    "last_message": format_time(*last_ts),
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    print_section_title("Local Message Cache");
    println!(
        "  Total: {} messages across {} chats",
        total,
        chat_stats.len()
    );
    println!();

    if !chat_stats.is_empty() {
        let rows: Vec<Vec<String>> = chat_stats
            .iter()
            .map(|(cid, count, last_ts)| {
                vec![cid.to_string(), count.to_string(), format_time(*last_ts)]
            })
            .collect();
        print_table(&["Chat ID", "Messages", "Last Msg"], rows);
    }

    Ok(())
}
