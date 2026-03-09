use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::Result;
use hmac::{Hmac, Mac};
use owo_colors::OwoColorize;
use serde_json::Value;
use sha2::Sha256;

use crate::loco_connect_with_auto_refresh;
use crate::media::{download_media_file, parse_attachment_url, sanitize_filename};
use crate::state::{
    auth_cooldown_remaining_secs, hook_remaining_secs, mark_hook_attempt, mark_webhook_attempt,
    record_failure, record_guard, record_transport_success, webhook_remaining_secs,
};
use crate::util::{
    color_enabled, get_bson_i64, get_bson_str_array, message_type_label,
    render_message_content, require_permission,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookFormat {
    Raw,
    Slack,
    Discord,
}

impl WebhookFormat {
    pub fn from_str_opt(s: Option<&str>) -> Result<Self> {
        match s {
            None | Some("raw") => Ok(Self::Raw),
            Some("slack") => Ok(Self::Slack),
            Some("discord") => Ok(Self::Discord),
            Some(other) => anyhow::bail!(
                "Unknown webhook format '{}'. Expected: raw, slack, discord",
                other
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatchHookConfig {
    pub command: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_headers: Vec<(String, String)>,
    pub webhook_signing_secret: Option<String>,
    pub webhook_format: WebhookFormat,
    pub chat_ids: Vec<i64>,
    pub keywords: Vec<String>,
    pub message_types: Vec<i32>,
    pub fail_fast: bool,
    pub min_hook_interval_secs: u64,
    pub min_webhook_interval_secs: u64,
    pub hook_timeout_secs: u64,
    pub webhook_timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub struct WatchOptions {
    pub unattended: bool,
    pub allow_side_effects: bool,
    pub filter_chat_id: Option<i64>,
    pub raw: bool,
    pub read_receipt: bool,
    pub max_reconnect: u32,
    pub download_media: bool,
    pub download_dir: String,
    pub hook_cmd: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_headers: Vec<String>,
    pub webhook_signing_secret: Option<String>,
    pub hook_chat_ids: Vec<i64>,
    pub hook_keywords: Vec<String>,
    pub hook_types: Vec<i32>,
    pub hook_fail_fast: bool,
    pub min_hook_interval_secs: u64,
    pub min_webhook_interval_secs: u64,
    pub hook_timeout_secs: u64,
    pub webhook_timeout_secs: u64,
    pub allow_insecure_webhooks: bool,
    pub webhook_format: WebhookFormat,
}

#[derive(Debug, Clone)]
pub struct WatchMessageEvent {
    pub event_type: &'static str,
    pub received_at: String,
    pub method: String,
    pub chat_id: i64,
    pub chat_name: String,
    pub log_id: i64,
    pub author_id: i64,
    pub author_nickname: String,
    pub message_type: i32,
    pub message: String,
    pub attachment: String,
}

impl WatchMessageEvent {
    pub fn as_json(&self) -> Value {
        serde_json::json!({
            "event_type": self.event_type,
            "received_at": self.received_at,
            "method": self.method,
            "chat_id": self.chat_id,
            "chat_name": self.chat_name,
            "log_id": self.log_id,
            "author_id": self.author_id,
            "author_nickname": self.author_nickname,
            "message_type": self.message_type,
            "message": self.message,
            "attachment": self.attachment,
        })
    }
}

pub fn watch_hook_matches(config: &WatchHookConfig, event: &WatchMessageEvent) -> bool {
    if !config.chat_ids.is_empty() && !config.chat_ids.contains(&event.chat_id) {
        return false;
    }

    if !config.message_types.is_empty() && !config.message_types.contains(&event.message_type) {
        return false;
    }

    if !config.keywords.is_empty() {
        let haystack = event.message.to_lowercase();
        if !config
            .keywords
            .iter()
            .any(|keyword| haystack.contains(&keyword.to_lowercase()))
        {
            return false;
        }
    }

    true
}

pub fn parse_webhook_header(header: &str) -> Result<(String, String)> {
    let (name, value) = header
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid webhook header, expected 'Name: Value'"))?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        anyhow::bail!("invalid webhook header, expected non-empty name and value");
    }
    Ok((name.to_string(), value.to_string()))
}

pub fn build_webhook_signature(secret: &str, timestamp: &str, payload: &[u8]) -> Result<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("failed to initialize webhook signer: {}", e))?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(payload);
    Ok(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ))
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

pub fn validate_webhook_url(webhook_url: &str, allow_insecure_webhooks: bool) -> Result<()> {
    let url = reqwest::Url::parse(webhook_url)
        .map_err(|e| anyhow::anyhow!("invalid webhook URL '{}': {}", webhook_url, e))?;
    match url.scheme() {
        "https" => Ok(()),
        "http" => {
            let host = url.host_str().unwrap_or_default();
            if is_loopback_host(host) || allow_insecure_webhooks {
                Ok(())
            } else {
                anyhow::bail!(
                    "refusing insecure webhook URL '{}'; use https or localhost, or opt in via config safety.allow_insecure_webhooks = true",
                    webhook_url
                )
            }
        }
        other => anyhow::bail!(
            "unsupported webhook URL scheme '{}'; use https or localhost http",
            other
        ),
    }
}

pub fn run_watch_command_hook(config: &WatchHookConfig, event: &WatchMessageEvent) -> Result<()> {
    let command = config
        .command
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing hook command"))?;
    if let Some(remaining) = hook_remaining_secs(config.min_hook_interval_secs)? {
        record_guard("hook_rate_limited")?;
        eprintln!(
            "[guard/hook] Skipping local hook for chat {} log {}: {}s rate-limit remaining.",
            event.chat_id, event.log_id, remaining
        );
        return Ok(());
    }
    mark_hook_attempt()?;
    let payload = serde_json::to_vec_pretty(&event.as_json())?;

    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .env("OPENKAKAO_EVENT_TYPE", event.event_type)
        .env("OPENKAKAO_CHAT_ID", event.chat_id.to_string())
        .env("OPENKAKAO_CHAT_NAME", &event.chat_name)
        .env("OPENKAKAO_LOG_ID", event.log_id.to_string())
        .env("OPENKAKAO_AUTHOR_ID", event.author_id.to_string())
        .env("OPENKAKAO_AUTHOR_NICKNAME", &event.author_nickname)
        .env("OPENKAKAO_MESSAGE_TYPE", event.message_type.to_string())
        .env(
            "OPENKAKAO_MESSAGE_TYPE_LABEL",
            message_type_label(event.message_type),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(&payload)?;
    }

    let timeout = Duration::from_secs(config.hook_timeout_secs.max(1));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow::anyhow!(
                "hook command timed out after {}s",
                config.hook_timeout_secs
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "hook command exited with status {}",
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string())
        ))
    }
}

