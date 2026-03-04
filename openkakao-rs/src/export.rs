use std::fs::File;
use std::io::{self, Write};

use anyhow::{anyhow, Result};
use chrono::{Local, TimeZone};

use crate::model::{ChatMember, ChatMessage};

pub enum ExportFormat {
    Json,
    Csv,
    Txt,
}

impl ExportFormat {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "csv" => Ok(Self::Csv),
            "txt" => Ok(Self::Txt),
            _ => Err(anyhow!("Unknown format '{}'. Use: json, csv, txt", s)),
        }
    }
}

pub fn export_messages(
    messages: &[ChatMessage],
    members: &[ChatMember],
    my_user_id: i64,
    format: &ExportFormat,
    output: Option<&str>,
) -> Result<()> {
    let content = match format {
        ExportFormat::Json => format_json(messages, members, my_user_id)?,
        ExportFormat::Csv => format_csv(messages, members, my_user_id)?,
        ExportFormat::Txt => format_txt(messages, members, my_user_id),
    };

    match output {
        Some(path) => {
            let mut file = File::create(path)?;
            file.write_all(content.as_bytes())?;
        }
        None => {
            io::stdout().write_all(content.as_bytes())?;
        }
    }

    Ok(())
}

fn resolve_author(author_id: i64, members: &[ChatMember], my_user_id: i64) -> String {
    if author_id == my_user_id {
        return "Me".to_string();
    }
    members
        .iter()
        .find(|m| m.user_id == author_id)
        .map(|m| m.display_name())
        .unwrap_or_else(|| author_id.to_string())
}

fn format_json(
    messages: &[ChatMessage],
    members: &[ChatMember],
    my_user_id: i64,
) -> Result<String> {
    let entries: Vec<serde_json::Value> = messages
        .iter()
        .map(|msg| {
            serde_json::json!({
                "log_id": msg.log_id,
                "author": resolve_author(msg.author_id, members, my_user_id),
                "message_type": msg.message_type,
                "message": msg.message,
                "attachment": msg.attachment,
                "send_at": msg.send_at,
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&entries)?)
}

fn format_csv(messages: &[ChatMessage], members: &[ChatMember], my_user_id: i64) -> Result<String> {
    let mut buf = Vec::new();
    {
        let mut wtr = csv::Writer::from_writer(&mut buf);
        wtr.write_record([
            "log_id",
            "author",
            "message_type",
            "message",
            "attachment",
            "send_at",
        ])?;
        for msg in messages {
            wtr.write_record(&[
                msg.log_id.to_string(),
                resolve_author(msg.author_id, members, my_user_id),
                msg.message_type.to_string(),
                msg.message.clone(),
                msg.attachment.clone(),
                msg.send_at.to_string(),
            ])?;
        }
        wtr.flush()?;
    }
    Ok(String::from_utf8(buf)?)
}

fn format_txt(messages: &[ChatMessage], members: &[ChatMember], my_user_id: i64) -> String {
    let mut lines = Vec::new();
    for msg in messages {
        let author = resolve_author(msg.author_id, members, my_user_id);
        let time_str = Local
            .timestamp_opt(msg.send_at, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| msg.send_at.to_string());
        lines.push(format!("[{}] {}: {}", time_str, author, msg.message));
    }
    let mut result = lines.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    result
}
