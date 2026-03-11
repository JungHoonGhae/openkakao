use std::process::Command;
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use bson::Document;
use serde_json::Value;
use tokio::task;

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
    record_failure, record_success, recovery_state_summary, relogin_cooldown_remaining_secs_with,
    renew_cooldown_remaining_secs,
};

static AUTH_POLICY: OnceLock<AuthPolicy> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPolicy {
    pub prefer_relogin: bool,
    pub auto_renew: bool,
    pub password_cmd: Option<String>,
    pub email_cmd: Option<String>,
}

impl Default for AuthPolicy {
    fn default() -> Self {
        Self {
            prefer_relogin: true,
            auto_renew: true,
            password_cmd: None,
            email_cmd: None,
        }
    }
}

impl AuthPolicy {
    pub fn from_config(config: &AuthConfig) -> Self {
        Self {
            prefer_relogin: config.prefer_relogin.unwrap_or(true),
            auto_renew: config.auto_renew.unwrap_or(true),
            password_cmd: config.password_cmd.clone(),
            email_cmd: config.email_cmd.clone(),
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
    pub fn recovery_order(self, policy: &AuthPolicy) -> Vec<&'static str> {
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
pub(crate) enum RecoveryAttempt {
    Unavailable {
        source: &'static str,
        reason: String,
    },
    Failed {
        source: &'static str,
        detail: String,
        response: Option<Value>,
    },
    Recovered {
        source: &'static str,
        credentials: KakaoCredentials,
        response: Value,
    },
}

pub fn resolve_base_credentials() -> Result<KakaoCredentials> {
    if let Some(mut saved) = load_credentials()? {
        // Best-effort: populate rest_token from Cache.db if not already set
        if saved.rest_token.is_none() {
            match crate::auth::extract_rest_token_from_cache_db() {
                Ok(Some(token)) => {
                    eprintln!("[auth] Extracted REST bearer token from Cache.db");
                    saved.rest_token = Some(token);
                    let _ = save_credentials(&saved);
                }
                Ok(None) => {}
                Err(e) => {
                    if std::env::var("OPENKAKAO_RS_DEBUG").is_ok() {
                        eprintln!("[auth] Cache.db rest_token extraction failed: {}", e);
                    }
                }
            }
        }
        return Ok(saved);
    }

    let candidates = get_credential_candidates(8)?;
    if !candidates.is_empty() {
        return select_best_credential(candidates);
    }

    get_credentials_interactive()
}

/// Attempt to refresh the REST bearer token from Cache.db.
/// Returns true if a new token was extracted and saved.
/// Used by REST retry logic when a pilsner endpoint returns UNAUTHENTICATED.
#[allow(dead_code)]
pub fn refresh_rest_token(creds: &mut KakaoCredentials) -> Result<bool> {
    match crate::auth::extract_rest_token_from_cache_db() {
        Ok(Some(token)) => {
            creds.rest_token = Some(token);
            save_credentials(creds)?;
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(e) => {
            eprintln!("[auth] Cache.db rest_token refresh failed: {}", e);
            Ok(false)
        }
    }
}

pub fn set_auth_policy(policy: AuthPolicy) {
    let _ = AUTH_POLICY.set(policy);
}

pub fn get_auth_policy() -> AuthPolicy {
    AUTH_POLICY.get().cloned().unwrap_or_default()
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
                Transport::Rest.recovery_order(&policy).join(" -> ")
            );
        }
        Err(_) => return Ok(creds),
    }

    for step in recovery_steps(&policy) {
        match run_recovery_step_sync(step, &creds)? {
            RecoveryAttempt::Unavailable { source, reason } => {
                eprintln!("[auth/rest] {} unavailable: {}", source, reason);
            }
            RecoveryAttempt::Failed { source, detail, .. } => {
                eprintln!("[auth/rest] {} failed: {}", source, detail);
            }
            RecoveryAttempt::Recovered {
                source,
                credentials,
                ..
            } => {
                eprintln!("[auth/rest] Recovered via {}.", source);
                save_credentials(&credentials)?;
                record_success("rest", Some(source))?;
                eprintln!("[auth/rest] State: {}", recovery_state_summary()?);
                return Ok(credentials);
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

/// Resolved login parameters from multiple sources (Cache.db-free when possible).
struct ResolvedLoginParams {
    email: String,
    password: String,
    device_uuid: String,
    device_name: String,
}

/// 3-tier fallback for login parameters, minimizing Cache.db dependency.
///
/// email:       email_override → email_cmd (Doppler) → credentials.json → Cache.db
/// device_uuid: credentials.json → Cache.db
/// password:    password_override → password_cmd (Doppler) → Cache.db cached password
fn resolve_login_params(
    creds: &KakaoCredentials,
    password_override: Option<&str>,
    email_override: Option<&str>,
    policy: &AuthPolicy,
) -> Result<Option<ResolvedLoginParams>> {
    // --- email ---
    let email = if let Some(e) = non_empty_secret(email_override) {
        e
    } else if let Some(cmd) = policy.email_cmd.as_deref() {
        match run_shell_command(cmd) {
            Ok(output) if !output.trim().is_empty() => output.trim().to_string(),
            Ok(_) => {
                eprintln!(
                    "[auth] email_cmd returned empty output; trying credentials.json / Cache.db."
                );
                String::new()
            }
            Err(err) => {
                eprintln!(
                    "[auth] email_cmd failed ({}); trying credentials.json / Cache.db.",
                    err
                );
                String::new()
            }
        }
    } else {
        String::new()
    };

    // Try credentials.json email
    let email = if email.is_empty() {
        creds.email.clone().unwrap_or_default()
    } else {
        email
    };

    // device_uuid from credentials.json (always available after first login --save)
    let device_uuid = creds.device_uuid.clone();
    let has_device_uuid = !device_uuid.is_empty();

    // --- password ---
    let password = if let Some(p) = non_empty_secret(password_override) {
        Some(p)
    } else if let Some(cmd) = policy.password_cmd.as_deref() {
        match run_shell_command(cmd) {
            Ok(output) => non_empty_secret(Some(output.as_str())),
            Err(err) => {
                eprintln!(
                    "[auth] password_cmd failed ({}); falling back to Cache.db.",
                    err
                );
                None
            }
        }
    } else {
        None
    };

    // If we have email + device_uuid + password from non-Cache.db sources, skip Cache.db entirely
    if let (false, true, Some(pw)) = (email.is_empty(), has_device_uuid, password.as_ref()) {
        return Ok(Some(ResolvedLoginParams {
            email,
            password: pw.clone(),
            device_uuid,
            device_name: creds.device_name.clone(),
        }));
    }

    // Fall back to Cache.db for missing pieces. Treat access errors (e.g. KakaoTalk
    // locking the directory) the same as "Cache.db not present" so the function
    // always returns Ok(…) even when Cache.db is temporarily unavailable.
    let cache_db_result = extract_login_params().unwrap_or_else(|e| {
        eprintln!("[auth] Cache.db access failed ({}); continuing without it.", e);
        None
    });
    let cache_params = match cache_db_result {
        Some(p) => p,
        None => {
            if password.is_none() {
                return Ok(None); // no password anywhere
            }
            if email.is_empty() {
                return Ok(None); // no email anywhere
            }
            // Have password + email but no Cache.db — use what we have
            return Ok(Some(ResolvedLoginParams {
                email,
                password: password.unwrap(),
                device_uuid,
                device_name: creds.device_name.clone(),
            }));
        }
    };

    let final_email = if email.is_empty() {
        cache_params.email.clone()
    } else {
        email
    };
    let final_device_uuid = if has_device_uuid {
        device_uuid
    } else {
        cache_params.device_uuid.clone()
    };
    let final_password = password
        .unwrap_or_else(|| non_empty_secret(Some(&cache_params.password)).unwrap_or_default());

    if final_email.is_empty() || final_password.is_empty() {
        return Ok(None);
    }

    Ok(Some(ResolvedLoginParams {
        email: final_email,
        password: final_password,
        device_uuid: final_device_uuid,
        device_name: cache_params.device_name,
    }))
}

fn is_transient_login_error(status: i64) -> bool {
    matches!(status, -500 | -503 | -9999)
}

pub(crate) fn attempt_relogin(
    creds: &KakaoCredentials,
    fresh_xvc: bool,
    password_override: Option<&str>,
    email_override: Option<&str>,
) -> Result<RecoveryAttempt> {
    let source = relogin_source(fresh_xvc);
    let policy = get_auth_policy();

    let Some(params) = resolve_login_params(creds, password_override, email_override, &policy)?
    else {
        return Ok(RecoveryAttempt::Unavailable {
            source,
            reason: "no relogin parameters available (email/password/device_uuid)".to_string(),
        });
    };

    let client = KakaoRestClient::new(creds.clone())?;

    let response = if fresh_xvc {
        client.login_with_xvc(
            &params.email,
            &params.password,
            &params.device_uuid,
            &params.device_name,
        )?
    } else {
        // For cached X-VC, fall back to Cache.db params
        let cache_params = extract_login_params()?;
        let x_vc = cache_params.as_ref().map(|p| p.x_vc.as_str()).unwrap_or("");
        if x_vc.is_empty() {
            return Ok(RecoveryAttempt::Unavailable {
                source,
                reason: "cached X-VC unavailable".to_string(),
            });
        }
        client.login_direct(
            &params.email,
            &params.password,
            &params.device_uuid,
            &params.device_name,
            x_vc,
        )?
    };

    let status = response.get("status").and_then(Value::as_i64).unwrap_or(-1);

    // Retry once on transient errors
    if status != 0 && is_transient_login_error(status) {
        eprintln!(
            "[auth] login returned transient error (status={}); retrying in 2s...",
            status
        );
        std::thread::sleep(std::time::Duration::from_secs(2));
        let retry_response = if fresh_xvc {
            client.login_with_xvc(
                &params.email,
                &params.password,
                &params.device_uuid,
                &params.device_name,
            )?
        } else {
            let cache_params = extract_login_params()?;
            let x_vc = cache_params.as_ref().map(|p| p.x_vc.as_str()).unwrap_or("");
            client.login_direct(
                &params.email,
                &params.password,
                &params.device_uuid,
                &params.device_name,
                x_vc,
            )?
        };
        let retry_status = retry_response
            .get("status")
            .and_then(Value::as_i64)
            .unwrap_or(-1);
        if retry_status == 0 {
            let mut new_creds = credentials_from_auth_response(creds, &retry_response);
            backfill_email(&mut new_creds, &params.email);
            return Ok(RecoveryAttempt::Recovered {
                source,
                credentials: new_creds,
                response: retry_response,
            });
        }
        return Ok(RecoveryAttempt::Failed {
            source,
            detail: format!("status={} (after retry)", retry_status),
            response: Some(retry_response),
        });
    }

    if status != 0 {
        return Ok(RecoveryAttempt::Failed {
            source,
            detail: format!("status={}", status),
            response: Some(response),
        });
    }

    let mut new_creds = credentials_from_auth_response(creds, &response);
    backfill_email(&mut new_creds, &params.email);
    Ok(RecoveryAttempt::Recovered {
        source,
        credentials: new_creds,
        response,
    })
}

/// Backfill email into credentials so future relogins don't need Cache.db or email_cmd.
fn backfill_email(creds: &mut KakaoCredentials, email: &str) {
    if !email.is_empty() && creds.email.is_none() {
        creds.email = Some(email.to_string());
    }
}

pub(crate) fn attempt_renew(creds: &KakaoCredentials) -> Result<RecoveryAttempt> {
    let refresh_token = creds
        .refresh_token
        .clone()
        .or_else(|| extract_refresh_token().ok().flatten());

    let Some(refresh_token) = refresh_token else {
        return Ok(RecoveryAttempt::Unavailable {
            source: "refresh_token renewal",
            reason: "no refresh token available".to_string(),
        });
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

        return Ok(RecoveryAttempt::Recovered {
            source: "oauth2_token.json",
            credentials: new_creds,
            response: oauth2_response,
        });
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

        return Ok(RecoveryAttempt::Recovered {
            source: "renew_token.json",
            credentials: new_creds,
            response: legacy_response,
        });
    }

    Ok(RecoveryAttempt::Failed {
        source: "refresh_token renewal",
        detail: format!(
            "oauth2 status={}, legacy status={}",
            oauth2_status, legacy_status
        ),
        response: Some(serde_json::json!({
            "oauth2": oauth2_response,
            "legacy": legacy_response,
        })),
    })
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
        Transport::Loco.recovery_order(&policy).join(" -> ")
    );

    for step in recovery_steps(&policy) {
        match run_recovery_step_async(step, client.credentials.clone()).await? {
            RecoveryAttempt::Unavailable { source, reason } => {
                eprintln!("[auth/loco] {} unavailable: {}", source, reason);
            }
            RecoveryAttempt::Failed { source, detail, .. } => {
                eprintln!("[auth/loco] {} failed: {}", source, detail);
            }
            RecoveryAttempt::Recovered {
                source,
                credentials,
                ..
            } => {
                return reconnect_loco_with_credentials(client, credentials, source).await;
            }
        }
    }

    let fresh = get_credential_candidates_async(8).await?;
    if !fresh.is_empty() {
        let new_creds = select_best_credential_async(fresh).await?;
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

async fn attempt_relogin_async(
    creds: KakaoCredentials,
    fresh_xvc: bool,
    password_override: Option<String>,
    email_override: Option<String>,
) -> Result<RecoveryAttempt> {
    task::spawn_blocking(move || {
        attempt_relogin(
            &creds,
            fresh_xvc,
            password_override.as_deref(),
            email_override.as_deref(),
        )
    })
    .await
    .map_err(|err| anyhow!("relogin task join failed: {}", err))?
}

async fn attempt_renew_async(creds: KakaoCredentials) -> Result<RecoveryAttempt> {
    task::spawn_blocking(move || attempt_renew(&creds))
        .await
        .map_err(|err| anyhow!("renew task join failed: {}", err))?
}

async fn get_credential_candidates_async(max_candidates: usize) -> Result<Vec<KakaoCredentials>> {
    task::spawn_blocking(move || get_credential_candidates(max_candidates))
        .await
        .map_err(|err| anyhow!("credential scan task join failed: {}", err))?
}

async fn select_best_credential_async(
    candidates: Vec<KakaoCredentials>,
) -> Result<KakaoCredentials> {
    task::spawn_blocking(move || select_best_credential(candidates))
        .await
        .map_err(|err| anyhow!("credential selection task join failed: {}", err))?
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

fn recovery_steps(policy: &AuthPolicy) -> Vec<RecoveryStep> {
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

fn relogin_source(fresh_xvc: bool) -> &'static str {
    if fresh_xvc {
        "login.json + fresh X-VC"
    } else {
        "login.json + cached X-VC"
    }
}

fn run_recovery_step_sync(step: RecoveryStep, creds: &KakaoCredentials) -> Result<RecoveryAttempt> {
    let policy = get_auth_policy();
    match step {
        RecoveryStep::Relogin => {
            let has_pw = policy.password_cmd.is_some();
            if let Some(remaining) = relogin_cooldown_remaining_secs_with(has_pw)? {
                return Ok(RecoveryAttempt::Unavailable {
                    source: "login.json relogin",
                    reason: format!("cooldown {}s remaining", remaining),
                });
            }
            mark_relogin_attempt()?;
            attempt_relogin(creds, true, None, None)
        }
        RecoveryStep::Renew => {
            if let Some(remaining) = renew_cooldown_remaining_secs()? {
                return Ok(RecoveryAttempt::Unavailable {
                    source: "refresh_token renewal",
                    reason: format!("cooldown {}s remaining", remaining),
                });
            }
            mark_renew_attempt()?;
            attempt_renew(creds)
        }
    }
}

async fn run_recovery_step_async(
    step: RecoveryStep,
    creds: KakaoCredentials,
) -> Result<RecoveryAttempt> {
    let policy = get_auth_policy();
    match step {
        RecoveryStep::Relogin => {
            let has_pw = policy.password_cmd.is_some();
            if let Some(remaining) = relogin_cooldown_remaining_secs_with(has_pw)? {
                return Ok(RecoveryAttempt::Unavailable {
                    source: "login.json relogin",
                    reason: format!("cooldown {}s remaining", remaining),
                });
            }
            mark_relogin_attempt()?;
            attempt_relogin_async(creds, true, None, None).await
        }
        RecoveryStep::Renew => {
            if let Some(remaining) = renew_cooldown_remaining_secs()? {
                return Ok(RecoveryAttempt::Unavailable {
                    source: "refresh_token renewal",
                    reason: format!("cooldown {}s remaining", remaining),
                });
            }
            mark_renew_attempt()?;
            attempt_renew_async(creds).await
        }
    }
}

fn non_empty_secret(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn run_shell_command(cmd: &str) -> Result<String> {
    let output = Command::new("sh")
        .arg("-lc")
        .arg(cmd)
        .output()
        .map_err(|err| anyhow!("could not spawn command: {}", err))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            anyhow::bail!("exit status {}", output.status);
        }
        anyhow::bail!("{}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
        assert!(Transport::Rest.recovery_order(&AuthPolicy::default()).len() >= 3);
        assert!(Transport::Loco.recovery_order(&AuthPolicy::default()).len() >= 3);
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
    fn relogin_source_matches_freshness() {
        assert_eq!(relogin_source(true), "login.json + fresh X-VC");
        assert_eq!(relogin_source(false), "login.json + cached X-VC");
    }

    #[test]
    fn default_policy_prefers_relogin_then_renew() {
        assert_eq!(
            recovery_steps(&AuthPolicy::default()),
            vec![RecoveryStep::Relogin, RecoveryStep::Renew]
        );
    }

    #[test]
    fn policy_can_prefer_renew_first() {
        assert_eq!(
            recovery_steps(&AuthPolicy {
                prefer_relogin: false,
                auto_renew: true,
                password_cmd: None,
                email_cmd: None,
            }),
            vec![RecoveryStep::Renew, RecoveryStep::Relogin]
        );
    }

    #[test]
    fn policy_can_disable_renew() {
        assert_eq!(
            recovery_steps(&AuthPolicy {
                prefer_relogin: false,
                auto_renew: false,
                password_cmd: None,
                email_cmd: None,
            }),
            vec![RecoveryStep::Relogin]
        );
    }

    #[test]
    fn resolve_login_params_with_overrides_skips_cache_db() {
        let creds = KakaoCredentials::new(
            "token".to_string(),
            1,
            "device-uuid".to_string(),
            "3.7.0".to_string(),
            String::new(),
            String::new(),
        );
        let policy = AuthPolicy {
            password_cmd: Some("printf 'doppler-pw'".into()),
            email_cmd: Some("printf 'user@example.com'".into()),
            ..AuthPolicy::default()
        };
        let params =
            resolve_login_params(&creds, None, None, &policy).expect("should resolve params");
        let params = params.expect("should have params");
        assert_eq!(params.email, "user@example.com");
        assert_eq!(params.password, "doppler-pw");
        assert_eq!(params.device_uuid, "device-uuid");
    }

    #[test]
    fn resolve_login_params_email_override_wins() {
        let creds = KakaoCredentials::new(
            "token".to_string(),
            1,
            "device-uuid".to_string(),
            "3.7.0".to_string(),
            String::new(),
            String::new(),
        );
        let policy = AuthPolicy {
            password_cmd: Some("printf 'pw'".into()),
            email_cmd: Some("printf 'cmd@example.com'".into()),
            ..AuthPolicy::default()
        };
        let params = resolve_login_params(&creds, None, Some("override@example.com"), &policy)
            .expect("should resolve params");
        let params = params.expect("should have params");
        assert_eq!(params.email, "override@example.com");
    }

    #[test]
    fn resolve_login_params_password_override_wins() {
        let creds = KakaoCredentials::new(
            "token".to_string(),
            1,
            "device-uuid".to_string(),
            "3.7.0".to_string(),
            String::new(),
            String::new(),
        );
        let policy = AuthPolicy {
            password_cmd: Some("printf 'doppler-pw'".into()),
            email_cmd: Some("printf 'user@example.com'".into()),
            ..AuthPolicy::default()
        };
        let params = resolve_login_params(&creds, Some("manual-pw"), None, &policy)
            .expect("should resolve params");
        let params = params.expect("should have params");
        assert_eq!(params.password, "manual-pw");
    }

    #[test]
    fn resolve_login_params_uses_creds_email() {
        let mut creds = KakaoCredentials::new(
            "token".to_string(),
            1,
            "device-uuid".to_string(),
            "3.7.0".to_string(),
            String::new(),
            String::new(),
        );
        creds.email = Some("saved@example.com".to_string());
        let policy = AuthPolicy {
            password_cmd: Some("printf 'pw'".into()),
            ..AuthPolicy::default()
        };
        let params =
            resolve_login_params(&creds, None, None, &policy).expect("should resolve params");
        let params = params.expect("should have params");
        assert_eq!(params.email, "saved@example.com");
    }

    #[test]
    fn resolve_login_params_none_when_no_password_no_cache() {
        // Without password_cmd and with empty device_uuid (no prior login),
        // resolve_login_params should not panic and returns either None
        // (if Cache.db is unavailable) or Some (if Cache.db exists on this machine).
        let creds = KakaoCredentials::new(
            "token".to_string(),
            1,
            String::new(), // empty device_uuid
            "3.7.0".to_string(),
            String::new(),
            String::new(),
        );
        let policy = AuthPolicy::default();
        let result = resolve_login_params(&creds, None, None, &policy).expect("should not error");
        // On CI (no Cache.db), this is None. On dev machines with Cache.db, it may resolve.
        // The key invariant is: no panic, no error.
        let _ = result;
    }

    #[test]
    fn backfill_email_sets_when_missing() {
        let mut creds = KakaoCredentials::new(
            "token".to_string(),
            1,
            "dev".to_string(),
            "3.7.0".to_string(),
            String::new(),
            String::new(),
        );
        assert!(creds.email.is_none());
        backfill_email(&mut creds, "user@example.com");
        assert_eq!(creds.email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn backfill_email_preserves_existing() {
        let mut creds = KakaoCredentials::new(
            "token".to_string(),
            1,
            "dev".to_string(),
            "3.7.0".to_string(),
            String::new(),
            String::new(),
        );
        creds.email = Some("existing@example.com".to_string());
        backfill_email(&mut creds, "new@example.com");
        assert_eq!(creds.email.as_deref(), Some("existing@example.com"));
    }

    #[test]
    fn transient_login_errors_are_identified() {
        assert!(is_transient_login_error(-500));
        assert!(is_transient_login_error(-503));
        assert!(is_transient_login_error(-9999));
        assert!(!is_transient_login_error(-950));
        assert!(!is_transient_login_error(-300));
        assert!(!is_transient_login_error(0));
    }

    #[test]
    fn run_shell_command_captures_output() {
        let output = run_shell_command("printf 'hello'").expect("should succeed");
        assert_eq!(output, "hello");
    }

    #[test]
    fn run_shell_command_trims_whitespace() {
        let output = run_shell_command("printf '  trimmed  \\n'").expect("should succeed");
        assert_eq!(output, "trimmed");
    }
}