pub fn run_watch_webhook(config: &WatchHookConfig, event: &WatchMessageEvent) -> Result<()> {
    let webhook_url = config
        .webhook_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing webhook url"))?;
    if let Some(remaining) = webhook_remaining_secs(config.min_webhook_interval_secs)? {
        record_guard("webhook_rate_limited")?;
        eprintln!(
            "[guard/webhook] Skipping webhook for chat {} log {}: {}s rate-limit remaining.",
            event.chat_id, event.log_id, remaining
        );
        return Ok(());
    }
    mark_webhook_attempt()?;
    let payload_json = match &config.webhook_format {
        WebhookFormat::Slack => {
            serde_json::json!({
                "text": format!(
                    "*[{}]* {}: {}",
                    event.chat_name, event.author_nickname, event.message
                ),
                "blocks": [
                    {
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": format!(
                                "*{}* in _{}_\n{}",
                                event.author_nickname, event.chat_name, event.message
                            )
                        }
                    },
                    {
                        "type": "context",
                        "elements": [
                            {
                                "type": "mrkdwn",
                                "text": format!(
                                    "chat:{} | log:{} | type:{}",
                                    event.chat_id, event.log_id,
                                    message_type_label(event.message_type)
                                )
                            }
                        ]
                    }
                ]
            })
        }
        WebhookFormat::Discord => {
            serde_json::json!({
                "content": format!(
                    "**[{}]** {}: {}",
                    event.chat_name, event.author_nickname, event.message
                ),
                "embeds": [
                    {
                        "title": format!("Message in {}", event.chat_name),
                        "description": event.message,
                        "color": 16764229,
                        "fields": [
                            {"name": "Author", "value": &event.author_nickname, "inline": true},
                            {"name": "Type", "value": message_type_label(event.message_type), "inline": true},
                            {"name": "Chat ID", "value": event.chat_id.to_string(), "inline": true}
                        ],
                        "timestamp": &event.received_at
                    }
                ]
            })
        }
        WebhookFormat::Raw => event.as_json(),
    };
    let payload = serde_json::to_vec(&payload_json)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(config.webhook_timeout_secs.max(1)))
        .build()?;
    let mut request = client
        .post(webhook_url)
        .header("Content-Type", "application/json");

    for (name, value) in &config.webhook_headers {
        request = request.header(name, value);
    }

    if let Some(secret) = &config.webhook_signing_secret {
        let timestamp = chrono::Utc::now().timestamp().to_string();
        let signature = build_webhook_signature(secret, &timestamp, &payload)?;
        request = request
            .header("X-OpenKakao-Timestamp", &timestamp)
            .header("X-OpenKakao-Signature", signature);
    }

    let response = request.body(payload).send()?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "webhook returned non-success status {}",
            response.status()
        ))
    }
}

