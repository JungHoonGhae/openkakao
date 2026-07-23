use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime};

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use openkakao_cli::bujamentor_service::{
    append_log_record, classify_status_path, classify_status_text, validate_health_runtime_paths,
    validate_log_record_value, validate_watch_runtime_paths, write_config_invalid_status,
    ClosedReason, HealthClass, LogEvent, LogRecord, NotificationOutcome, ServiceName,
};
use serde_json::json;
use tempfile::tempdir;

fn healthy_status_text(started_at: &str, heartbeat_at: &str) -> String {
    json!({
        "schema_version": 1,
        "state": "healthy",
        "started_at": started_at,
        "heartbeat_at": heartbeat_at,
        "instance_id": "0123456789abcdef0123456789abcdef"
    })
    .to_string()
}

#[test]
fn healthy_boundary_is_fresh_at_35_seconds_and_stale_after() {
    let now = Utc.with_ymd_and_hms(2026, 7, 23, 10, 0, 35).unwrap();
    let started_at = (now - ChronoDuration::seconds(70)).to_rfc3339();
    let heartbeat_at = (now - ChronoDuration::seconds(35)).to_rfc3339();

    let fresh = classify_status_text(now, &healthy_status_text(&started_at, &heartbeat_at));
    assert_eq!(fresh.class, HealthClass::Healthy);
    assert_eq!(fresh.reason, None);
    assert_eq!(fresh.fingerprint, None);

    let stale = classify_status_text(
        now + ChronoDuration::milliseconds(1),
        &healthy_status_text(&started_at, &heartbeat_at),
    );
    assert_eq!(stale.class, HealthClass::Stale);
    assert_eq!(stale.reason, Some(ClosedReason::HeartbeatStale));
    assert_eq!(
        stale.fingerprint.as_deref(),
        Some("v1:stale:heartbeat_stale")
    );
}

#[test]
fn starting_records_expire_after_grace_window() {
    let now = Utc.with_ymd_and_hms(2026, 7, 23, 10, 0, 35).unwrap();
    let started_at = (now - ChronoDuration::seconds(35)).to_rfc3339();
    let heartbeat_at = (now - ChronoDuration::seconds(35)).to_rfc3339();
    let status = json!({
        "schema_version": 1,
        "state": "starting",
        "started_at": started_at,
        "heartbeat_at": heartbeat_at
    })
    .to_string();

    let grace = classify_status_text(now, &status);
    assert_eq!(grace.class, HealthClass::StartingGrace);
    assert_eq!(grace.reason, None);

    let expired = classify_status_text(now + ChronoDuration::milliseconds(1), &status);
    assert_eq!(expired.class, HealthClass::Stale);
    assert_eq!(expired.reason, Some(ClosedReason::StartingExpired));
}

#[test]
fn temporal_invalid_records_keep_named_stale_reasons() {
    let now = Utc.with_ymd_and_hms(2026, 7, 23, 10, 0, 0).unwrap();
    let status = json!({
        "schema_version": 1,
        "state": "healthy",
        "started_at": (now + ChronoDuration::seconds(1)).to_rfc3339(),
        "heartbeat_at": now.to_rfc3339(),
    })
    .to_string();

    let classified = classify_status_text(now, &status);
    assert_eq!(classified.class, HealthClass::Stale);
    assert_eq!(classified.reason, Some(ClosedReason::StartedAtFuture));
    assert_eq!(
        classified.fingerprint.as_deref(),
        Some("v1:stale:started_at_future")
    );
}

#[test]
fn log_record_validator_rejects_unknown_fields_and_invalid_instance_ids() {
    let mut invalid = json!({
        "schema_version": 1,
        "ts": "2026-07-23T10:00:00.000Z",
        "service": "health",
        "event": "health_observation",
        "state": null,
        "class": "stale",
        "reason": "heartbeat_stale",
        "failure": null,
        "instance_id": "short",
        "started_delta_ms": 0,
        "heartbeat_delta_ms": 36000,
        "completed_delta_ms": 0,
        "observed_age_ms": 36000,
        "notification_outcome": "submitted"
    });
    assert!(validate_log_record_value(&invalid).is_err());

    invalid["instance_id"] = json!("0123456789abcdef0123456789abcdef");
    invalid["failure"] = json!("hook_failed");
    assert!(validate_log_record_value(&invalid).is_err());

    invalid["failure"] = serde_json::Value::Null;
    invalid["state"] = json!("healthy");
    assert!(validate_log_record_value(&invalid).is_err());

    invalid["state"] = serde_json::Value::Null;
    invalid["extra"] = json!(true);
    assert!(validate_log_record_value(&invalid).is_err());
}

#[test]
fn config_invalid_status_round_trips_through_health_classifier() {
    let temp = tempdir().unwrap();
    let status_path = temp.path().join("watch-status.json");
    let persisted = write_config_invalid_status(&status_path).unwrap();
    assert_eq!(persisted.failure, Some(ClosedReason::ConfigInvalid));

    let classified = classify_status_path(Utc::now(), &status_path);
    assert_eq!(classified.class, HealthClass::Degraded);
    assert_eq!(classified.reason, Some(ClosedReason::ConfigInvalid));
}

#[test]
fn managed_paths_require_canonical_names_and_shared_roots() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("state");
    let other = temp.path().join("other");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&other).unwrap();

    assert!(
        validate_watch_runtime_paths(&root.join("watch-status.json"), &root.join("watch.log"))
            .is_ok()
    );
    assert!(
        validate_watch_runtime_paths(&root.join("status.json"), &root.join("watch.log")).is_err()
    );
    assert!(validate_health_runtime_paths(
        &root.join("watch-status.json"),
        &other.join("health-alerts.json"),
        &root.join("health.log"),
    )
    .is_err());
}

#[test]
fn log_rotation_preserves_sparse_holes() {
    let temp = tempdir().unwrap();
    let log_path = temp.path().join("health.log");
    let older_path = temp.path().join("health.log.2");
    fs::write(&log_path, vec![b'a'; 70_000]).unwrap();
    fs::write(&older_path, b"old-two\n").unwrap();
    fs::set_permissions(&log_path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::set_permissions(&older_path, fs::Permissions::from_mode(0o600)).unwrap();

    let record = LogRecord {
        schema_version: 1,
        ts: "2026-07-23T10:00:00.000Z".to_owned(),
        service: ServiceName::Health,
        event: LogEvent::HealthObservation,
        state: None,
        class: Some(HealthClass::Healthy),
        reason: None,
        failure: None,
        instance_id: Some("0123456789abcdef0123456789abcdef".to_owned()),
        started_delta_ms: 1,
        heartbeat_delta_ms: 1,
        completed_delta_ms: 0,
        observed_age_ms: 1,
        notification_outcome: NotificationOutcome::None,
    };
    append_log_record(
        &log_path,
        &record,
        SystemTime::UNIX_EPOCH + Duration::from_secs(1),
    )
    .unwrap();

    assert!(temp.path().join("health.log.1").exists());
    assert!(!temp.path().join("health.log.2").exists());
    assert!(temp.path().join("health.log.3").exists());

    let active = fs::read_to_string(&log_path).unwrap();
    assert!(active.contains("\"event\":\"health_observation\""));
    let rotated = fs::metadata(temp.path().join("health.log.1")).unwrap();
    assert!(rotated.len() >= 70_000);
}
