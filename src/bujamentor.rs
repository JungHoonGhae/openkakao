use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;



pub const WATCH_STATUS_SCHEMA_VERSION: u8 = 1;
pub const SERVICE_WATCH_INTERVAL_SECS: u64 = 5;
pub const SERVICE_FRESHNESS_SECS: i64 = 35;
pub const SERVICE_HOOK_TIMEOUT_SECS: u64 = 20;
const SERVICE_HOOK_PAYLOAD_LIMIT_BYTES: usize = 4096;

fn message_type_label(message_type: i32) -> &'static str {
    match message_type {
        1 => "text",
        2 => "photo",
        3 => "video",
        5 => "contact",
        12 => "voice",
        14 => "emoticon",
        16 => "live",
        18 => "search",
        22 => "map",
        23 => "profile",
        26 => "file",
        27 => "multi-photo",
        71 | 72 => "poll",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchStatusState {
    Starting,
    Healthy,
    Degraded,
}

impl WatchStatusState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchStatusClass {
    Healthy,
    StartingGrace,
    Stale,
    Degraded,
}

impl WatchStatusClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::StartingGrace => "starting_grace",
            Self::Stale => "stale",
            Self::Degraded => "degraded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosedReason {
    StatusAbsent,
    StatusUnreadable,
    StatusInvalid,
    StartedAtFuture,
    HeartbeatBeforeStart,
    HeartbeatFuture,
    CompletedBeforeStart,
    CompletedAfterHeartbeat,
    CompletedFuture,
    StartingExpired,
    HeartbeatStale,
    AxUnavailable,
    ScrapeFailed,
    HookFailed,
    HookTimedOut,
    HookRateLimited,
    ConfigInvalid,
    LoggerFailed,
}

