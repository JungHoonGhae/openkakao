use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

const RELOGIN_MIN_INTERVAL_SECS: i64 = 5 * 60;
const RENEW_MIN_INTERVAL_SECS: i64 = 2 * 60;
const MAX_AUTH_COOLDOWN_SECS: i64 = 30 * 60;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenKakaoState {
    pub last_success_at: Option<String>,
    pub last_success_transport: Option<String>,
    pub last_recovery_source: Option<String>,
    pub last_renew_at: Option<String>,
    pub last_relogin_at: Option<String>,
    #[serde(default)]
    pub consecutive_failures: u32,
    pub last_failure_kind: Option<String>,
    pub last_failure_at: Option<String>,
    pub cooldown_until: Option<String>,
    pub last_unattended_send_at: Option<String>,
    pub last_hook_at: Option<String>,
    pub last_webhook_at: Option<String>,
    pub last_guard_reason: Option<String>,
    pub last_guard_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecoverySnapshot {
    pub path: String,
    pub last_success_at: Option<String>,
    pub last_success_transport: Option<String>,
    pub last_recovery_source: Option<String>,
    pub last_failure_kind: Option<String>,
    pub last_failure_at: Option<String>,
    pub consecutive_failures: u32,
    pub cooldown_until: Option<String>,
    pub auth_cooldown_remaining_secs: Option<u64>,
    pub relogin_available_in_secs: Option<u64>,
    pub renew_available_in_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SafetySnapshot {
    pub path: String,
    pub last_unattended_send_at: Option<String>,
    pub last_hook_at: Option<String>,
    pub last_webhook_at: Option<String>,
    pub last_guard_reason: Option<String>,
    pub last_guard_at: Option<String>,
    pub send_available_in_secs: Option<u64>,
    pub hook_available_in_secs: Option<u64>,
    pub webhook_available_in_secs: Option<u64>,
}

pub fn state_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not resolve home directory")?;
    Ok(home.join(".config").join("openkakao").join("state.json"))
}

pub fn load_state() -> Result<OpenKakaoState> {
    let path = state_path()?;
    if !path.exists() {
        return Ok(OpenKakaoState::default());
    }

    let data =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let state: OpenKakaoState = serde_json::from_str(&data)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(state)
}

pub fn save_state(state: &OpenKakaoState) -> Result<PathBuf> {
    let path = state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    let data = serde_json::to_string_pretty(state).context("Failed to serialize state")?;

    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("Failed to create {}", path.display()))?
    };
    #[cfg(not(unix))]
    let mut file =
        fs::File::create(&path).with_context(|| format!("Failed to create {}", path.display()))?;

    file.write_all(data.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(path)
}

