use std::fs;

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use openkakao_cli::bujamentor_service::{
    observe_health, NotificationOutcome, Notifier, WatchState, WatchStatusRecord,
};
use tempfile::tempdir;

struct RecordingNotifier {
    calls: Vec<(String, String)>,
    outcome: NotificationOutcome,
}

impl Default for RecordingNotifier {
    fn default() -> Self {
        Self {
            calls: Vec::new(),
            outcome: NotificationOutcome::None,
        }
    }
}

impl Notifier for RecordingNotifier {
    fn notify(&mut self, class: &str, reason: &str) -> NotificationOutcome {
        self.calls.push((class.to_owned(), reason.to_owned()));
        self.outcome
    }
}

fn write_status(path: &std::path::Path, status: &WatchStatusRecord) {
    fs::write(path, serde_json::to_vec_pretty(status).unwrap()).unwrap();
}

#[test]
fn observe_health_dedupes_open_alerts_and_emits_single_recovery() {
    let temp = tempdir().unwrap();
    let status_path = temp.path().join("watch-status.json");
    let alerts_path = temp.path().join("health-alerts.json");
    let log_path = temp.path().join("health.log");
    let now = Utc.with_ymd_and_hms(2026, 7, 23, 10, 0, 40).unwrap();
    let mut notifier = RecordingNotifier {
        outcome: NotificationOutcome::Submitted,
        ..Default::default()
    };

    let first = observe_health(now, &status_path, &alerts_path, &log_path, &mut notifier).unwrap();
    assert_eq!(first.attempt_key.as_deref(), Some("v1:stale:status_absent"));
    assert_eq!(
        notifier.calls,
        vec![("stale".to_owned(), "status_absent".to_owned())]
    );

    let second = observe_health(
        now + ChronoDuration::seconds(15),
        &status_path,
        &alerts_path,
        &log_path,
        &mut notifier,
    )
    .unwrap();
    assert_eq!(second.notification_outcome, NotificationOutcome::None);
    assert_eq!(notifier.calls.len(), 1);

    let mut healthy = WatchStatusRecord::new(
        WatchState::Healthy,
        (now - ChronoDuration::seconds(20)).to_rfc3339(),
        (now + ChronoDuration::seconds(30)).to_rfc3339(),
    );
    healthy.instance_id = Some("0123456789abcdef0123456789abcdef".to_owned());
    write_status(&status_path, &healthy);

    let recovery = observe_health(
        now + ChronoDuration::seconds(30),
        &status_path,
        &alerts_path,
        &log_path,
        &mut notifier,
    )
    .unwrap();
    assert_eq!(
        recovery.attempt_key.as_deref(),
        Some("recovered:v1:stale:status_absent")
    );
    assert_eq!(
        notifier.calls,
        vec![
            ("stale".to_owned(), "status_absent".to_owned()),
            ("recovered".to_owned(), "status_absent".to_owned())
        ]
    );

    let steady = observe_health(
        now + ChronoDuration::seconds(45),
        &status_path,
        &alerts_path,
        &log_path,
        &mut notifier,
    )
    .unwrap();
    assert_eq!(steady.notification_outcome, NotificationOutcome::None);
    assert_eq!(notifier.calls.len(), 2);
}

#[test]
fn observe_health_logs_each_observation() {
    let temp = tempdir().unwrap();
    let status_path = temp.path().join("watch-status.json");
    let alerts_path = temp.path().join("health-alerts.json");
    let log_path = temp.path().join("health.log");
    let now = Utc.with_ymd_and_hms(2026, 7, 23, 10, 0, 35).unwrap();

    let status = WatchStatusRecord::new(
        WatchState::Degraded,
        (now - ChronoDuration::seconds(10)).to_rfc3339(),
        now.to_rfc3339(),
    );
    let mut status = status;
    status.failure = Some(openkakao_cli::bujamentor_service::ClosedReason::HookFailed);
    status.instance_id = Some("0123456789abcdef0123456789abcdef".to_owned());
    write_status(&status_path, &status);

    let mut notifier = RecordingNotifier {
        outcome: NotificationOutcome::Rejected,
        ..Default::default()
    };
    observe_health(now, &status_path, &alerts_path, &log_path, &mut notifier).unwrap();

    let log = fs::read_to_string(&log_path).unwrap();
    assert!(log.contains("\"service\":\"health\""));
    assert!(log.contains("\"reason\":\"hook_failed\""));
    assert!(log.contains("\"notification_outcome\":\"rejected\""));
}
