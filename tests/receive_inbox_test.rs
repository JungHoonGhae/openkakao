use std::collections::HashSet;

use openkakao_cli::receive_inbox::{CaptureResult, FailureDisposition, ReceiveInbox};
use serde_json::json;

#[test]
fn captured_event_is_pending_and_duplicate_capture_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let inbox = ReceiveInbox::open_at(&dir.path().join("receive.db")).unwrap();
    let payload = json!({
        "event_type": "notif_message",
        "chat_id": 42,
        "log_id": 99,
        "message": "hello"
    });

    assert_eq!(
        inbox.capture("notif:42:99", "notif", &payload).unwrap(),
        CaptureResult::Captured
    );
    assert_eq!(
        inbox.capture("notif:42:99", "notif", &payload).unwrap(),
        CaptureResult::Duplicate
    );

    let pending = inbox.claim_next(60).unwrap().unwrap().event;
    assert_eq!(pending.event_id, "notif:42:99");
    assert_eq!(pending.source, "notif");
    assert_eq!(pending.payload, payload);
    assert_eq!(pending.attempts, 0);
}

#[test]
fn delivered_event_stays_acknowledged_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receive.db");
    let inbox = ReceiveInbox::open_at(&path).unwrap();
    let payload = json!({"chat_id": 42, "log_id": 100});
    inbox.capture("notif:42:100", "notif", &payload).unwrap();

    let claim = inbox.claim_next(60).unwrap().unwrap();
    assert_eq!(claim.event.event_id, "notif:42:100");
    inbox.mark_delivered(&claim).unwrap();
    assert!(inbox.claim_next(60).unwrap().is_none());
    drop(inbox);

    let reopened = ReceiveInbox::open_at(&path).unwrap();
    assert!(reopened.claim_next(60).unwrap().is_none());
    assert_eq!(
        reopened.capture("notif:42:100", "notif", &payload).unwrap(),
        CaptureResult::Duplicate
    );
}

#[test]
fn deferred_delivery_remains_pending_and_records_the_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let inbox = ReceiveInbox::open_at(&dir.path().join("receive.db")).unwrap();
    let payload = json!({"chat_id": 42, "log_id": 101});
    inbox.capture("notif:42:101", "notif", &payload).unwrap();
    let claim = inbox.claim_next(60).unwrap().unwrap();
    assert_eq!(claim.event.event_id, "notif:42:101");

    let disposition = inbox.mark_failed(&claim, "webhook unavailable").unwrap();
    assert!(matches!(
        disposition,
        FailureDisposition::RetryScheduled { delay_secs } if delay_secs >= 5
    ));

    assert!(inbox.claim_next(60).unwrap().is_none());
    drop(inbox);
    let conn = rusqlite::Connection::open(dir.path().join("receive.db")).unwrap();
    let attempts: u32 = conn
        .query_row(
            "SELECT attempts FROM receive_inbox WHERE event_id = ?1",
            ["notif:42:101"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(attempts, 1);
}

#[test]
fn malformed_persisted_json_is_quarantined_without_blocking_later_events() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receive.db");
    let inbox = ReceiveInbox::open_at(&path).unwrap();
    inbox
        .capture("notif:42:broken", "notif", &json!({"message": "broken"}))
        .unwrap();
    inbox
        .capture("notif:42:valid", "notif", &json!({"message": "valid"}))
        .unwrap();
    drop(inbox);

    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "UPDATE receive_inbox SET payload = 'not-json' WHERE event_id = ?1",
        ["notif:42:broken"],
    )
    .unwrap();
    drop(conn);

    let inbox = ReceiveInbox::open_at(&path).unwrap();
    assert!(inbox.claim_next(60).unwrap().is_none());
    let pending = inbox.claim_next(60).unwrap().unwrap().event;
    assert_eq!(pending.event_id, "notif:42:valid");
    drop(inbox);

    let conn = rusqlite::Connection::open(&path).unwrap();
    let (state, attempts, error): (String, u32, String) = conn
        .query_row(
            "SELECT state, attempts, last_error FROM receive_inbox WHERE event_id = ?1",
            ["notif:42:broken"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(state, "quarantined");
    assert_eq!(attempts, 1);
    assert!(error.contains("invalid persisted JSON payload"));
}

#[test]
fn delivery_claim_is_atomic_across_connections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receive.db");
    let first = ReceiveInbox::open_at(&path).unwrap();
    first
        .capture("notif:42:claim", "notif", &json!({"message": "once"}))
        .unwrap();
    let second = ReceiveInbox::open_at(&path).unwrap();

    assert_eq!(
        first.claim_next(60).unwrap().unwrap().event.event_id,
        "notif:42:claim"
    );
    assert!(second.claim_next(60).unwrap().is_none());
}