impl ClosedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StatusAbsent => "status_absent",
            Self::StatusUnreadable => "status_unreadable",
            Self::StatusInvalid => "status_invalid",
            Self::StartedAtFuture => "started_at_future",
            Self::HeartbeatBeforeStart => "heartbeat_before_start",
            Self::HeartbeatFuture => "heartbeat_future",
            Self::CompletedBeforeStart => "completed_before_start",
            Self::CompletedAfterHeartbeat => "completed_after_heartbeat",
            Self::CompletedFuture => "completed_future",
            Self::StartingExpired => "starting_expired",
            Self::HeartbeatStale => "heartbeat_stale",
            Self::AxUnavailable => "ax_unavailable",
            Self::ScrapeFailed => "scrape_failed",
            Self::HookFailed => "hook_failed",
            Self::HookTimedOut => "hook_timed_out",
            Self::HookRateLimited => "hook_rate_limited",
            Self::ConfigInvalid => "config_invalid",
            Self::LoggerFailed => "logger_failed",
        }
    }

    pub fn fingerprint(self, class: WatchStatusClass) -> String {
        format!("v1:{}:{}", class.as_str(), self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchStatusV1 {
    pub schema_version: u8,
    pub instance_id: String,
    pub state: WatchStatusState,
    pub started_at: String,
    pub heartbeat_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_reason: Option<ClosedReason>,
}

impl WatchStatusV1 {
    pub fn starting_now() -> Self {
        Self::starting_at(generate_instance_id(), Utc::now())
    }

    pub fn starting_at(instance_id: String, now: DateTime<Utc>) -> Self {
        let now = now.to_rfc3339();
        Self {
            schema_version: WATCH_STATUS_SCHEMA_VERSION,
            instance_id,
            state: WatchStatusState::Starting,
            started_at: now.clone(),
            heartbeat_at: now,
            completed_at: None,
            closed_reason: None,
        }
    }

    pub fn mark_healthy(&mut self, now: DateTime<Utc>) {
        let now = now.to_rfc3339();
        self.state = WatchStatusState::Healthy;
        self.heartbeat_at = now.clone();
        self.completed_at = Some(now);
        self.closed_reason = None;
    }

    pub fn mark_degraded(&mut self, now: DateTime<Utc>, reason: ClosedReason) {
        let now = now.to_rfc3339();
        self.state = WatchStatusState::Degraded;
        self.heartbeat_at = now.clone();
        self.completed_at = Some(now);
        self.closed_reason = Some(reason);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedWatchStatus {
    pub class: WatchStatusClass,
    pub reason: Option<ClosedReason>,
    pub fingerprint: Option<String>,
}

impl ClassifiedWatchStatus {
    fn healthy() -> Self {
        Self {
            class: WatchStatusClass::Healthy,
            reason: None,
            fingerprint: None,
        }
    }

    fn starting_grace() -> Self {
        Self {
            class: WatchStatusClass::StartingGrace,
            reason: None,
            fingerprint: None,
        }
    }

    fn closed(class: WatchStatusClass, reason: ClosedReason) -> Self {
        Self {
            class,
            reason: Some(reason),
            fingerprint: Some(reason.fingerprint(class)),
        }
    }
}

fn parse_rfc3339_utc(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("invalid RFC3339 timestamp: {value}"))?
        .with_timezone(&Utc))
}

pub fn classify_watch_status(status: &WatchStatusV1, now: DateTime<Utc>) -> ClassifiedWatchStatus {
    if status.schema_version != WATCH_STATUS_SCHEMA_VERSION {
        return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusInvalid);
    }

    let started_at = match parse_rfc3339_utc(&status.started_at) {
        Ok(ts) => ts,
        Err(_) => return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusInvalid),
    };
    let heartbeat_at = match parse_rfc3339_utc(&status.heartbeat_at) {
        Ok(ts) => ts,
        Err(_) => return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusInvalid),
    };

    if started_at > now {
        return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StartedAtFuture);
    }
    if heartbeat_at < started_at {
        return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::HeartbeatBeforeStart);
    }
    if heartbeat_at > now {
        return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::HeartbeatFuture);
    }

    let completed_at = match status.completed_at.as_deref() {
        Some(value) => match parse_rfc3339_utc(value) {
            Ok(ts) => Some(ts),
            Err(_) => {
                return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusInvalid);
            }
        },
        None => None,
    };

    if let Some(completed_at) = completed_at {
        if completed_at < started_at {
            return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::CompletedBeforeStart);
        }
        if completed_at > heartbeat_at {
            return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::CompletedAfterHeartbeat);
        }
        if completed_at > now {
            return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::CompletedFuture);
        }
    }

    let heartbeat_age_secs = (now - heartbeat_at).num_seconds();
    let started_age_secs = (now - started_at).num_seconds();

    match status.state {
        WatchStatusState::Starting => {
            if status.closed_reason.is_some() {
                return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusInvalid);
            }
            if heartbeat_age_secs <= SERVICE_FRESHNESS_SECS && started_age_secs <= SERVICE_FRESHNESS_SECS {
                return ClassifiedWatchStatus::starting_grace();
            }
            ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StartingExpired)
        }
        WatchStatusState::Healthy => {
            if status.closed_reason.is_some() {
                return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusInvalid);
            }
            if heartbeat_age_secs > SERVICE_FRESHNESS_SECS {
                return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::HeartbeatStale);
            }
            ClassifiedWatchStatus::healthy()
        }
        WatchStatusState::Degraded => {
            let Some(reason) = status.closed_reason else {
                return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusInvalid);
            };
            if heartbeat_age_secs > SERVICE_FRESHNESS_SECS {
                return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::HeartbeatStale);
            }
            ClassifiedWatchStatus::closed(WatchStatusClass::Degraded, reason)
        }
    }
}

pub fn inspect_watch_status_path(path: &Path, now: DateTime<Utc>) -> ClassifiedWatchStatus {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusAbsent);
        }
        Err(_) => {
            return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusUnreadable);
        }
    };

    let status: WatchStatusV1 = match serde_json::from_str(&raw) {
        Ok(status) => status,
        Err(_) => return ClassifiedWatchStatus::closed(WatchStatusClass::Stale, ClosedReason::StatusInvalid),
    };

    classify_watch_status(&status, now)
}

pub fn write_watch_status(path: &Path, status: &WatchStatusV1) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let data = serde_json::to_vec_pretty(status).context("failed to serialize watch status")?;
    atomic_write_private_file(path, &data)
}

