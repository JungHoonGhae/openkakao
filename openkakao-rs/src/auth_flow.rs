use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use bson::Document;
use serde_json::Value;

use crate::auth::{
    extract_login_params, extract_refresh_token, get_credential_candidates,
    get_credentials_interactive,
};
use crate::config::AuthConfig;
use crate::credentials::{load_credentials, save_credentials};
use crate::loco::client::LocoClient;
use crate::model::KakaoCredentials;
use crate::rest::KakaoRestClient;
use crate::state::{
    auth_cooldown_remaining_secs, enter_auth_cooldown, mark_relogin_attempt, mark_renew_attempt,
    record_failure, record_success, recovery_state_summary, relogin_cooldown_remaining_secs,
    renew_cooldown_remaining_secs,
};

static AUTH_POLICY: OnceLock<AuthPolicy> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthPolicy {
    pub prefer_relogin: bool,
    pub auto_renew: bool,
}

impl Default for AuthPolicy {
    fn default() -> Self {
        Self {
            prefer_relogin: true,
            auto_renew: true,
        }
    }
}

impl AuthPolicy {
    pub fn from_config(config: &AuthConfig) -> Self {
        Self {
            prefer_relogin: config.prefer_relogin.unwrap_or(true),
            auto_renew: config.auto_renew.unwrap_or(true),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryStep {
    Relogin,
    Renew,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Rest,
    Loco,
}

impl Transport {
    pub fn recovery_order(self, policy: AuthPolicy) -> Vec<&'static str> {
        let mut order = vec!["saved credentials"];

        for step in recovery_steps(policy) {
            match step {
                RecoveryStep::Relogin => order.push("login.json relogin"),
                RecoveryStep::Renew => order.push("refresh_token renewal"),
            }
        }

        order.push("Cache.db extraction");
        order
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

pub fn set_auth_policy(policy: AuthPolicy) {
    let _ = AUTH_POLICY.set(policy);
}

pub fn get_auth_policy() -> AuthPolicy {
    AUTH_POLICY.get().copied().unwrap_or_default()
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
    let policy = get_auth_policy();
    let client = KakaoRestClient::new(creds.clone())?;

    match client.verify_token() {
        Ok(true) => {
            record_success("rest", Some("saved credentials"))?;
            eprintln!("[auth/rest] State: {}", recovery_state_summary()?);
            return Ok(creds);
        }
        Ok(false) => {
            record_failure("auth_expired")?;
            if let Some(remaining) = auth_cooldown_remaining_secs()? {
                eprintln!("[auth/rest] State: {}", recovery_state_summary()?);
                anyhow::bail!(
                    "REST auth recovery cooling down for {}s; retry later or relogin manually",
                    remaining
                );
            }
            eprintln!(
                "[auth/rest] Token invalid. Recovery order: {}",
                Transport::Rest.recovery_order(policy).join(" -> ")
            );
        }
        Err(_) => return Ok(creds),
    }

    for step in recovery_steps(policy) {
        let result = match step {
            RecoveryStep::Relogin => {
                if let Some(remaining) = relogin_cooldown_remaining_secs()? {
                    eprintln!(
                        "[auth/rest] Skipping relogin, cooldown {}s remaining.",
                        remaining
                    );
                    None
                } else {
                    mark_relogin_attempt()?;
                    attempt_relogin(&creds, true, None, None)?
                }
            }
            RecoveryStep::Renew => {
                if let Some(remaining) = renew_cooldown_remaining_secs()? {
                    eprintln!(
                        "[auth/rest] Skipping renew, cooldown {}s remaining.",
                        remaining
                    );
                    None
                } else {
                    mark_renew_attempt()?;
                    attempt_renew(&creds)?
                }
            }
        };

        if let Some(result) = result {
            if let Some(new_creds) = result.credentials {
                eprintln!("[auth/rest] Recovered via {}.", result.source);
                save_credentials(&new_creds)?;
                record_success("rest", Some(result.source))?;
                eprintln!("[auth/rest] State: {}", recovery_state_summary()?);
                return Ok(new_creds);
            }
        }
    }

    let fresh = get_credential_candidates(8)?;
    if !fresh.is_empty() {
        let new_creds = select_best_credential(fresh)?;
        save_credentials(&new_creds)?;
        eprintln!("[auth/rest] Recovered via Cache.db extraction.");
        record_success("rest", Some("Cache.db extraction"))?;
        eprintln!("[auth/rest] State: {}", recovery_state_summary()?);
        return Ok(new_creds);
    }

    record_failure("auth_recovery_exhausted")?;
    let cooldown = enter_auth_cooldown()?;
    eprintln!("[auth/rest] State: {}", recovery_state_summary()?);
    anyhow::bail!(
        "REST token invalid and no recovery path succeeded; cooling down for {}s",
        cooldown
    )
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
    let policy = get_auth_policy();
    let login_data = client.full_connect_with_retry(3).await?;
    let status = login_status(&login_data);

    if status == 0 {
        record_success("loco", Some("saved credentials"))?;
        eprintln!("[auth/loco] State: {}", recovery_state_summary()?);
        return Ok(login_data);
    }

    if status != -950 {
        anyhow::bail!("LOCO login failed (status={})", status);
    }

    record_failure("auth_expired")?;

    if let Some(remaining) = auth_cooldown_remaining_secs()? {
        eprintln!("[auth/loco] State: {}", recovery_state_summary()?);
        anyhow::bail!("LOCO auth recovery cooling down for {}s", remaining);
    }

    eprintln!(
        "[auth/loco] LOGINLIST rejected token. Recovery order: {}",
        Transport::Loco.recovery_order(policy).join(" -> ")
    );

    for step in recovery_steps(policy) {
        let result = match step {
            RecoveryStep::Relogin => {
                if let Some(remaining) = relogin_cooldown_remaining_secs()? {
                    eprintln!(
                        "[auth/loco] Skipping relogin, cooldown {}s remaining.",
                        remaining
                    );
                    None
                } else {
                    mark_relogin_attempt()?;
                    attempt_relogin(&client.credentials, true, None, None)?
                }
            }
            RecoveryStep::Renew => {
                if let Some(remaining) = renew_cooldown_remaining_secs()? {
                    eprintln!(
                        "[auth/loco] Skipping renew, cooldown {}s remaining.",
                        remaining
                    );
                    None
                } else {
                    mark_renew_attempt()?;
                    attempt_renew(&client.credentials)?
                }
            }
        };

        if let Some(result) = result {
            if let Some(new_creds) = result.credentials {
                return reconnect_loco_with_credentials(client, new_creds, result.source).await;
            }
        }
    }

    let fresh = get_credential_candidates(8)?;
    if !fresh.is_empty() {
        let new_creds = select_best_credential(fresh)?;
        return reconnect_loco_with_credentials(client, new_creds, "Cache.db extraction").await;
    }

    record_failure("auth_recovery_exhausted")?;
    let cooldown = enter_auth_cooldown()?;
    eprintln!("[auth/loco] State: {}", recovery_state_summary()?);
    anyhow::bail!(
        "LOCO login failed (status=-950) and no recovery path succeeded; cooling down for {}s",
        cooldown
    )
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

fn recovery_steps(policy: AuthPolicy) -> Vec<RecoveryStep> {
    let mut steps = Vec::new();

    if policy.prefer_relogin {
        steps.push(RecoveryStep::Relogin);
        if policy.auto_renew {
            steps.push(RecoveryStep::Renew);
        }
    } else {
        if policy.auto_renew {
            steps.push(RecoveryStep::Renew);
        }
        steps.push(RecoveryStep::Relogin);
    }

    steps
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
        record_failure("auth_relogin_needed")?;
        eprintln!("[auth/loco] State: {}", recovery_state_summary()?);
        anyhow::bail!(
            "LOCO login still fails after {} (status={})",
            source,
            status
        );
    }

    record_success("loco", Some(source))?;
    eprintln!("[auth/loco] State: {}", recovery_state_summary()?);
    Ok(login_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_recovery_order_is_defined() {
        assert!(Transport::Rest.recovery_order(AuthPolicy::default()).len() >= 3);
        assert!(Transport::Loco.recovery_order(AuthPolicy::default()).len() >= 3);
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

    #[test]
    fn default_policy_prefers_relogin_then_renew() {
        assert_eq!(
            recovery_steps(AuthPolicy::default()),
            vec![RecoveryStep::Relogin, RecoveryStep::Renew]
        );
    }

    #[test]
    fn policy_can_prefer_renew_first() {
        assert_eq!(
            recovery_steps(AuthPolicy {
                prefer_relogin: false,
                auto_renew: true,
            }),
            vec![RecoveryStep::Renew, RecoveryStep::Relogin]
        );
    }

    #[test]
    fn policy_can_disable_renew() {
        assert_eq!(
            recovery_steps(AuthPolicy {
                prefer_relogin: false,
                auto_renew: false,
            }),
            vec![RecoveryStep::Relogin]
        );
    }
}