#[test]
fn pending_claim_query_uses_covering_order_without_temp_sort() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receive.db");
    let _inbox = ReceiveInbox::open_at(&path).unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();

    let mut stmt = conn
        .prepare(
            "EXPLAIN QUERY PLAN
             SELECT event_id, source, payload, attempts
             FROM receive_inbox
             WHERE state = 'pending' AND next_attempt_at <= ?1
             ORDER BY next_attempt_at, captured_at, event_id
             LIMIT 1",
        )
        .unwrap();
    let details = stmt
        .query_map([chrono::Utc::now().timestamp()], |row| {
            row.get::<_, String>(3)
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .join("\n");

    assert!(details.contains("idx_receive_inbox_pending"), "{details}");
    assert!(!details.contains("TEMP B-TREE"), "{details}");
}

#[test]
fn expired_claim_cannot_acknowledge_a_newer_claim() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receive.db");
    let first = ReceiveInbox::open_at(&path).unwrap();
    first
        .capture("notif:42:lease", "notif", &json!({"message": "once"}))
        .unwrap();
    let stale_claim = first.claim_next(60).unwrap().unwrap();

    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute(
        "UPDATE receive_inbox SET lease_until = 0 WHERE event_id = ?1",
        ["notif:42:lease"],
    )
    .unwrap();
    drop(conn);

    let second = ReceiveInbox::open_at(&path).unwrap();
    let current_claim = second.claim_next(60).unwrap().unwrap();
    assert!(first.mark_delivered(&stale_claim).is_err());
    second.mark_delivered(&current_claim).unwrap();
}

#[test]
fn terminal_reconciliation_preserves_retained_ids_and_prunes_removed_ids() {
    let dir = tempfile::tempdir().unwrap();
    let inbox = ReceiveInbox::open_at(&dir.path().join("receive.db")).unwrap();
    for id in ["notif:42:keep", "notif:42:prune"] {
        inbox.capture(id, "notif", &json!({"message": id})).unwrap();
        let claim = inbox.claim_next(60).unwrap().unwrap();
        assert_eq!(claim.event.event_id, id);
        inbox.mark_delivered(&claim).unwrap();
    }

    let retained = HashSet::from(["notif:42:keep".to_string()]);
    assert_eq!(inbox.reconcile_terminal("notif", &retained, 0).unwrap(), 1);
    assert_eq!(
        inbox
            .capture("notif:42:keep", "notif", &json!({"message": "duplicate"}))
            .unwrap(),
        CaptureResult::Duplicate
    );
    assert_eq!(
        inbox
            .capture("notif:42:prune", "notif", &json!({"message": "new again"}))
            .unwrap(),
        CaptureResult::Captured
    );
}

#[test]
fn repeated_failures_quarantine_poison_event_and_unblock_later_delivery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receive.db");
    let inbox = ReceiveInbox::open_at(&path).unwrap();
    inbox
        .capture("notif:42:00-poison", "notif", &json!({"message": "poison"}))
        .unwrap();
    inbox
        .capture("notif:42:99-later", "notif", &json!({"message": "later"}))
        .unwrap();

    for attempt in 1..=8 {
        let claim = inbox.claim_next(60).unwrap().unwrap();
        assert_eq!(claim.event.event_id, "notif:42:00-poison");
        let disposition = inbox.mark_failed(&claim, "permanent failure").unwrap();
        if attempt < 8 {
            assert!(matches!(
                disposition,
                FailureDisposition::RetryScheduled { .. }
            ));
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute(
                "UPDATE receive_inbox SET next_attempt_at = 0 WHERE event_id = ?1",
                ["notif:42:00-poison"],
            )
            .unwrap();
        } else {
            assert_eq!(disposition, FailureDisposition::Quarantined);
        }
    }

    assert_eq!(
        inbox.claim_next(60).unwrap().unwrap().event.event_id,
        "notif:42:99-later"
    );
}