pub fn write_config_invalid_status(path: &Path) -> Result<WatchStatusV1> {
    let mut status = WatchStatusV1::starting_now();
    status.mark_degraded(Utc::now(), ClosedReason::ConfigInvalid);
    write_watch_status(path, &status)?;
    Ok(status)
}

fn atomic_write_private_file(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("{} has no UTF-8 file name", path.display()))?;
    let temp_path = unique_temp_path(parent, file_name);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temp_path)
            .with_context(|| format!("failed to create {}", temp_path.display()))?;
        use std::io::Write;
        file.write_all(data)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temp_path.display()))?;
    }

    #[cfg(not(unix))]
    {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .with_context(|| format!("failed to create {}", temp_path.display()))?;
        use std::io::Write;
        file.write_all(data)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temp_path.display()))?;
    }

    fs::rename(&temp_path, path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

fn unique_temp_path(parent: &Path, file_name: &str) -> PathBuf {
    let mut random = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut random);
    parent.join(format!(".{file_name}.{}.tmp", hex::encode(random)))
}

fn generate_instance_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceHookEvent {
    pub event_type: String,
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
    pub unread: i32,
}

#[derive(Debug)]
pub enum DirectServiceHookError {
    TimedOut,
    Failed(anyhow::Error),
}

impl std::fmt::Display for DirectServiceHookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut => write!(f, "service hook timed out"),
            Self::Failed(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DirectServiceHookError {}

pub fn validate_service_path(path: &Path, label: &str) -> Result<()> {
    let raw = path.to_string_lossy();
    if raw.contains(['\n', '\r']) {
        anyhow::bail!("{label} must not contain newlines");
    }
    if !path.is_absolute() {
        anyhow::bail!("{label} must be an absolute path");
    }
    if path.file_name().is_none() {
        anyhow::bail!("{label} must target a file path");
    }
    Ok(())
}

pub async fn run_direct_service_hook(
    hook_path: &Path,
    event: &ServiceHookEvent,
    timeout_secs: u64,
) -> std::result::Result<(), DirectServiceHookError> {
    validate_service_path(hook_path, "hook path").map_err(DirectServiceHookError::Failed)?;

    let payload = serde_json::to_vec(event).map_err(|error| DirectServiceHookError::Failed(error.into()))?;
    if payload.len() > SERVICE_HOOK_PAYLOAD_LIMIT_BYTES {
        return Err(DirectServiceHookError::Failed(anyhow::anyhow!(
            "service hook payload exceeded {} bytes",
            SERVICE_HOOK_PAYLOAD_LIMIT_BYTES
        )));
    }

    let mut child = tokio::process::Command::new(hook_path)
        .env_clear()
        .env("OPENKAKAO_EVENT_TYPE", &event.event_type)
        .env("OPENKAKAO_CHAT_ID", event.chat_id.to_string())
        .env("OPENKAKAO_CHAT_NAME", &event.chat_name)
        .env("OPENKAKAO_LOG_ID", event.log_id.to_string())
        .env("OPENKAKAO_AUTHOR_ID", event.author_id.to_string())
        .env("OPENKAKAO_AUTHOR_NICKNAME", &event.author_nickname)
        .env("OPENKAKAO_MESSAGE_TYPE", event.message_type.to_string())
        .env("OPENKAKAO_MESSAGE_TYPE_LABEL", message_type_label(event.message_type))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| DirectServiceHookError::Failed(error.into()))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&payload)
            .await
            .map_err(|error| DirectServiceHookError::Failed(error.into()))?;
    }

    let timeout = Duration::from_secs(timeout_secs.max(1));
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(DirectServiceHookError::Failed(anyhow::anyhow!(
            "service hook exited with status {}",
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string())
        ))),
        Ok(Err(error)) => Err(DirectServiceHookError::Failed(error.into())),
        Err(_) => {
            let _ = child.kill().await;
            Err(DirectServiceHookError::TimedOut)
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectHookLimiter {
    min_interval: Duration,
    last_attempt: Option<Instant>,
}

impl DirectHookLimiter {
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last_attempt: None,
        }
    }

    pub fn is_rate_limited(&self) -> bool {
        self.last_attempt
            .map(|last_attempt| last_attempt.elapsed() < self.min_interval)
            .unwrap_or(false)
    }

    pub fn record_attempt(&mut self) {
        self.last_attempt = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use tempfile::tempdir;

    fn status_at(now: DateTime<Utc>, state: WatchStatusState, reason: Option<ClosedReason>) -> WatchStatusV1 {
        WatchStatusV1 {
            schema_version: WATCH_STATUS_SCHEMA_VERSION,
            instance_id: "0123456789abcdef0123456789abcdef".to_string(),
            state,
            started_at: (now - ChronoDuration::seconds(5)).to_rfc3339(),
            heartbeat_at: now.to_rfc3339(),
            completed_at: Some(now.to_rfc3339()),
            closed_reason: reason,
        }
    }

    #[test]
    fn healthy_boundary_stays_fresh_at_35_seconds() {
        let now = Utc::now();
        let mut status = status_at(now - ChronoDuration::seconds(SERVICE_FRESHNESS_SECS), WatchStatusState::Healthy, None);
        status.started_at = (now - ChronoDuration::seconds(SERVICE_FRESHNESS_SECS)).to_rfc3339();
        status.heartbeat_at = (now - ChronoDuration::seconds(SERVICE_FRESHNESS_SECS)).to_rfc3339();
        status.completed_at = Some((now - ChronoDuration::seconds(SERVICE_FRESHNESS_SECS)).to_rfc3339());
        let classified = classify_watch_status(&status, now);
        assert_eq!(classified.class, WatchStatusClass::Healthy);
        assert_eq!(classified.reason, None);
    }

    #[test]
    fn healthy_status_turns_stale_after_35_seconds() {
        let now = Utc::now();
        let mut status = status_at(now, WatchStatusState::Healthy, None);
        status.started_at = (now - ChronoDuration::seconds(SERVICE_FRESHNESS_SECS + 2)).to_rfc3339();
        status.heartbeat_at = (now - ChronoDuration::seconds(SERVICE_FRESHNESS_SECS + 1)).to_rfc3339();
        status.completed_at = Some((now - ChronoDuration::seconds(SERVICE_FRESHNESS_SECS + 1)).to_rfc3339());
        let classified = classify_watch_status(&status, now);
        assert_eq!(classified.class, WatchStatusClass::Stale);
        assert_eq!(classified.reason, Some(ClosedReason::HeartbeatStale));
        assert_eq!(classified.fingerprint.as_deref(), Some("v1:stale:heartbeat_stale"));
    }

    #[test]
    fn degraded_status_without_reason_is_invalid() {
        let now = Utc::now();
        let status = status_at(now, WatchStatusState::Degraded, None);
        let classified = classify_watch_status(&status, now);
        assert_eq!(classified.reason, Some(ClosedReason::StatusInvalid));
    }

    #[test]
    fn inspect_watch_status_marks_absent_file() {
        let dir = tempdir().unwrap();
        let classified = inspect_watch_status_path(&dir.path().join("missing.json"), Utc::now());
        assert_eq!(classified.reason, Some(ClosedReason::StatusAbsent));
    }

    #[test]
    fn write_config_invalid_status_persists_expected_reason() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("watch-status.json");
        let status = write_config_invalid_status(&path).unwrap();
        assert_eq!(status.state, WatchStatusState::Degraded);
        assert_eq!(status.closed_reason, Some(ClosedReason::ConfigInvalid));
        let persisted: WatchStatusV1 = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(persisted.closed_reason, Some(ClosedReason::ConfigInvalid));
    }

    #[test]
    fn service_path_validation_rejects_relative_paths() {
        let error = validate_service_path(Path::new("relative/path"), "status path").unwrap_err();
        assert!(error.to_string().contains("absolute path"));
    }

    #[test]
    fn direct_hook_limiter_blocks_second_immediate_attempt() {
        let mut limiter = DirectHookLimiter::new(Duration::from_secs(5));
        assert!(!limiter.is_rate_limited());
        limiter.record_attempt();
        assert!(limiter.is_rate_limited());
    }
}
