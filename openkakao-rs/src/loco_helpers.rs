use anyhow::Result;

use crate::auth_flow::connect_loco_with_reauth;
use crate::loco;
use crate::rest::KakaoRestClient;
use crate::model::KakaoCredentials;
use crate::util::{parse_loco_status_from_error, print_loco_error_hint};

/// Connect LOCO client and login, auto-refreshing token on -950.
pub async fn loco_connect_with_auto_refresh(
    client: &mut loco::client::LocoClient,
) -> Result<bson::Document> {
    match connect_loco_with_reauth(client).await {
        Ok(data) => Ok(data),
        Err(e) => {
            if let Some(status) = parse_loco_status_from_error(&e.to_string()) {
                print_loco_error_hint(status);
            }
            Err(e)
        }
    }
}

/// Try to renew token via REST API. Returns the new access_token if successful.
pub fn try_renew_token(creds: &KakaoCredentials, refresh_token: &str) -> Result<Option<String>> {
    let rest = KakaoRestClient::new(creds.clone())?;

    // Try oauth2_token.json first (sends both access_token + refresh_token)
    eprintln!("[renew] Trying oauth2_token.json...");
    if let Ok(response) = rest.oauth2_token(refresh_token) {
        let status = response.get("status").and_then(serde_json::Value::as_i64).unwrap_or(-1);
        eprintln!("[renew] oauth2_token.json status: {}", status);
        if status == 0 {
            return Ok(response
                .get("access_token")
                .and_then(serde_json::Value::as_str)
                .map(String::from));
        }
    }

    // Fallback: try renew_token.json
    eprintln!("[renew] Trying renew_token.json...");
    let response = rest.renew_token(refresh_token)?;
    let status = response.get("status").and_then(serde_json::Value::as_i64).unwrap_or(-1);
    eprintln!("[renew] renew_token.json status: {}", status);

    if let Some(obj) = response.as_object() {
        for (k, v) in obj {
            let v_str = format!("{}", v);
            if v_str.len() > 60 {
                eprintln!("  {}: {}...", k, &v_str[..60]);
            } else {
                eprintln!("  {}: {}", k, v);
            }
        }
    }

    if status != 0 {
        return Ok(None);
    }

    Ok(response
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(String::from))
}

/// Whether a LOCO probe error should be retried (transient socket failures).
pub fn should_retry_loco_probe_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("early eof")
        || message.contains("connection reset by peer")
        || message.contains("broken pipe")
        || message.contains("os error 54")
}

/// Reconnect a LOCO client for probing, retrying on transient errors.
pub async fn reconnect_loco_probe_client(client: &mut loco::client::LocoClient) -> Result<()> {
    let mut last_error = None;
    for _ in 0..3 {
        match loco_connect_with_auto_refresh(client).await {
            Ok(_) => return Ok(()),
            Err(error) if error.to_string().contains("status=-300") => {
                last_error = Some(error);
                continue;
            }
            Err(error) => return Err(error),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("LOCO probe reconnect failed")))
}

