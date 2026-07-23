use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

pub const WATCH_SERVICE_LABEL: &str = "com.openkakao.bujamentor.watch";
pub const HEALTH_SERVICE_LABEL: &str = "com.openkakao.bujamentor.health";
pub const WATCH_STATUS_FILE: &str = "watch-status.json";
pub const HEALTH_ALERTS_FILE: &str = "health-alerts.json";
pub const WATCH_LOG_FILE: &str = "watch.log";
pub const HEALTH_LOG_FILE: &str = "health.log";
pub const STATUS_FRESH_SECS: i64 = 35;
pub const HEALTH_INTERVAL_SECS: u64 = 15;
pub const LOG_ROTATE_BYTES: u64 = 65_536;
pub const LOG_ROTATE_AGE_SECS: u64 = 7 * 24 * 60 * 60;
pub const WATCH_INTERVAL_SECS: u64 = 5;
pub const HOOK_TOTAL_DEADLINE_SECS: u64 = 20;
const HOOK_PAYLOAD_LIMIT_BYTES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchState {
    Starting,
    Healthy,
    Degraded,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthClass {
    Healthy,
    StartingGrace,
    Stale,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationOutcome {
    None,
    Unknown,
    Submitted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceName {
    Watch,
    Health,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogEvent {
    WatchStarted,
    WatchPollCompleted,
    WatchTerminalFailure,
    HealthObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchStatusRecord {
    pub schema_version: u8,
    pub state: WatchState,
    pub started_at: String,
    pub heartbeat_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub failure: Option<ClosedReason>,
    #[serde(default)]
    pub poll_count: u64,
    #[serde(default)]
    pub hook_success_count: u64,
    #[serde(default)]
    pub hook_failure_count: u64,
    #[serde(default)]
    pub hook_rate_limited_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthAlertsRecord {
    pub schema_version: u8,
    #[serde(default)]
    pub open_fingerprint: Option<String>,
    #[serde(default)]
    pub last_attempt_key: Option<String>,
    #[serde(default)]
    pub last_outcome: Option<NotificationOutcome>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl Default for HealthAlertsRecord {
    fn default() -> Self {
        Self {
            schema_version: 1,
            open_fingerprint: None,
            last_attempt_key: None,
            last_outcome: None,
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogRecord {
    pub schema_version: u8,
    pub ts: String,
    pub service: ServiceName,
    pub event: LogEvent,
    #[serde(default)]
    pub state: Option<WatchState>,
    #[serde(default)]
    pub class: Option<HealthClass>,
    #[serde(default)]
    pub reason: Option<ClosedReason>,
    #[serde(default)]
    pub failure: Option<ClosedReason>,
    #[serde(default)]
    pub instance_id: Option<String>,
    pub started_delta_ms: u64,
    pub heartbeat_delta_ms: u64,
    pub completed_delta_ms: u64,
    pub observed_age_ms: u64,
    pub notification_outcome: NotificationOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusClassification {
    pub state: Option<WatchState>,
    pub class: HealthClass,
    pub reason: Option<ClosedReason>,
    pub fingerprint: Option<String>,
    pub instance_id: Option<String>,
    pub started_delta_ms: u64,
    pub heartbeat_delta_ms: u64,
    pub completed_delta_ms: u64,
    pub observed_age_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthObservationResult {
    pub classification: StatusClassification,
    pub notification_outcome: NotificationOutcome,
    pub attempt_key: Option<String>,
}

pub trait Notifier {
    fn notify(&mut self, class: &str, reason: &str) -> NotificationOutcome;
}

pub fn default_state_root(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join("openkakao")
        .join("bujamentor")
}

pub fn fingerprint(class: HealthClass, reason: ClosedReason) -> String {
    format!("v1:{}:{}", class.as_str(), reason.as_str())
}

pub fn parse_fingerprint_reason(value: &str) -> Option<ClosedReason> {
    let mut parts = value.split(':');
    let version = parts.next()?;
    let _class = parts.next()?;
    let reason = parts.next()?;
    if version != "v1" || parts.next().is_some() {
        return None;
    }
    ClosedReason::parse(reason)
}

impl WatchStatusRecord {
    pub fn new(
        state: WatchState,
        started_at: impl Into<String>,
        heartbeat_at: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: 1,
            state,
            started_at: started_at.into(),
            heartbeat_at: heartbeat_at.into(),
            completed_at: None,
            instance_id: None,
            failure: None,
            poll_count: 0,
            hook_success_count: 0,
            hook_failure_count: 0,
            hook_rate_limited_count: 0,
        }
    }

    pub fn starting_now(now: DateTime<Utc>) -> Self {
        let now = now.to_rfc3339();
        let mut status = Self::new(WatchState::Starting, now.clone(), now);
        status.instance_id = Some(generate_instance_id());
        status
    }

    pub fn mark_healthy(&mut self, now: DateTime<Utc>) {
        let now = now.to_rfc3339();
        self.state = WatchState::Healthy;
        self.heartbeat_at = now.clone();
        self.completed_at = Some(now);
        self.failure = None;
    }

    pub fn mark_degraded(&mut self, now: DateTime<Utc>, reason: ClosedReason) {
        let now = now.to_rfc3339();
        self.state = WatchState::Degraded;
        self.heartbeat_at = now.clone();
        self.completed_at = Some(now);
        self.failure = Some(reason);
    }
}

impl HealthClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::StartingGrace => "starting_grace",
            Self::Stale => "stale",
            Self::Degraded => "degraded",
        }
    }
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

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "status_absent" => Self::StatusAbsent,
            "status_unreadable" => Self::StatusUnreadable,
            "status_invalid" => Self::StatusInvalid,
            "started_at_future" => Self::StartedAtFuture,
            "heartbeat_before_start" => Self::HeartbeatBeforeStart,
            "heartbeat_future" => Self::HeartbeatFuture,
            "completed_before_start" => Self::CompletedBeforeStart,
            "completed_after_heartbeat" => Self::CompletedAfterHeartbeat,
            "completed_future" => Self::CompletedFuture,
            "starting_expired" => Self::StartingExpired,
            "heartbeat_stale" => Self::HeartbeatStale,
            "ax_unavailable" => Self::AxUnavailable,
            "scrape_failed" => Self::ScrapeFailed,
            "hook_failed" => Self::HookFailed,
            "hook_timed_out" => Self::HookTimedOut,
            "hook_rate_limited" => Self::HookRateLimited,
            "config_invalid" => Self::ConfigInvalid,
            "logger_failed" => Self::LoggerFailed,
            _ => return None,
        })
    }
}

pub fn failure_reason(reason: Option<ClosedReason>) -> Option<ClosedReason> {
    reason.filter(|reason| {
        matches!(
            reason,
            ClosedReason::AxUnavailable
                | ClosedReason::ScrapeFailed
                | ClosedReason::HookFailed
                | ClosedReason::HookTimedOut
                | ClosedReason::HookRateLimited
                | ClosedReason::ConfigInvalid
                | ClosedReason::LoggerFailed
        )
    })
}

pub fn validate_watch_runtime_paths(status_path: &Path, log_path: &Path) -> Result<PathBuf> {
    let status_root = validate_managed_service_path(status_path, WATCH_STATUS_FILE, "status path")?;
    let log_root = validate_managed_service_path(log_path, WATCH_LOG_FILE, "log path")?;
    if status_root != log_root {
        bail!("watch managed paths must share one state root");
    }
    Ok(status_root)
}

pub fn validate_health_runtime_paths(
    status_path: &Path,
    alerts_path: &Path,
    log_path: &Path,
) -> Result<PathBuf> {
    let status_root = validate_managed_service_path(status_path, WATCH_STATUS_FILE, "status path")?;
    let alerts_root =
        validate_managed_service_path(alerts_path, HEALTH_ALERTS_FILE, "alerts path")?;
    let log_root = validate_managed_service_path(log_path, HEALTH_LOG_FILE, "log path")?;
    if status_root != alerts_root || status_root != log_root {
        bail!("health managed paths must share one state root");
    }
    Ok(status_root)
}

pub fn validate_hook_program_path(path: &Path) -> Result<()> {
    validate_absolute_service_path(path, "hook path")
}

pub fn validate_absolute_service_path(path: &Path, label: &str) -> Result<()> {
    let raw = path.to_string_lossy();
    if raw.contains(['\n', '\r']) {
        bail!("{label} must not contain newlines");
    }
    if !path.is_absolute() {
        bail!("{label} must be an absolute path");
    }
    if path.file_name().is_none() {
        bail!("{label} must target a file path");
    }
    Ok(())
}

pub fn write_status(path: &Path, status: &WatchStatusRecord) -> Result<()> {
    validate_watch_status_record(status)?;
    let bytes = serde_json::to_vec_pretty(status)?;
    atomic_write(path, &bytes)
}

pub fn write_config_invalid_status(path: &Path) -> Result<WatchStatusRecord> {
    let mut status = WatchStatusRecord::starting_now(Utc::now());
    status.mark_degraded(Utc::now(), ClosedReason::ConfigInvalid);
    write_status(path, &status)?;
    Ok(status)
}

pub fn watch_log_record(
    now: DateTime<Utc>,
    event: LogEvent,
    status: &WatchStatusRecord,
) -> Result<LogRecord> {
    let (started_delta_ms, heartbeat_delta_ms, completed_delta_ms, observed_age_ms) =
        status_deltas(now, status)?;
    Ok(LogRecord {
        schema_version: 1,
        ts: now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        service: ServiceName::Watch,
        event,
        state: Some(status.state),
        class: None,
        reason: status.failure,
        failure: failure_reason(status.failure),
        instance_id: status.instance_id.clone(),
        started_delta_ms,
        heartbeat_delta_ms,
        completed_delta_ms,
        observed_age_ms,
        notification_outcome: NotificationOutcome::None,
    })
}

pub fn health_log_record(
    now: DateTime<Utc>,
    classification: &StatusClassification,
    notification_outcome: NotificationOutcome,
) -> LogRecord {
    LogRecord {
        schema_version: 1,
        ts: now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        service: ServiceName::Health,
        event: LogEvent::HealthObservation,
        state: None,
        class: Some(classification.class),
        reason: classification.reason,
        failure: failure_reason(classification.reason),
        instance_id: classification.instance_id.clone(),
        started_delta_ms: classification.started_delta_ms,
        heartbeat_delta_ms: classification.heartbeat_delta_ms,
        completed_delta_ms: classification.completed_delta_ms,
        observed_age_ms: classification.observed_age_ms,
        notification_outcome,
    }
}

fn validate_managed_service_path(path: &Path, expected_file: &str, label: &str) -> Result<PathBuf> {
    validate_absolute_service_path(path, label)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("{label} must end with a UTF-8 file name"))?;
    if file_name != expected_file {
        bail!("{label} must end with {expected_file}");
    }
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("{label} must have a parent directory"))
}

fn validate_watch_status_record(status: &WatchStatusRecord) -> Result<()> {
    if status.schema_version != 1 {
        bail!(
            "unsupported watch status schema version {}",
            status.schema_version
        );
    }
    parse_rfc3339(&status.started_at).context("invalid started_at")?;
    parse_rfc3339(&status.heartbeat_at).context("invalid heartbeat_at")?;
    if let Some(completed_at) = status.completed_at.as_deref() {
        parse_rfc3339(completed_at).context("invalid completed_at")?;
    }
    if let Some(instance_id) = status.instance_id.as_deref() {
        validate_instance_id(instance_id)?;
    }
    match status.state {
        WatchState::Starting | WatchState::Healthy if status.failure.is_some() => {
            bail!("non-degraded watch status must not include failure")
        }
        WatchState::Degraded if failure_reason(status.failure).is_none() => {
            bail!("degraded watch status requires a failure reason")
        }
        _ => {}
    }
    Ok(())
}

fn status_deltas(now: DateTime<Utc>, status: &WatchStatusRecord) -> Result<(u64, u64, u64, u64)> {
    let started_at = parse_rfc3339(&status.started_at)?;
    let heartbeat_at = parse_rfc3339(&status.heartbeat_at)?;
    let completed_delta_ms = status
        .completed_at
        .as_deref()
        .map(parse_rfc3339)
        .transpose()?
        .map(|value| saturating_delta_ms(now, value))
        .unwrap_or(0);
    let started_delta_ms = saturating_delta_ms(now, started_at);
    let heartbeat_delta_ms = saturating_delta_ms(now, heartbeat_at);
    Ok((
        started_delta_ms,
        heartbeat_delta_ms,
        completed_delta_ms,
        heartbeat_delta_ms,
    ))
}

fn validate_instance_id(instance_id: &str) -> Result<()> {
    if instance_id.len() != 32 || !instance_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("instance_id must be 32 hex characters");
    }
    Ok(())
}

pub fn classify_status_path(now: DateTime<Utc>, status_path: &Path) -> StatusClassification {
    match fs::read_to_string(status_path) {
        Ok(text) => classify_status_text(now, &text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            closed_classification(HealthClass::Stale, ClosedReason::StatusAbsent)
        }
        Err(_) => closed_classification(HealthClass::Stale, ClosedReason::StatusUnreadable),
    }
}

pub fn classify_status_text(now: DateTime<Utc>, text: &str) -> StatusClassification {
    let status: WatchStatusRecord = match serde_json::from_str::<WatchStatusRecord>(text) {
        Ok(status) if status.schema_version == 1 => status,
        _ => return closed_classification(HealthClass::Stale, ClosedReason::StatusInvalid),
    };

    let started_at = match parse_rfc3339(&status.started_at) {
        Ok(value) => value,
        Err(_) => return closed_classification(HealthClass::Stale, ClosedReason::StatusInvalid),
    };
    let heartbeat_at = match parse_rfc3339(&status.heartbeat_at) {
        Ok(value) => value,
        Err(_) => return closed_classification(HealthClass::Stale, ClosedReason::StatusInvalid),
    };
    let completed_at = match status.completed_at.as_deref() {
        Some(value) => match parse_rfc3339(value) {
            Ok(parsed) => Some(parsed),
            Err(_) => {
                return closed_classification(HealthClass::Stale, ClosedReason::StatusInvalid)
            }
        },
        None => None,
    };
    if let Some(instance_id) = status.instance_id.as_deref() {
        if validate_instance_id(instance_id).is_err() {
            return closed_classification(HealthClass::Stale, ClosedReason::StatusInvalid);
        }
    }

    if started_at > now {
        return closed_classification(HealthClass::Stale, ClosedReason::StartedAtFuture);
    }
    if heartbeat_at < started_at {
        return closed_classification(HealthClass::Stale, ClosedReason::HeartbeatBeforeStart);
    }
    if heartbeat_at > now {
        return closed_classification(HealthClass::Stale, ClosedReason::HeartbeatFuture);
    }
    if let Some(completed_at) = completed_at {
        if completed_at < started_at {
            return closed_classification(HealthClass::Stale, ClosedReason::CompletedBeforeStart);
        }
        if completed_at > heartbeat_at {
            return closed_classification(
                HealthClass::Stale,
                ClosedReason::CompletedAfterHeartbeat,
            );
        }
        if completed_at > now {
            return closed_classification(HealthClass::Stale, ClosedReason::CompletedFuture);
        }
    }

    let started_delta_ms = saturating_delta_ms(now, started_at);
    let heartbeat_delta_ms = saturating_delta_ms(now, heartbeat_at);
    let completed_delta_ms = completed_at
        .map(|value| saturating_delta_ms(now, value))
        .unwrap_or(0);
    let observed_age_ms = heartbeat_delta_ms;

    if status.state == WatchState::Starting {
        if status.failure.is_some() {
            return closed_classification(HealthClass::Stale, ClosedReason::StatusInvalid);
        }
        if heartbeat_delta_ms <= STATUS_FRESH_SECS as u64 * 1000
            && started_delta_ms <= STATUS_FRESH_SECS as u64 * 1000
        {
            return StatusClassification {
                state: Some(status.state),
                class: HealthClass::StartingGrace,
                reason: None,
                fingerprint: None,
                instance_id: status.instance_id,
                started_delta_ms,
                heartbeat_delta_ms,
                completed_delta_ms,
                observed_age_ms,
            };
        }
        return StatusClassification {
            state: Some(status.state),
            class: HealthClass::Stale,
            reason: Some(ClosedReason::StartingExpired),
            fingerprint: Some(fingerprint(
                HealthClass::Stale,
                ClosedReason::StartingExpired,
            )),
            instance_id: status.instance_id,
            started_delta_ms,
            heartbeat_delta_ms,
            completed_delta_ms,
            observed_age_ms,
        };
    }

    if heartbeat_delta_ms > STATUS_FRESH_SECS as u64 * 1000 {
        return StatusClassification {
            state: Some(status.state),
            class: HealthClass::Stale,
            reason: Some(ClosedReason::HeartbeatStale),
            fingerprint: Some(fingerprint(
                HealthClass::Stale,
                ClosedReason::HeartbeatStale,
            )),
            instance_id: status.instance_id,
            started_delta_ms,
            heartbeat_delta_ms,
            completed_delta_ms,
            observed_age_ms,
        };
    }

    if status.state == WatchState::Healthy {
        if status.failure.is_some() {
            return closed_classification(HealthClass::Stale, ClosedReason::StatusInvalid);
        }
        return StatusClassification {
            state: Some(status.state),
            class: HealthClass::Healthy,
            reason: None,
            fingerprint: None,
            instance_id: status.instance_id,
            started_delta_ms,
            heartbeat_delta_ms,
            completed_delta_ms,
            observed_age_ms,
        };
    }

    let Some(reason) = failure_reason(status.failure) else {
        return closed_classification(HealthClass::Stale, ClosedReason::StatusInvalid);
    };
    StatusClassification {
        state: Some(status.state),
        class: HealthClass::Degraded,
        reason: Some(reason),
        fingerprint: Some(fingerprint(HealthClass::Degraded, reason)),
        instance_id: status.instance_id,
        started_delta_ms,
        heartbeat_delta_ms,
        completed_delta_ms,
        observed_age_ms,
    }
}

pub fn observe_health<N: Notifier>(
    now: DateTime<Utc>,
    status_path: &Path,
    alerts_path: &Path,
    log_path: &Path,
    notifier: &mut N,
) -> Result<HealthObservationResult> {
    let classification = classify_status_path(now, status_path);
    let mut alerts = read_alerts(alerts_path)?;
    let mut notification_outcome = NotificationOutcome::None;
    let mut attempt_key = None;

    if let Some(fp) = classification.fingerprint.as_deref() {
        if alerts.last_attempt_key.as_deref() != Some(fp) {
            let reason = classification
                .reason
                .expect("fingerprinted classifications require a reason");
            notification_outcome = notifier.notify(classification.class.as_str(), reason.as_str());
            attempt_key = Some(fp.to_owned());
            alerts.open_fingerprint = Some(fp.to_owned());
            alerts.last_attempt_key = Some(fp.to_owned());
            alerts.last_outcome = Some(notification_outcome);
            alerts.updated_at = Some(now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
            write_alerts(alerts_path, &alerts)?;
        }
    } else if classification.class == HealthClass::Healthy {
        if let Some(open_fingerprint) = alerts.open_fingerprint.clone() {
            let recovery_key = format!("recovered:{open_fingerprint}");
            if alerts.last_attempt_key.as_deref() != Some(recovery_key.as_str()) {
                let recovery_reason = parse_fingerprint_reason(&open_fingerprint)
                    .map(|value| value.as_str().to_owned())
                    .unwrap_or_else(|| "unknown".to_owned());
                notification_outcome = notifier.notify("recovered", &recovery_reason);
                attempt_key = Some(recovery_key.clone());
                alerts.open_fingerprint = None;
                alerts.last_attempt_key = Some(recovery_key);
                alerts.last_outcome = Some(notification_outcome);
                alerts.updated_at = Some(now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
                write_alerts(alerts_path, &alerts)?;
            }
        }
    }

    let record = health_log_record(now, &classification, notification_outcome);
    append_log_record(log_path, &record, system_time_from(now))?;

    Ok(HealthObservationResult {
        classification,
        notification_outcome,
        attempt_key,
    })
}

pub fn read_alerts(path: &Path) -> Result<HealthAlertsRecord> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let alerts: HealthAlertsRecord = serde_json::from_str(&text)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            if alerts.schema_version != 1 {
                bail!(
                    "unsupported alerts schema version {}",
                    alerts.schema_version
                );
            }
            Ok(alerts)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(HealthAlertsRecord::default()),
        Err(err) => Err(err).with_context(|| format!("failed to read {}", path.display())),
    }
}

pub fn write_alerts(path: &Path, alerts: &HealthAlertsRecord) -> Result<()> {
    if alerts.schema_version != 1 {
        bail!(
            "unsupported alerts schema version {}",
            alerts.schema_version
        );
    }
    let bytes = serde_json::to_vec_pretty(alerts)?;
    atomic_write(path, &bytes)
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

pub async fn run_direct_service_hook(
    hook_path: &Path,
    event: &ServiceHookEvent,
    timeout_secs: u64,
) -> std::result::Result<(), DirectServiceHookError> {
    validate_hook_program_path(hook_path).map_err(DirectServiceHookError::Failed)?;

    let payload =
        serde_json::to_vec(event).map_err(|error| DirectServiceHookError::Failed(error.into()))?;
    if payload.len() > HOOK_PAYLOAD_LIMIT_BYTES {
        return Err(DirectServiceHookError::Failed(anyhow!(
            "service hook payload exceeded {} bytes",
            HOOK_PAYLOAD_LIMIT_BYTES
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
        .env(
            "OPENKAKAO_MESSAGE_TYPE_LABEL",
            message_type_label(event.message_type),
        )
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| DirectServiceHookError::Failed(error.into()))?;

    let timeout = Duration::from_secs(timeout_secs.clamp(1, HOOK_TOTAL_DEADLINE_SECS));
    let result = tokio::time::timeout(timeout, async {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&payload)
                .await
                .map_err(|error| DirectServiceHookError::Failed(error.into()))?;
            stdin
                .shutdown()
                .await
                .map_err(|error| DirectServiceHookError::Failed(error.into()))?;
        }
        child
            .wait()
            .await
            .map_err(|error| DirectServiceHookError::Failed(error.into()))
    })
    .await;

    match result {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(DirectServiceHookError::Failed(anyhow!(
            "service hook exited with status {}",
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string())
        ))),
        Ok(Err(error)) => Err(error),
        Err(_) => {
            let _ = child.kill().await;
            Err(DirectServiceHookError::TimedOut)
        }
    }
}

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

pub fn validate_log_record_value(value: &Value) -> Result<()> {
    let record: LogRecord = serde_json::from_value(value.clone())?;
    validate_log_record(&record)
}

pub fn validate_log_record(record: &LogRecord) -> Result<()> {
    if record.schema_version != 1 {
        bail!("unsupported log schema version {}", record.schema_version);
    }
    parse_rfc3339(&record.ts).context("invalid ts")?;
    if let Some(instance_id) = &record.instance_id {
        validate_instance_id(instance_id)?;
    }
    if record.failure != failure_reason(record.reason) {
        bail!("failure must match the canonical failure subset of reason");
    }
    match record.event {
        LogEvent::HealthObservation => {
            if record.service != ServiceName::Health {
                bail!("health observations must use the health service label");
            }
            if record.state.is_some() {
                bail!("health observations must not set state");
            }
            if record.class.is_none() {
                bail!("health observations must set class");
            }
        }
        LogEvent::WatchStarted | LogEvent::WatchPollCompleted | LogEvent::WatchTerminalFailure => {
            if record.service != ServiceName::Watch {
                bail!("watch events must use the watch service label");
            }
            if record.state.is_none() {
                bail!("watch events must set state");
            }
            if record.class.is_some() {
                bail!("watch events must not set class");
            }
            if record.notification_outcome != NotificationOutcome::None {
                bail!("watch events must not set notification outcome");
            }
        }
    }
    let encoded = serde_json::to_vec(record)?;
    if encoded.len() > 512 {
        bail!("log record exceeds 512 bytes");
    }
    Ok(())
}

pub fn append_log_record(path: &Path, record: &LogRecord, now: SystemTime) -> Result<()> {
    validate_log_record(record)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    rotate_log_if_needed(path, now)?;

    if path.exists() {
        validate_owned_regular_file(path)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let line = serde_json::to_string(record)?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

pub fn rotate_log_if_needed(path: &Path, now: SystemTime) -> Result<()> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("failed to stat {}", path.display())),
    };
    validate_owned_regular_file(path)?;

    let should_rotate_for_size = metadata.len() > LOG_ROTATE_BYTES;
    let modified = metadata.modified().unwrap_or(now);
    let age = now.duration_since(modified).unwrap_or_default();
    let should_rotate_for_age = age > Duration::from_secs(LOG_ROTATE_AGE_SECS);
    if !should_rotate_for_size && !should_rotate_for_age {
        return Ok(());
    }

    remove_if_present(&suffixed_log_path(path, 3))?;
    move_if_present(&suffixed_log_path(path, 2), &suffixed_log_path(path, 3))?;
    move_if_present(&suffixed_log_path(path, 1), &suffixed_log_path(path, 2))?;
    move_if_present(path, &suffixed_log_path(path, 1))?;
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn move_if_present(from: &Path, to: &Path) -> Result<()> {
    let existed = match fs::symlink_metadata(from) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                bail!("{} is not a regular file", from.display());
            }
            true
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => return Err(err).with_context(|| format!("failed to stat {}", from.display())),
    };
    if !existed {
        return Ok(());
    }
    fs::rename(from, to)
        .with_context(|| format!("failed to rename {} -> {}", from.display(), to.display()))
}

fn suffixed_log_path(path: &Path, suffix: usize) -> PathBuf {
    let mut rendered = path.as_os_str().to_os_string();
    rendered.push(format!(".{suffix}"));
    PathBuf::from(rendered)
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let temp_path = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap().to_string_lossy()
    ));
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temp_path)
            .with_context(|| format!("failed to open {}", temp_path.display()))?;
        file.write_all(contents)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to rename {} -> {}",
            temp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn validate_owned_regular_file(path: &Path) -> Result<()> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("{} must not be a symlink", path.display());
    }
    if !metadata.file_type().is_file() {
        bail!("{} must be a regular file", path.display());
    }
    if metadata.uid() != unsafe { libc::geteuid() } {
        bail!("{} must be owned by the effective uid", path.display());
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        bail!("{} must not be group- or world-accessible", path.display());
    }
    Ok(())
}

fn generate_instance_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}
fn parse_rfc3339(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

fn saturating_delta_ms(now: DateTime<Utc>, then: DateTime<Utc>) -> u64 {
    now.signed_duration_since(then)
        .num_milliseconds()
        .max(0)
        .try_into()
        .unwrap_or(0)
}

fn closed_classification(class: HealthClass, reason: ClosedReason) -> StatusClassification {
    StatusClassification {
        state: None,
        class,
        reason: Some(reason),
        fingerprint: Some(fingerprint(class, reason)),
        instance_id: None,
        started_delta_ms: 0,
        heartbeat_delta_ms: 0,
        completed_delta_ms: 0,
        observed_age_ms: 0,
    }
}

fn system_time_from(value: DateTime<Utc>) -> SystemTime {
    let millis = value.timestamp_millis();
    if millis <= 0 {
        return SystemTime::UNIX_EPOCH;
    }
    SystemTime::UNIX_EPOCH + Duration::from_millis(millis as u64)
}
