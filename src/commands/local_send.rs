use anyhow::Result;

use crate::ax_send;
use crate::util::{confirm, truncate, validate_outbound_message};

pub struct LocalSendOptions {
    pub chat_name: String,
    pub message: String,
    pub skip_confirm: bool,
    pub dry_run: bool,
    pub json: bool,
}

/// Send a message via AX automation (drives the real KakaoTalk window) —
/// no LOCO/REST session and no local SQLCipher DB access required. Both the
/// chat lookup and the post-send delivery check happen entirely through the
/// Accessibility tree. See `src/ax_send.rs` for the mechanism.
pub fn cmd_local_send(opts: LocalSendOptions) -> Result<()> {
    let LocalSendOptions {
        ref chat_name,
        ref message,
        skip_confirm,
        dry_run,
        json,
    } = opts;
    validate_outbound_message(message)?;

    if dry_run {
        eprintln!(
            "[dry-run] Would AX-send to chat \"{}\": \"{}\"",
            chat_name,
            truncate(message, 80)
        );
        if json {
            crate::util::output_json(&serde_json::json!({
                "dry_run": true,
                "action": "local_send",
                "chat_name": chat_name,
                "message": message,
            }))?;
        }
        return Ok(());
    }

    if !skip_confirm {
        eprint!(
            "AX-send to chat \"{}\"? Message: \"{}\"\n[y/N] ",
            chat_name,
            truncate(message, 50)
        );
        if !confirm()? {
            println!("Cancelled.");
            return Ok(());
        }
    }

    ax_send::send_via_ax(chat_name, message)?;

    if json {
        crate::util::output_json(&serde_json::json!({
            "chat_name": chat_name,
            "status": "sent",
        }))?;
    } else {
        println!("Message sent!");
    }

    Ok(())
}
