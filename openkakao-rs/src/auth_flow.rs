use anyhow::{anyhow, Result};
use bson::Document;
use serde_json::Value;

use crate::auth::{
    extract_login_params, extract_refresh_token, get_credential_candidates,
    get_credentials_interactive,
};
use crate::credentials::{load_credentials, save_credentials};
use crate::loco::client::LocoClient;
use crate::model::KakaoCredentials;
use crate::rest::KakaoRestClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Rest,
    Loco,
}

impl Transport {
    pub fn default_recovery_order(self) -> &'static [&'static str] {
        match self {
            Self::Rest => &[
                "saved credentials",
                "login.json relogin",
                "refresh_token renewal",
                "Cache.db extraction",
            ],
            Self::Loco => &[
                "saved credentials",
                "login.json relogin",
                "refresh_token renewal",
                "Cache.db extraction",
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub struct RefreshResult {
    pub response: Value,
    pub credentials: Option<KakaoCredentials>,
    pub source: &'static str,
}

pub fn resolve_base_credentials() -> Result<KakaoCredentials> {
    if let Some(saved) = load_credentials()? {
        return Ok(saved);
    }

    let candidates = get_credential_candidates(8)?;
    if !candidates.is_empty() {
        return select_best_credential(candidates);
    }

    get_credentials_interactive()
}

pub fn select_best_credential(candidates: Vec<KakaoCredentials>) -> Result<KakaoCredentials> {
    let mut unique = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in candidates {
        if seen.insert(c.oauth_token.clone()) {
            unique.push(c);
        }
    }

    let first = unique
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("No credentials candidate"))?;

    for creds in unique {
        let client = match KakaoRestClient::new(creds.clone()) {
            Ok(client) => client,
            Err(_) => continue,
        };

        match client.verify_token() {
            Ok(true) => return Ok(creds),
            Ok(false) => continue,
            Err(_) => continue,
        }
    }

    eprintln!("[auth] No valid token candidate found; using newest cached token.");
    Ok(first)
}

pub fn get_rest_ready_client() -> Result<KakaoRestClient> {
    let creds = resolve_base_credentials()?;
    let stable = stabilize_rest_credentials(creds)?;
    KakaoRestClient::new(stable)
}

pub fn stabilize_rest_credentials(creds: KakaoCredentials) -> Result<KakaoCredentials> {
    let client = KakaoRestClient::new(creds.clone())?;

    match client.verify_token() {
        Ok(true) => return Ok(creds),
        Ok(false) => {
            eprintln!(
                "[auth/rest] Token invalid. Recovery order: {}",
                Transport::Rest.default_recovery_order().join(" -> ")
            );
        }
        Err(_) => return Ok(creds),
    }

    if let Some(result) = attempt_relogin(&creds, true, None, None)? {
        if let Some(new_creds) = result.credentials {
            eprintln!("[auth/rest] Recovered via {}.", result.source);
            save_credentials(&new_creds)?;
            return Ok(new_creds);
        }
    }

    if let Some(result) = attempt_renew(&creds)? {
        if let Some(new_creds) = result.credentials {
            eprintln!("[auth/rest] Recovered via {}.", result.source);
            save_credentials(&new_creds)?;
            return Ok(new_creds);
        }
    }

    let fresh = get_credential_candidates(8)?;
    if !fresh.is_empty() {
        let new_creds = select_best_credential(fresh)?;
        save_credentials(&new_creds)?;
        eprintln!("[auth/rest] Recovered via Cache.db extraction.");
        return Ok(new_creds);
    }

    eprintln!("[auth/rest] No better credentials available; proceeding with current token.");
    Ok(creds)
}

pub fn attempt_relogin(
    creds: &KakaoCredentials,
    fresh_xvc: bool,
    password_override: Option<&str>,
    email_override: Option<&str>,
) -> Result<Option<RefreshResult>> {
    let params = match extract_login_params()? {
        Some(params) => params,
        None => return Ok(None),
    };

    let password = password_override.unwrap_or(&params.password);
    let email = email_override.unwrap_or(&params.email);
    let client = KakaoRestClient::new(creds.clone())?;

    let response = if fresh_xvc {
        client.login_with_xvc(email, password, &params.device_uuid, &params.device_name)?
    } else {
        client.login_direct(
            email,
            password,
            &params.device_uuid,
            &params.device_name,
            &params.x_vc,
        )?
    };

    let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);
    if status != 0 {
        return Ok(Some(RefreshResult {
            response,
            credentials: None,
            source: if fresh_xvc {
                "login.json + fresh X-VC"
            } else {
                "login.json + cached X-VC"
            },
        }));
    }

    let new_creds = credentials_from_auth_response(creds, &response);
    Ok(Some(RefreshResult {
        response,
        credentials: Some(new_creds),
        source: if fresh_xvc {
            "login.json + fresh X-VC"
        } else {
            "login.json + cached X-VC"
        },
    }))
}

pub fn attempt_renew(creds: &KakaoCredentials) -> Result<Option<RefreshResult>> {
    let refresh_token = creds
        .refresh_token
        .clone()
        .or_else(|| extract_refresh_token().ok().flatten());

    let Some(refresh_token) = refresh_token else {
        return Ok(None);
    };

    let client = KakaoRestClient::new(creds.clone())?;

    let oauth2_response = client.oauth2_token(&refresh_token)?;
    let oauth2_status = oauth2_response
        .get("status")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    if oauth2_status == 0 {
        let mut new_creds = credentials_from_auth_response(creds, &oauth2_response);
        new_creds.refresh_token = oauth2_response
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some(refresh_token.clone()));

        return Ok(Some(RefreshResult {
            response: oauth2_response,
            credentials: Some(new_creds),
            source: "oauth2_token.json",
        }));
    }

    let legacy_response = client.renew_token(&refresh_token)?;
    let legacy_status = legacy_response
        .get("status")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    if legacy_status == 0 {
        let mut new_creds = credentials_from_auth_response(creds, &legacy_response);
        new_creds.refresh_token = legacy_response
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or(Some(refresh_token));

        return Ok(Some(RefreshResult {
            response: legacy_response,
            credentials: Some(new_creds),
            source: "renew_token.json",
        }));
    }

    Ok(Some(RefreshResult {
        response: legacy_response,
        credentials: None,
        source: "refresh_token renewal",
    }))
}

