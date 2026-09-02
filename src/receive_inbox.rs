use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rand::{Rng, RngCore};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

/// Result of atomically capturing an event by its stable idempotency key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureResult {
    Captured,
    Duplicate,
}

/// One event awaiting delivery to configured watch sinks.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingEvent {
    pub event_id: String,
    pub source: String,
    pub payload: Value,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryClaim {
    pub event: PendingEvent,
    pub(crate) claim_token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureDisposition {
    RetryScheduled { delay_secs: u64 },
    Quarantined,
}

const MAX_DELIVERY_ATTEMPTS: u32 = 8;
const RETRY_BASE_SECS: u64 = 5;
const RETRY_CAP_SECS: u64 = 5 * 60;

/// Durable local inbox for receive events.
///
/// Events are captured idempotently before external delivery. Callers consume
/// pending events and acknowledge them through the same interface; SQLite
/// schema, transactions, and retry bookkeeping stay inside the module.
pub struct ReceiveInbox {
    conn: Connection,
}

impl ReceiveInbox {
    pub fn open() -> Result<Self> {
        Self::open_at(&inbox_path()?)
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create receive inbox directory {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("open receive inbox {}", path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .with_context(|| format!("secure receive inbox {}", path.display()))?;
        }

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS receive_inbox (
                 event_id     TEXT PRIMARY KEY,
                 source       TEXT NOT NULL,
                 payload      TEXT NOT NULL,
                 state        TEXT NOT NULL DEFAULT 'pending'
                              CHECK (state IN ('pending', 'delivering', 'delivered', 'quarantined')),
                 captured_at  INTEGER NOT NULL,
                 attempts     INTEGER NOT NULL DEFAULT 0,
                 next_attempt_at INTEGER NOT NULL DEFAULT 0,
                 claimed_at   INTEGER,
                 lease_until  INTEGER,
                 claim_token  TEXT,
                 last_error   TEXT,
                 delivered_at INTEGER
             );
             CREATE INDEX IF NOT EXISTS idx_receive_inbox_pending
                 ON receive_inbox(state, next_attempt_at, captured_at, event_id);",
        )?;
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE receive_inbox
             SET state = 'pending', claimed_at = NULL, lease_until = NULL, claim_token = NULL,
                 last_error = COALESCE(last_error, 'delivery lease expired after interruption')
             WHERE state = 'delivering' AND (lease_until IS NULL OR lease_until <= ?1)",
            [now],
        )?;

        Ok(Self { conn })
    }

    /// Persist an event once. Re-capturing the same id never rewrites its
    /// payload or delivery state.
    pub fn capture<T: Serialize>(
        &self,
        event_id: &str,
        source: &str,
        payload: &T,
    ) -> Result<CaptureResult> {
        let payload = serde_json::to_string(payload)?;
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO receive_inbox
             (event_id, source, payload, state, captured_at)
             VALUES (?1, ?2, ?3, 'pending', ?4)",
            params![event_id, source, payload, chrono::Utc::now().timestamp()],
        )?;
        Ok(if inserted == 1 {
            CaptureResult::Captured
        } else {
            CaptureResult::Duplicate
        })
    }

    /// Atomically claim the next ready event. Concurrent watcher processes
    /// cannot receive the same claim; an interrupted claim is recovered after
    /// a bounded lease on the next open/claim.
    pub fn claim_next(&self, lease_secs: u64) -> Result<Option<DeliveryClaim>> {
        let now = chrono::Utc::now().timestamp();
        let lease_until = now.saturating_add(lease_secs.max(60).min(i64::MAX as u64) as i64);
        let transaction = self.conn.unchecked_transaction()?;
        transaction.execute(
            "UPDATE receive_inbox
             SET state = 'pending', claimed_at = NULL, lease_until = NULL, claim_token = NULL,
                 last_error = COALESCE(last_error, 'delivery lease expired after interruption')
             WHERE state = 'delivering' AND (lease_until IS NULL OR lease_until <= ?1)",
            [now],
        )?;
        let row = transaction
            .query_row(
                "SELECT event_id, source, payload, attempts
                 FROM receive_inbox
                 WHERE state = 'pending' AND next_attempt_at <= ?1
                 ORDER BY next_attempt_at, captured_at, event_id
                 LIMIT 1",
                [now],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u32>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((event_id, source, payload, attempts)) = row else {
            transaction.commit()?;
            return Ok(None);
        };
        let claim_token = random_claim_token();
        let claimed = transaction.execute(
            "UPDATE receive_inbox
             SET state = 'delivering', claimed_at = ?2, lease_until = ?3, claim_token = ?4
             WHERE event_id = ?1 AND state = 'pending' AND next_attempt_at <= ?2",
            params![event_id, now, lease_until, claim_token],
        )?;
        if claimed != 1 {
            anyhow::bail!("receive inbox claim changed concurrently");
        }
        transaction.commit()?;

        match serde_json::from_str(&payload) {
            Ok(payload) => Ok(Some(DeliveryClaim {
                event: PendingEvent {
                    event_id,
                    source,
                    payload,
                    attempts,
                },
                claim_token,
            })),
            Err(error) => {
                self.mark_claim_quarantined_parts(
                    &event_id,
                    &claim_token,
                    &format!("invalid persisted JSON payload: {error}"),
                )?;
                Ok(None)
            }
        }
    }

    /// Acknowledge successful delivery. The event remains in the ledger so a
    /// retained Notification Center record cannot be replayed after restart.
    pub fn mark_delivered(&self, claim: &DeliveryClaim) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE receive_inbox
             SET state = 'delivered', payload = 'null', delivered_at = ?3,
                 claimed_at = NULL, lease_until = NULL, claim_token = NULL, last_error = NULL
             WHERE event_id = ?1 AND state = 'delivering' AND claim_token = ?2",
            params![
                claim.event.event_id,
                claim.claim_token,
                chrono::Utc::now().timestamp()
            ],
        )?;
        if changed != 1 {
            anyhow::bail!("receive inbox delivery claim is stale; refusing acknowledgement");
        }
        Ok(())
    }

    /// Keep an event pending after an unsuccessful delivery and record enough
    /// state for operators to see that retries are occurring.
    pub fn mark_failed(&self, claim: &DeliveryClaim, error: &str) -> Result<FailureDisposition> {
        let attempts: u32 = self.conn.query_row(
            "SELECT attempts FROM receive_inbox
             WHERE event_id = ?1 AND state = 'delivering' AND claim_token = ?2",
            params![claim.event.event_id, claim.claim_token],
            |row| row.get(0),
        )?;
        let attempts = attempts.saturating_add(1);
        if attempts >= MAX_DELIVERY_ATTEMPTS {
            let changed = self.conn.execute(
                "UPDATE receive_inbox
                 SET state = 'quarantined', attempts = ?3, claimed_at = NULL,
                     lease_until = NULL, claim_token = NULL, last_error = ?4
                 WHERE event_id = ?1 AND state = 'delivering' AND claim_token = ?2",
                params![claim.event.event_id, claim.claim_token, attempts, error],
            )?;
            if changed != 1 {
                anyhow::bail!("receive inbox delivery claim is stale; refusing quarantine");
            }
            return Ok(FailureDisposition::Quarantined);
        }

        let base_delay = retry_delay_secs(attempts);
        let jitter = rand::thread_rng().gen_range(0..=base_delay / 4);
        let delay_secs = base_delay.saturating_add(jitter);
        let next_attempt_at = chrono::Utc::now()
            .timestamp()
            .saturating_add(delay_secs as i64);
        let changed = self.conn.execute(
            "UPDATE receive_inbox
             SET state = 'pending', attempts = ?3, claimed_at = NULL,
                 lease_until = NULL, claim_token = NULL,
                 next_attempt_at = ?4, last_error = ?5
             WHERE event_id = ?1 AND state = 'delivering' AND claim_token = ?2",
            params![
                claim.event.event_id,
                claim.claim_token,
                attempts,
                next_attempt_at,
                error
            ],
        )?;
        if changed != 1 {
            anyhow::bail!("receive inbox delivery claim is stale; refusing reschedule");
        }
        Ok(FailureDisposition::RetryScheduled { delay_secs })
    }

    /// Permanently remove a malformed event from the retry queue while
    /// retaining its payload and diagnostic details for local inspection.
    pub fn mark_claim_quarantined(&self, claim: &DeliveryClaim, error: &str) -> Result<()> {
        self.mark_claim_quarantined_parts(&claim.event.event_id, &claim.claim_token, error)
    }

    fn mark_claim_quarantined_parts(
        &self,
        event_id: &str,
        claim_token: &str,
        error: &str,
    ) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE receive_inbox
             SET state = 'quarantined', attempts = attempts + 1,
                 claimed_at = NULL, lease_until = NULL, claim_token = NULL, last_error = ?3
             WHERE event_id = ?1 AND state = 'delivering' AND claim_token = ?2",
            params![event_id, claim_token, error],
        )?;
        if changed != 1 {
            anyhow::bail!("receive inbox delivery claim is stale; refusing quarantine");
        }
        Ok(())
    }

    /// Bound the durable ledger while preserving tombstones for notifications
    /// still retained by Notification Center and a grace window for recently
    /// removed records.
    pub fn reconcile_terminal(
        &self,
        source: &str,
        retained_event_ids: &HashSet<String>,
        grace_secs: u64,
    ) -> Result<usize> {
        let cutoff = chrono::Utc::now()
            .timestamp()
            .saturating_sub(grace_secs.min(i64::MAX as u64) as i64);
        let mut stmt = self.conn.prepare(
            "SELECT event_id FROM receive_inbox
             WHERE source = ?1 AND state IN ('delivered', 'quarantined') AND captured_at <= ?2",
        )?;
        let candidates = stmt
            .query_map(params![source, cutoff], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        let transaction = self.conn.unchecked_transaction()?;
        let mut deleted = 0;
        for event_id in candidates {
            if !retained_event_ids.contains(&event_id) {
                deleted += transaction.execute(
                    "DELETE FROM receive_inbox
                     WHERE event_id = ?1 AND state IN ('delivered', 'quarantined')",
                    [&event_id],
                )?;
            }
        }
        transaction.commit()?;
        Ok(deleted)
    }
}

fn retry_delay_secs(attempts: u32) -> u64 {
    let exponent = attempts.saturating_sub(1).min(16);
    RETRY_BASE_SECS
        .saturating_mul(1_u64 << exponent)
        .min(RETRY_CAP_SECS)
}

fn random_claim_token() -> String {
    let mut bytes = [0_u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn inbox_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory for receive inbox")?;
    Ok(home
        .join(".config")
        .join("openkakao")
        .join("receive_inbox.db"))
}
