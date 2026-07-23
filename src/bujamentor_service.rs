use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

    let reason = status.failure.unwrap_or(ClosedReason::StatusInvalid);
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

    let record = LogRecord {
        schema_version: 1,
        ts: now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        service: ServiceName::Health,
        event: LogEvent::HealthObservation,
        state: classification.state,
        class: Some(classification.class),
        reason: classification.reason,
        failure: classification.reason.filter(|reason| {
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
        }),
        instance_id: classification.instance_id.clone(),
        started_delta_ms: classification.started_delta_ms,
        heartbeat_delta_ms: classification.heartbeat_delta_ms,
        completed_delta_ms: classification.completed_delta_ms,
        observed_age_ms: classification.observed_age_ms,
        notification_outcome,
    };
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
        if instance_id.len() != 32 || !instance_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("instance_id must be 32 hex characters");
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