pub async fn connect_loco_with_reauth(client: &mut LocoClient) -> Result<Document> {
    let login_data = client.full_connect_with_retry(3).await?;
    let status = login_status(&login_data);

    if status == 0 {
        return Ok(login_data);
    }

    if status != -950 {
        anyhow::bail!("LOCO login failed (status={})", status);
    }

    eprintln!(
        "[auth/loco] LOGINLIST rejected token. Recovery order: {}",
        Transport::Loco.default_recovery_order().join(" -> ")
    );

    if let Some(result) = attempt_relogin(&client.credentials, true, None, None)? {
        if let Some(new_creds) = result.credentials {
            return reconnect_loco_with_credentials(client, new_creds, result.source).await;
        }
    }

    if let Some(result) = attempt_renew(&client.credentials)? {
        if let Some(new_creds) = result.credentials {
            return reconnect_loco_with_credentials(client, new_creds, result.source).await;
        }
    }

    let fresh = get_credential_candidates(8)?;
    if !fresh.is_empty() {
        let new_creds = select_best_credential(fresh)?;
        return reconnect_loco_with_credentials(client, new_creds, "Cache.db extraction").await;
    }

    anyhow::bail!("LOCO login failed (status=-950) and no recovery path succeeded")
}

fn credentials_from_auth_response(
    current: &KakaoCredentials,
    response: &Value,
) -> KakaoCredentials {
    let mut new_creds = current.clone();
    if let Some(access) = response.get("access_token").and_then(Value::as_str) {
        new_creds.oauth_token = access.to_string();
    }
    if let Some(user_id) = response.get("userId").and_then(Value::as_i64) {
        new_creds.user_id = user_id;
    }
    if let Some(refresh) = response.get("refresh_token").and_then(Value::as_str) {
        new_creds.refresh_token = Some(refresh.to_string());
    }
    new_creds
}

fn login_status(login_data: &Document) -> i64 {
    login_data
        .get_i64("status")
        .or_else(|_| login_data.get_i32("status").map(|v| v as i64))
        .unwrap_or(-1)
}

async fn reconnect_loco_with_credentials(
    client: &mut LocoClient,
    new_creds: KakaoCredentials,
    source: &'static str,
) -> Result<Document> {
    eprintln!("[auth/loco] Re-authenticated via {}.", source);
    save_credentials(&new_creds)?;
    client.credentials = new_creds;
    client.disconnect();

    let login_data = client.full_connect_with_retry(3).await?;
    let status = login_status(&login_data);
    if status != 0 {
        anyhow::bail!(
            "LOCO login still fails after {} (status={})",
            source,
            status
        );
    }

    Ok(login_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_recovery_order_is_defined() {
        assert!(Transport::Rest.default_recovery_order().len() >= 3);
        assert!(Transport::Loco.default_recovery_order().len() >= 3);
    }

    #[test]
    fn auth_response_updates_tokens_and_user_id() {
        let creds = KakaoCredentials::new(
            "old-token".to_string(),
            1,
            "device".to_string(),
            "3.7.0".to_string(),
            String::new(),
            String::new(),
        );
        let response = serde_json::json!({
            "access_token": "new-token",
            "refresh_token": "refresh-2",
            "userId": 99
        });

        let updated = credentials_from_auth_response(&creds, &response);
        assert_eq!(updated.oauth_token, "new-token");
        assert_eq!(updated.refresh_token.as_deref(), Some("refresh-2"));
        assert_eq!(updated.user_id, 99);
    }
}
