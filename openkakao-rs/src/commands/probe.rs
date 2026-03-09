use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::loco;
use crate::loco_helpers::{
    loco_connect_with_auto_refresh, reconnect_loco_probe_client, should_retry_loco_probe_error,
};
use crate::util::{get_creds, print_section_title};

#[derive(Debug, Clone, Serialize)]
pub struct MethodProbeResult {
    pub method: String,
    pub body: serde_json::Value,
    pub packet_status_code: i16,
    pub body_status: Option<i32>,
    pub push_count: usize,
    pub push_methods: Vec<String>,
}

pub fn parse_loco_probe_body(body: Option<&str>) -> Result<bson::Document> {
    let Some(raw) = body else {
        return Ok(bson::Document::new());
    };

    let value: serde_json::Value =
        serde_json::from_str(raw).context("probe body must be valid JSON")?;
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("probe body must be a JSON object"))?;

    bson::to_document(object).context("failed to convert probe JSON to BSON document")
}

pub fn cmd_loco_probe(method: &str, body: Option<&str>, json: bool) -> Result<()> {
    let creds = get_creds()?;
    let method = method.to_uppercase();
    let request_body = parse_loco_probe_body(body)?;
    let request_json = serde_json::to_value(&request_body)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

        let result = client
            .send_command_collect(&method, request_body.clone(), Duration::from_secs(3))
            .await?;
        let response_json = result
            .response
            .as_ref()
            .map(|response| {
                Ok::<_, anyhow::Error>(serde_json::json!({
                    "method": response.method,
                    "packet_id": response.packet_id,
                    "status_code": response.status_code,
                    "status": response.status(),
                    "body_type": response.body_type,
                    "body": serde_json::to_value(&response.body)?,
                }))
            })
            .transpose()?;
        let pushes_json = result
            .pushes
            .iter()
            .map(|packet| {
                Ok::<_, anyhow::Error>(serde_json::json!({
                    "method": packet.method,
                    "packet_id": packet.packet_id,
                    "status_code": packet.status_code,
                    "status": packet.status(),
                    "body_type": packet.body_type,
                    "body": serde_json::to_value(&packet.body)?,
                }))
            })
            .collect::<Result<Vec<_>>>()?;

        let payload = serde_json::json!({
            "request": {
                "method": &method,
                "body": request_json,
            },
            "response_present": response_json.is_some(),
            "push_count": pushes_json.len(),
            "empty_within_timeout": response_json.is_none() && pushes_json.is_empty(),
            "response": response_json,
            "pushes": pushes_json,
        });

        if json {
            println!("{}", serde_json::to_string_pretty(&payload)?);
        } else {
            print_section_title(&format!("LOCO probe: {}", method));
            if let Some(response) = &result.response {
                println!("  status: {}", response.status());
                println!("  packet: {}", response.packet_id);
                println!("{}", serde_json::to_string_pretty(&payload["response"])?);
            } else {
                println!("  response: <none> (no direct response within timeout)");
            }
            if !result.pushes.is_empty() {
                println!("  pushes: {}", result.pushes.len());
                println!("{}", serde_json::to_string_pretty(&payload["pushes"])?);
            } else if result.response.is_none() {
                println!("  pushes: <none> (no push packets within timeout)");
            }
        }

        Ok(())
    })
}

pub fn cmd_loco_chatinfo(chat_id: i64, json: bool) -> Result<()> {
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

        // Special: chat_id=0 → find or create MemoChat ("나와의 채팅")
        if chat_id == 0 {
            eprintln!("Finding MemoChat (나와의 채팅)...");

            // First scan LOGINLIST chatDatas for existing MemoChat
            let login_data = client.full_connect_with_retry(3).await?;
            if let Ok(chat_datas) = login_data.get_array("chatDatas") {
                for cd in chat_datas {
                    if let Some(doc) = cd.as_document() {
                        let ctype = doc.get_str("t").unwrap_or("?");
                        if ctype == "MemoChat" {
                            let cid = doc
                                .get_i64("c")
                                .or_else(|_| doc.get_i32("c").map(|v| v as i64))
                                .unwrap_or(0);
                            println!("Memo chat ID: {}", cid);
                            return Ok(());
                        }
                    }
                }
            }

            // Not found — create one via CREATE with memoChat=true (node-kakao pattern)
            eprintln!("No existing MemoChat found, creating...");
            let resp = client
                .send_command(
                    "CREATE",
                    bson::doc! {
                        "memberIds": bson::Bson::Array(vec![]),
                        "memoChat": true,
                    },
                )
                .await?;

            let status = resp.status();
            if status == 0 {
                let memo_id = resp
                    .body
                    .get_i64("chatId")
                    .or_else(|_| resp.body.get_i32("chatId").map(|v| v as i64))
                    .unwrap_or(0);
                println!("MemoChat created! ID: {}", memo_id);
            } else {
                eprintln!("CREATE MemoChat failed (status={})", status);
                eprintln!("Response: {:?}", resp.body);
            }
            return Ok(());
        }

        let response = client
            .send_command("CHATINFO", bson::doc! { "chatId": chat_id })
            .await?;

        if response.status() != 0 {
            anyhow::bail!("CHATINFO failed (status={})", response.status());
        }

        if json {
            // Convert BSON body to JSON
            let json_val: serde_json::Value = bson::from_document(response.body.clone())?;
            println!("{}", serde_json::to_string_pretty(&json_val)?);
        } else {
            print_section_title(&format!("Chat info: {}", chat_id));
            for (k, v) in response.body.iter() {
                let v_str = format!("{:?}", v);
                if v_str.len() > 100 {
                    println!("  {}: {}...", k, &v_str[..100]);
                } else {
                    println!("  {}: {}", k, v_str);
                }
            }
        }

        Ok(())
    })
}

pub async fn probe_method_variants(
    method: &str,
    variants: &[serde_json::Value],
) -> Result<Vec<MethodProbeResult>> {
    let creds = get_creds()?;
    let mut client = loco::client::LocoClient::new(creds);
    reconnect_loco_probe_client(&mut client).await?;

    let mut results = Vec::new();
    for variant in variants {
        let object = variant
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("{method} probe body must be a JSON object"))?;
        let body = bson::to_document(object)?;
        let result = match client
            .send_command_collect(method, body.clone(), Duration::from_secs(2))
            .await
        {
            Ok(result) => result,
            Err(error) if should_retry_loco_probe_error(&error) => {
                reconnect_loco_probe_client(&mut client).await?;
                client
                    .send_command_collect(method, body, Duration::from_secs(2))
                    .await?
            }
            Err(error) => return Err(error),
        };
        let packet_status_code = result
            .response
            .as_ref()
            .map(|p| p.status_code)
            .unwrap_or(-1);
        let body_status = result.response.as_ref().and_then(|packet| {
            packet
                .body
                .get_i32("status")
                .ok()
                .or_else(|| packet.body.get_i64("status").ok().map(|value| value as i32))
        });
        let push_methods = result
            .pushes
            .iter()
            .map(|packet| packet.method.clone())
            .collect::<Vec<_>>();
        results.push(MethodProbeResult {
            method: method.to_string(),
            body: variant.clone(),
            packet_status_code,
            body_status,
            push_count: result.pushes.len(),
            push_methods,
        });
    }

    Ok(results)
}
