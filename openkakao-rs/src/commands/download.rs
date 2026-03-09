use std::path::Path;

use anyhow::Result;

use crate::loco_helpers::{check_loco_status, loco_connect_with_auto_refresh};
use crate::media::{download_media_file, parse_attachment_url, sanitize_filename};
use crate::util::{get_bson_i32, get_bson_i64, get_bson_str, get_creds, truncate};

pub fn cmd_download(chat_id: i64, log_id: i64, output_dir: Option<&str>) -> Result<()> {
    let creds = get_creds()?;
    let out_dir = output_dir.unwrap_or("downloads");

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = crate::loco::client::LocoClient::new(creds.clone());
        eprintln!("Connecting via LOCO...");
        loco_connect_with_auto_refresh(&mut client).await?;

        // Get lastLogId via CHATONROOM (required as max for SYNCMSG)
        let room_info = client
            .send_command("CHATONROOM", bson::doc! { "chatId": chat_id })
            .await?;
        check_loco_status("CHATONROOM", &room_info)?;
        let last_log_id = room_info.body.get_i64("l").unwrap_or(0);

        // Scan via SYNCMSG pagination to find the target message.
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

            check_loco_status("SYNCMSG", &response)?;

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
                let save_name = format!("{}_{}", log_id, sanitize_filename(&filename));
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