pub fn cmd_watch(options: WatchOptions) -> Result<()> {
    if options.read_receipt || options.hook_cmd.is_some() || options.webhook_url.is_some() {
        require_permission(
            options.unattended && options.allow_side_effects,
            "watch side effects (read receipts, hooks, or webhooks)",
            "Re-run with --unattended --allow-watch-side-effects, or set both in ~/.config/openkakao/config.toml.",
        )?;
    }

    if let Some(webhook_url) = &options.webhook_url {
        validate_webhook_url(webhook_url, options.allow_insecure_webhooks)?;
    }

    let creds = crate::util::get_creds()?;
    let parsed_webhook_headers = options
        .webhook_headers
        .iter()
        .map(|header| parse_webhook_header(header))
        .collect::<Result<Vec<_>>>()?;
    let hook_config = if options.hook_cmd.is_some() || options.webhook_url.is_some() {
        Some(WatchHookConfig {
            command: options.hook_cmd.clone(),
            webhook_url: options.webhook_url.clone(),
            webhook_headers: parsed_webhook_headers,
            webhook_signing_secret: options.webhook_signing_secret.clone(),
            webhook_format: options.webhook_format.clone(),
            chat_ids: options.hook_chat_ids.clone(),
            keywords: options.hook_keywords.clone(),
            message_types: options.hook_types.clone(),
            fail_fast: options.hook_fail_fast,
            min_hook_interval_secs: options.min_hook_interval_secs,
            min_webhook_interval_secs: options.min_webhook_interval_secs,
            hook_timeout_secs: options.hook_timeout_secs,
            webhook_timeout_secs: options.webhook_timeout_secs,
        })
    } else {
        None
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = crate::loco::client::LocoClient::new(creds);
        let mut reconnect_count: u32 = 0;

        'reconnect: loop {
            let login_data = match loco_connect_with_auto_refresh(&mut client).await {
                Ok(data) => data,
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("cooling down")
                        || err_msg.contains("-950")
                        || err_msg.contains("-999")
                    {
                        record_failure("auth_relogin_needed")?;
                        let delay = auth_cooldown_remaining_secs()?.unwrap_or(30);
                        eprintln!(
                            "[watch] Auth recovery not ready: {}. Retrying in {}s...",
                            e, delay
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                        client.disconnect();
                        continue 'reconnect;
                    }
                    record_failure("network")?;
                    if options.max_reconnect == 0 || reconnect_count >= options.max_reconnect {
                        return Err(e);
                    }
                    reconnect_count += 1;
                    let delay = std::cmp::min(2u64.pow(reconnect_count), 32);
                    eprintln!(
                        "[watch] Connect failed: {}. Reconnecting in {}s ({}/{})...",
                        e, delay, reconnect_count, options.max_reconnect
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    client.disconnect();
                    continue 'reconnect;
                }
            };

            // Build chat_id → name map from LOGINLIST chatDatas
            let mut chat_names: HashMap<i64, String> = HashMap::new();
            if let Ok(chat_datas) = login_data.get_array("chatDatas") {
                for cd in chat_datas {
                    if let Some(doc) = cd.as_document() {
                        let cid = get_bson_i64(doc, &["c", "chatId"]);
                        if cid != 0 {
                            let name = doc
                                .get_document("chatInfo")
                                .ok()
                                .and_then(|ci| ci.get_str("name").ok())
                                .map(String::from)
                                .unwrap_or_default();
                            let name = if name.is_empty() {
                                get_bson_str_array(doc, &["k"]).join(", ")
                            } else {
                                name
                            };
                            if !name.is_empty() {
                                chat_names.insert(cid, name);
                            }
                        }
                    }
                }
            }

            let chat_count = chat_names.len();
            if reconnect_count > 0 {
                eprintln!(
                    "[watch] Reconnected! ({} chats loaded)",
                    chat_count
                );
            } else {
                eprintln!(
                    "[watch] Connected! Listening for messages... ({} chats loaded)",
                    chat_count
                );
                if let Some(cid) = options.filter_chat_id {
                    eprintln!("[watch] Filtering chat_id={}", cid);
                }
                if let Some(config) = &hook_config {
                    if let Some(command) = &config.command {
                        eprintln!("[watch] Hook command enabled: {}", command);
                    }
                    if let Some(webhook_url) = &config.webhook_url {
                        eprintln!("[watch] Webhook enabled: {}", webhook_url);
                    }
                }
                eprintln!("[watch] Press Ctrl-C to stop.");
            }
            reconnect_count = 0;
            record_transport_success("watch")?;

            let mut ping_interval =
                tokio::time::interval(std::time::Duration::from_secs(60));
            ping_interval.tick().await;

            loop {
                tokio::select! {
                    packet_result = client.recv_packet() => {
                        match packet_result {
                            Ok(packet) => {
                                let method = &packet.method;

                                if method == "CHANGESVR" {
                                    eprintln!("[watch] Server requested reconnect (CHANGESVR)");
                                    client.disconnect();
                                    continue 'reconnect;
                                }

                                if options.raw {
                                    let now = chrono::Local::now().format("%H:%M:%S");
                                    println!("[{}] {} {:?}", now, method, packet.body);
                                    continue;
                                }

                                match method.as_str() {
                                    "MSG" => {
                                        let chat_id = packet.body
                                            .get_i64("chatId")
                                            .or_else(|_| packet.body.get_i32("chatId").map(|v| v as i64))
                                            .unwrap_or(0);

                                        if let Some(filter) = options.filter_chat_id {
                                            if chat_id != filter {
                                                continue;
                                            }
                                        }

                                        let chat_label = chat_names
                                            .get(&chat_id)
                                            .cloned()
                                            .unwrap_or_else(|| format!("{}", chat_id));

                                        let nick = packet.body
                                            .get_str("authorNickname")
                                            .map(String::from)
                                            .unwrap_or_else(|_| {
                                                packet.body
                                                    .get_document("author")
                                                    .ok()
                                                    .and_then(|a| a.get_str("nickName").ok())
                                                    .map(String::from)
                                                    .unwrap_or_else(|| "???".to_string())
                                            });

                                        let msg_type = packet.body
                                            .get_i32("type")
                                            .unwrap_or(0);

                                        let content = render_message_content(&packet.body, msg_type);
                                        let log_id = packet.body
                                            .get_i64("logId")
                                            .or_else(|_| packet.body.get_i32("logId").map(|v| v as i64))
                                            .unwrap_or(0);
                                        let author_id = packet.body
                                            .get_i64("authorId")
                                            .or_else(|_| packet.body.get_i32("authorId").map(|v| v as i64))
                                            .unwrap_or(0);
                                        let attachment = packet.body
                                            .get_str("attachment")
                                            .unwrap_or("")
                                            .to_string();
                                        let event = WatchMessageEvent {
                                            event_type: "message",
                                            received_at: chrono::Utc::now().to_rfc3339(),
                                            method: method.clone(),
                                            chat_id,
                                            chat_name: chat_label.clone(),
                                            log_id,
                                            author_id,
                                            author_nickname: nick.clone(),
                                            message_type: msg_type,
                                            message: content.clone(),
                                            attachment: attachment.clone(),
                                        };

                                        let now = chrono::Local::now().format("%H:%M:%S");
                                        if color_enabled() {
                                            println!(
                                                "{} {} {}: {}",
                                                format!("[{}]", now).dimmed(),
                                                format!("[{}]", chat_label).cyan(),
                                                nick.bold(),
                                                content
                                            );
                                        } else {
                                            println!(
                                                "[{}] [{}] {}: {}",
                                                now, chat_label, nick, content
                                            );
                                        }

                                        if let Some(config) = &hook_config {
                                            if watch_hook_matches(config, &event) {
                                                if config.command.is_some() {
                                                    match tokio::task::spawn_blocking({
                                                        let config = config.clone();
                                                        let event = event.clone();
                                                        move || run_watch_command_hook(&config, &event)
                                                    })
                                                    .await
                                                    {
                                                        Ok(Ok(())) => {}
                                                        Ok(Err(e)) => {
                                                            eprintln!("[watch] Hook failed: {}", e);
                                                            if config.fail_fast {
                                                                return Err(e);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            let err = anyhow::anyhow!("hook task join error: {}", e);
                                                            eprintln!("[watch] Hook failed: {}", err);
                                                            if config.fail_fast {
                                                                return Err(err);
                                                            }
                                                        }
                                                    }
                                                }
                                                if config.webhook_url.is_some() {
                                                    match tokio::task::spawn_blocking({
                                                        let config = config.clone();
                                                        let event = event.clone();
                                                        move || run_watch_webhook(&config, &event)
                                                    })
                                                    .await
                                                    {
                                                        Ok(Ok(())) => {}
                                                        Ok(Err(e)) => {
                                                            eprintln!("[watch] Webhook failed: {}", e);
                                                            if config.fail_fast {
                                                                return Err(e);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            let err = anyhow::anyhow!("webhook task join error: {}", e);
                                                            eprintln!("[watch] Webhook failed: {}", err);
                                                            if config.fail_fast {
                                                                return Err(err);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // Send read receipt if enabled
                                        if options.read_receipt && log_id > 0 {
                                            let _ = client.send_packet("NOTIREAD", bson::doc! {
                                                "chatId": chat_id,
                                                "watermark": log_id,
                                            }).await;
                                        }

                                        // Auto-download media if enabled
                                        if options.download_media
                                            && matches!(msg_type, 2 | 3 | 12 | 14 | 26 | 27)
                                            && !attachment.is_empty()
                                        {
                                                let dl_creds = client.credentials.clone();
                                                let dl_dir = options.download_dir.clone();
                                                tokio::task::spawn_blocking(move || {
                                                    if let Some((url, filename)) = parse_attachment_url(&attachment, msg_type) {
                                                        let dir = Path::new(&dl_dir).join(chat_id.to_string());
                                                        let save_name = format!("{}_{}", log_id, sanitize_filename(&filename));
                                                        let save_path = dir.join(&save_name);
                                                        match download_media_file(&dl_creds, &url, &save_path) {
                                                            Ok(bytes) => {
                                                                eprintln!(
                                                                    "[watch] Downloaded {} ({} bytes)",
                                                                    save_path.display(), bytes
                                                                );
                                                            }
                                                            Err(e) => {
                                                                eprintln!(
                                                                    "[watch] Download failed for {}: {}",
                                                                    save_name, e
                                                                );
                                                            }
                                                        }
                                                    }
                                                });
                                        }
                                    }
                                    "DECUNREAD" | "NOTIREAD" | "SYNCLINKCR" | "SYNCLINKUP"
                                    | "SYNCMSG" | "SYNCDLMSG" => {
                                        // Known push events, silently ignore
                                    }
                                    _ => {
                                        eprintln!("[watch] Push: {} (status={})", method, packet.status());
                                    }
                                }
                            }
                            Err(e) => {
                                let err_msg = e.to_string();
                                if err_msg.contains("-950") || err_msg.contains("-999") {
                                    record_failure("auth_relogin_needed")?;
                                    let delay = auth_cooldown_remaining_secs()?.unwrap_or(30);
                                    eprintln!(
                                        "[watch] Auth error: {}. Retrying in {}s...",
                                        e, delay
                                    );
                                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                                    client.disconnect();
                                    continue 'reconnect;
                                }
                                record_failure("network")?;
                                if options.max_reconnect == 0 {
                                    eprintln!("[watch] Connection lost: {}", e);
                                    return Err(e);
                                }
                                reconnect_count += 1;
                                if reconnect_count > options.max_reconnect {
                                    eprintln!(
                                        "[watch] Connection lost after {} reconnect attempts: {}",
                                        options.max_reconnect, e
                                    );
                                    return Err(e);
                                }
                                let delay = std::cmp::min(2u64.pow(reconnect_count), 32);
                                eprintln!(
                                    "[watch] Connection lost: {}. Reconnecting in {}s ({}/{})...",
                                    e, delay, reconnect_count, options.max_reconnect
                                );
                                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                                client.disconnect();
                                continue 'reconnect;
                            }
                        }
                    }
                    _ = ping_interval.tick() => {
                        if let Err(e) = client.send_packet("PING", bson::doc! {}).await {
                            record_failure("network")?;
                            eprintln!("[watch] PING failed: {}", e);
                            if options.max_reconnect == 0 {
                                return Err(anyhow::anyhow!("PING failed: {}", e));
                            }
                            reconnect_count += 1;
                            if reconnect_count > options.max_reconnect {
                                return Err(anyhow::anyhow!(
                                    "PING failed after {} reconnects: {}", options.max_reconnect, e
                                ));
                            }
                            let delay = std::cmp::min(2u64.pow(reconnect_count), 32);
                            eprintln!(
                                "[watch] Reconnecting in {}s ({}/{})...",
                                delay, reconnect_count, options.max_reconnect
                            );
                            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                            client.disconnect();
                            continue 'reconnect;
                        }
                    }
                    _ = tokio::signal::ctrl_c() => {
                        eprintln!("\n[watch] Shutting down...");
                        return Ok(());
                    }
                }
            }
        }
    })
}