fn parse_ts(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn now_string() -> String {
    Utc::now().to_rfc3339()
}

pub fn auth_cooldown_remaining_secs() -> Result<Option<u64>> {
    let state = load_state()?;
    let now = Utc::now();
    let Some(until) = parse_ts(state.cooldown_until.as_deref()) else {
        return Ok(None);
    };
    if until <= now {
        return Ok(None);
    }
    Ok(Some((until - now).num_seconds().max(1) as u64))
}

pub fn relogin_cooldown_remaining_secs() -> Result<Option<u64>> {
    rate_limit_remaining_secs(
        load_state()?.last_relogin_at.as_deref(),
        RELOGIN_MIN_INTERVAL_SECS,
    )
}

pub fn renew_cooldown_remaining_secs() -> Result<Option<u64>> {
    rate_limit_remaining_secs(
        load_state()?.last_renew_at.as_deref(),
        RENEW_MIN_INTERVAL_SECS,
    )
}

fn rate_limit_remaining_secs(last_attempt: Option<&str>, minimum_secs: i64) -> Result<Option<u64>> {
    let now = Utc::now();
    let Some(last) = parse_ts(last_attempt) else {
        return Ok(None);
    };
    let remaining = (last + Duration::seconds(minimum_secs) - now).num_seconds();
    if remaining > 0 {
        Ok(Some(remaining as u64))
    } else {
        Ok(None)
    }
}

fn rate_limit_with_config(last_attempt: Option<&str>, minimum_secs: u64) -> Result<Option<u64>> {
    rate_limit_remaining_secs(last_attempt, minimum_secs as i64)
}

pub fn mark_relogin_attempt() -> Result<()> {
    mutate_state(|state| state.last_relogin_at = Some(now_string()))
}

pub fn mark_renew_attempt() -> Result<()> {
    mutate_state(|state| state.last_renew_at = Some(now_string()))
}

pub fn unattended_send_remaining_secs(minimum_secs: u64) -> Result<Option<u64>> {
    rate_limit_with_config(
        load_state()?.last_unattended_send_at.as_deref(),
        minimum_secs,
    )
}

pub fn hook_remaining_secs(minimum_secs: u64) -> Result<Option<u64>> {
    rate_limit_with_config(load_state()?.last_hook_at.as_deref(), minimum_secs)
}

pub fn webhook_remaining_secs(minimum_secs: u64) -> Result<Option<u64>> {
    rate_limit_with_config(load_state()?.last_webhook_at.as_deref(), minimum_secs)
}

pub fn mark_unattended_send_attempt() -> Result<()> {
    mutate_state(|state| state.last_unattended_send_at = Some(now_string()))
}

pub fn mark_hook_attempt() -> Result<()> {
    mutate_state(|state| state.last_hook_at = Some(now_string()))
}

pub fn mark_webhook_attempt() -> Result<()> {
    mutate_state(|state| state.last_webhook_at = Some(now_string()))
}

pub fn record_guard(reason: &str) -> Result<()> {
    mutate_state(|state| {
        state.last_guard_reason = Some(reason.to_string());
        state.last_guard_at = Some(now_string());
    })
}

pub fn record_success(transport: &str, recovery_source: Option<&str>) -> Result<()> {
    mutate_state(|state| {
        state.last_success_at = Some(now_string());
        state.last_success_transport = Some(transport.to_string());
        state.last_recovery_source = recovery_source.map(str::to_string);
        state.consecutive_failures = 0;
        state.last_failure_kind = None;
        state.last_failure_at = None;
        state.cooldown_until = None;
    })
}

pub fn record_transport_success(transport: &str) -> Result<()> {
    mutate_state(|state| {
        state.last_success_at = Some(now_string());
        state.last_success_transport = Some(transport.to_string());
        state.consecutive_failures = 0;
        state.last_failure_kind = None;
        state.last_failure_at = None;
        state.cooldown_until = None;
    })
}

pub fn record_failure(kind: &str) -> Result<()> {
    mutate_state(|state| {
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.last_failure_kind = Some(kind.to_string());
        state.last_failure_at = Some(now_string());
    })
}

pub fn enter_auth_cooldown() -> Result<u64> {
    let mut saved = 60u64;
    mutate_state(|state| {
        let exponent = state.consecutive_failures.saturating_sub(1).min(5);
        let secs = (60u64.saturating_mul(1u64 << exponent)).min(MAX_AUTH_COOLDOWN_SECS as u64);
        saved = secs;
        state.cooldown_until = Some((Utc::now() + Duration::seconds(secs as i64)).to_rfc3339());
    })?;
    Ok(saved)
}

pub fn recovery_snapshot() -> Result<RecoverySnapshot> {
    let state = load_state()?;
    Ok(RecoverySnapshot {
        path: state_path()?.display().to_string(),
        last_success_at: state.last_success_at,
        last_success_transport: state.last_success_transport,
        last_recovery_source: state.last_recovery_source,
        last_failure_kind: state.last_failure_kind,
        last_failure_at: state.last_failure_at,
        consecutive_failures: state.consecutive_failures,
        cooldown_until: state.cooldown_until,
        auth_cooldown_remaining_secs: auth_cooldown_remaining_secs()?,
        relogin_available_in_secs: relogin_cooldown_remaining_secs()?,
        renew_available_in_secs: renew_cooldown_remaining_secs()?,
    })
}

pub fn safety_snapshot(
    min_send_secs: u64,
    min_hook_secs: u64,
    min_webhook_secs: u64,
) -> Result<SafetySnapshot> {
    let state = load_state()?;
    Ok(SafetySnapshot {
        path: state_path()?.display().to_string(),
        last_unattended_send_at: state.last_unattended_send_at,
        last_hook_at: state.last_hook_at,
        last_webhook_at: state.last_webhook_at,
        last_guard_reason: state.last_guard_reason,
        last_guard_at: state.last_guard_at,
        send_available_in_secs: unattended_send_remaining_secs(min_send_secs)?,
        hook_available_in_secs: hook_remaining_secs(min_hook_secs)?,
        webhook_available_in_secs: webhook_remaining_secs(min_webhook_secs)?,
    })
}

pub fn recovery_state_summary() -> Result<String> {
    let snapshot = recovery_snapshot()?;
    Ok(format!(
        "failures={}, last_failure={}, auth_cooldown={}, relogin_in={}, renew_in={}, last_success={} via {}",
        snapshot.consecutive_failures,
        snapshot
            .last_failure_kind
            .as_deref()
            .unwrap_or("none"),
        snapshot
            .auth_cooldown_remaining_secs
            .map(|v| format!("{v}s"))
            .unwrap_or_else(|| "now".to_string()),
        snapshot
            .relogin_available_in_secs
            .map(|v| format!("{v}s"))
            .unwrap_or_else(|| "now".to_string()),
        snapshot
            .renew_available_in_secs
            .map(|v| format!("{v}s"))
            .unwrap_or_else(|| "now".to_string()),
        snapshot
            .last_success_transport
            .as_deref()
            .unwrap_or("never"),
        snapshot
            .last_recovery_source
            .as_deref()
            .unwrap_or("none")
    ))
}

fn mutate_state(mutator: impl FnOnce(&mut OpenKakaoState)) -> Result<()> {
    let mut state = load_state()?;
    mutator(&mut state);
    save_state(&state)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_has_no_cooldown() {
        let state = OpenKakaoState::default();
        assert_eq!(state.consecutive_failures, 0);
        assert!(state.cooldown_until.is_none());
    }

    #[test]
    fn cooldown_cap_is_bounded() {
        let mut state = OpenKakaoState::default();
        state.consecutive_failures = 100;
        let exponent = state.consecutive_failures.saturating_sub(1).min(5);
        let secs = (60u64.saturating_mul(1u64 << exponent)).min(MAX_AUTH_COOLDOWN_SECS as u64);
        assert_eq!(secs, 1800);
    }

    #[test]
    fn rate_limit_reports_remaining_time() {
        let now = Utc::now();
        let last = (now - Duration::seconds(30)).to_rfc3339();
        let remaining = rate_limit_remaining_secs(Some(&last), 60).unwrap();
        assert!(remaining.is_some());
        assert!(remaining.unwrap() <= 30);
    }
}
