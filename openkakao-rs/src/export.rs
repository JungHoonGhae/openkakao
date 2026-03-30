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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChatMember, ChatMessage};

    fn make_msg(log_id: i64, author_id: i64, text: &str) -> ChatMessage {
        ChatMessage {
            log_id,
            author_id,
            message_type: 1,
            message: text.to_string(),
            attachment: String::new(),
            send_at: 1_700_000_000,
        }
    }

    fn make_member(user_id: i64, nickname: &str) -> ChatMember {
        ChatMember {
            user_id,
            nickname: nickname.to_string(),
            friend_nickname: String::new(),
            country_iso: String::new(),
        }
    }

    // ── ExportFormat::from_str ─────────────────────────────────────────────

    #[test]
    fn export_format_from_str_valid_formats() {
        assert!(matches!(
            ExportFormat::from_str("json"),
            Ok(ExportFormat::Json)
        ));
        assert!(matches!(
            ExportFormat::from_str("JSON"),
            Ok(ExportFormat::Json)
        ));
        assert!(matches!(
            ExportFormat::from_str("csv"),
            Ok(ExportFormat::Csv)
        ));
        assert!(matches!(
            ExportFormat::from_str("txt"),
            Ok(ExportFormat::Txt)
        ));
    }

    #[test]
    fn export_format_from_str_invalid_returns_error() {
        assert!(ExportFormat::from_str("xml").is_err());
        assert!(ExportFormat::from_str("").is_err());
    }

    // ── resolve_author ─────────────────────────────────────────────────────

    #[test]
    fn resolve_author_self_returns_me() {
        let members = vec![make_member(42, "Alice")];
        assert_eq!(resolve_author(1, &members, 1), "Me");
    }

    #[test]
    fn resolve_author_known_member_returns_display_name() {
        let members = vec![make_member(42, "Alice")];
        assert_eq!(resolve_author(42, &members, 1), "Alice");
    }

    #[test]
    fn resolve_author_unknown_returns_id_string() {
        let members: Vec<ChatMember> = vec![];
        assert_eq!(resolve_author(999, &members, 1), "999");
    }

    // ── format_json ────────────────────────────────────────────────────────

    #[test]
    fn format_json_empty_messages_returns_empty_array() {
        let result = format_json(&[], &[], 1).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 0);
    }

    #[test]
    fn format_json_includes_expected_fields() {
        let msgs = vec![make_msg(101, 1, "hello")];
        let result = format_json(&msgs, &[], 1).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let entry = &parsed[0];
        assert_eq!(entry["log_id"], 101);
        assert_eq!(entry["author"], "Me");
        assert_eq!(entry["message"], "hello");
    }

    // ── format_csv ─────────────────────────────────────────────────────────

    #[test]
    fn format_csv_empty_messages_has_header_only() {
        let result = format_csv(&[], &[], 1).unwrap();
        let first_line = result.lines().next().unwrap_or("");
        assert!(first_line.contains("log_id"));
        assert!(first_line.contains("author"));
        assert!(first_line.contains("message"));
    }

    #[test]
    fn format_csv_with_message_has_data_row() {
        let msgs = vec![make_msg(55, 1, "test msg")];
        let result = format_csv(&msgs, &[], 1).unwrap();
        assert!(result.contains("55"));
        assert!(result.contains("Me"));
        assert!(result.contains("test msg"));
    }

    // ── format_txt ─────────────────────────────────────────────────────────

    #[test]
    fn format_txt_empty_messages_returns_empty_string() {
        let result = format_txt(&[], &[], 1);
        assert!(result.is_empty());
    }

    #[test]
    fn format_txt_includes_author_and_message() {
        let msgs = vec![make_msg(1, 1, "world")];
        let result = format_txt(&msgs, &[], 1);
        assert!(result.contains("Me"));
        assert!(result.contains("world"));
    }
}
