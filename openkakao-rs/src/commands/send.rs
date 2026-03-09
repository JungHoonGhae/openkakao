use std::path::Path;

use anyhow::Result;

use crate::error::OpenKakaoError;
use crate::loco_helpers::{check_loco_status, loco_connect_with_auto_refresh};
use crate::media::{detect_media_type, jpeg_dimensions, png_dimensions};
use crate::state::{mark_unattended_send_attempt, record_guard, unattended_send_remaining_secs};
use crate::util::{
    confirm, extract_chat_type, get_creds, is_open_chat, require_permission, truncate, type_label,
    validate_outbound_message,
};

#[allow(clippy::too_many_arguments)]
pub fn cmd_send(
    chat_id: i64,
    message: &str,
    force: bool,
    skip_confirm: bool,
    unattended: bool,
    allow_non_interactive_send: bool,
    min_unattended_send_interval_secs: u64,
    json: bool,
) -> Result<()> {
    validate_outbound_message(message)?;
    if skip_confirm {
        require_permission(
            unattended && allow_non_interactive_send,
            "non-interactive send (-y/--yes)",
            "Re-run with --unattended --allow-non-interactive-send, or set both in ~/.config/openkakao/config.toml.",
        )?;
        if let Some(remaining) = unattended_send_remaining_secs(min_unattended_send_interval_secs)?
        {
            record_guard("unattended_send_rate_limited")?;
            anyhow::bail!(
                "unattended send is rate-limited for {}s; wait or raise safety.min_unattended_send_interval_secs",
                remaining
            );
        }
        mark_unattended_send_attempt()?;
    }
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = crate::loco::client::LocoClient::new(creds);
        eprintln!("Connecting via LOCO...");
        loco_connect_with_auto_refresh(&mut client).await?;

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
            return Err(OpenKakaoError::SafetyBlock(
                "Open chat send blocked (use --force to override)".into(),
            )
            .into());
        }

        if is_open_chat(&chat_type) {
            eprintln!(
                "Warning: sending to {} (open chat). Proceed with caution.",
                label
            );
        }

        if !skip_confirm {
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

        check_loco_status("WRITE", &response)?;

        let log_id = response.body.get_i64("logId").unwrap_or(0);
        if json {
            crate::util::output_json(&serde_json::json!({
                "chat_id": chat_id,
                "log_id": log_id,
                "status": "sent",
            }))?;
        } else {
            println!("Message sent!");
        }

        Ok(())
    })
}

#[allow(clippy::too_many_arguments)]
pub fn cmd_send_file(
    chat_id: i64,
    file_path: &str,
    force: bool,
    skip_confirm: bool,
    unattended: bool,
    allow_non_interactive_send: bool,
    min_unattended_send_interval_secs: u64,
    json: bool,
) -> Result<()> {
    if skip_confirm {
        require_permission(
            unattended && allow_non_interactive_send,
            "non-interactive file send (-y/--yes)",
            "Re-run with --unattended --allow-non-interactive-send, or set both in ~/.config/openkakao/config.toml.",
        )?;
        if let Some(remaining) = unattended_send_remaining_secs(min_unattended_send_interval_secs)?
        {
            record_guard("unattended_send_rate_limited")?;
            anyhow::bail!(
                "unattended send is rate-limited for {}s; wait or raise safety.min_unattended_send_interval_secs",
                remaining
            );
        }
        mark_unattended_send_attempt()?;
    }
    let path = Path::new(file_path);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file_path);
    }

    let data = std::fs::read(path)?;
    if data.len() < 4 {
        anyhow::bail!("File too small: {} bytes", data.len());
    }

    let file_ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let (msg_type, ext_str) = detect_media_type(&data, &file_ext);
    let ext = ext_str.as_str();

    let type_label_str = match msg_type {
        2 => "photo",
        3 => "video",
        14 => "gif",
        26 => "file",
        _ => "file",
    };

    let (width, height) = match (msg_type, ext) {
        (2, "jpg") => jpeg_dimensions(&data).unwrap_or((0, 0)),
        (2, "png") => png_dimensions(&data).unwrap_or((0, 0)),
        _ => (0, 0),
    };

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("upload.{}", ext));

    eprintln!(
        "{}: {} ({} bytes, {}x{}, type={})",
        type_label_str,
        file_name,
        data.len(),
        width,
        height,
        msg_type
    );

    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = crate::loco::client::LocoClient::new(creds.clone());
        eprintln!("Connecting via LOCO...");
        loco_connect_with_auto_refresh(&mut client).await?;

        let room_info = client
            .send_command("CHATONROOM", bson::doc! { "chatId": chat_id })
            .await?;
        let chat_type = extract_chat_type(&room_info.body);
        let label = type_label(&chat_type);

        if is_open_chat(&chat_type) && !force {
            return Err(OpenKakaoError::SafetyBlock(format!(
                "Blocked: chat {} is {} (open chat). Use --force to override.",
                chat_id, label
            ))
            .into());
        }

        if !skip_confirm {
            eprint!(
                "Send {} ({}) to {} chat {}?\n[y/N] ",
                file_name, type_label_str, label, chat_id
            );
            if !confirm()? {
                println!("Cancelled.");
                return Ok(());
            }
        }

        let checksum = {
            use sha1::Digest;
            let hash = sha1::Sha1::digest(&data);
            hex::encode(hash)
        };

        let ship_resp = client
            .send_command(
                "SHIP",
                bson::doc! {
                    "c": chat_id,
                    "s": data.len() as i64,
                    "t": msg_type,
                    "cs": &checksum,
                    "e": ext,
                    "ex": "{}",
                },
            )
            .await?;

        check_loco_status("SHIP", &ship_resp)?;

        let upload_key = ship_resp
            .body
            .get_str("k")
            .map_err(|_| anyhow::anyhow!("No key in SHIP response"))?
            .to_string();
        let vhost = ship_resp
            .body
            .get_str("vh")
            .map_err(|_| anyhow::anyhow!("No vhost in SHIP response"))?
            .to_string();
        let upload_port = ship_resp.body.get_i32("p").map(|p| p as u16).unwrap_or(443);

        eprintln!(
            "[ship] Upload server: {}:{}, key: {}",
            vhost, upload_port, upload_key
        );

        crate::loco::client::loco_upload(
            &vhost,
            upload_port,
            creds.user_id,
            &upload_key,
            chat_id,
            &data,
            msg_type,
            width,
            height,
            &creds.app_version,
        )
        .await?;

        if json {
            crate::util::output_json(&serde_json::json!({
                "chat_id": chat_id,
                "file": file_name,
                "type": type_label_str,
                "status": "sent",
            }))?;
        } else {
            println!(
                "{} sent!",
                type_label_str[..1].to_uppercase() + &type_label_str[1..]
            );
        }
        Ok(())
    })
}
