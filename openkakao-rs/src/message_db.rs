use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

/// Local SQLite message cache for offline search and analytics.
pub struct MessageDb {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct CachedMessage {
    pub chat_id: i64,
    pub log_id: i64,
    pub author_id: i64,
    pub author_name: String,
    pub message_type: i32,
    pub message: String,
    pub attachment: String,
    pub send_at: i64,
}

impl MessageDb {
    pub fn open() -> Result<Self> {
        let path = db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn =
            Connection::open(&path).with_context(|| format!("Failed to open {}", path.display()))?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                chat_id     INTEGER NOT NULL,
                log_id      INTEGER NOT NULL,
                author_id   INTEGER NOT NULL,
                author_name TEXT NOT NULL DEFAULT '',
                message_type INTEGER NOT NULL DEFAULT 1,
                message     TEXT NOT NULL DEFAULT '',
                attachment  TEXT NOT NULL DEFAULT '',
                send_at     INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (chat_id, log_id)
            );
            CREATE INDEX IF NOT EXISTS idx_messages_chat_send
                ON messages(chat_id, send_at);
            CREATE INDEX IF NOT EXISTS idx_messages_search
                ON messages(chat_id, message);

            CREATE TABLE IF NOT EXISTS chat_sync (
                chat_id         INTEGER PRIMARY KEY,
                last_log_id     INTEGER NOT NULL DEFAULT 0,
                synced_at       INTEGER NOT NULL DEFAULT 0,
                message_count   INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        Ok(())
    }

    /// Insert or replace a batch of messages.
    pub fn upsert_messages(&self, messages: &[CachedMessage]) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = tx.prepare_cached(
            "INSERT OR REPLACE INTO messages
             (chat_id, log_id, author_id, author_name, message_type, message, attachment, send_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;

        let mut count = 0;
        for m in messages {
            stmt.execute(params![
                m.chat_id,
                m.log_id,
                m.author_id,
                m.author_name,
                m.message_type,
                m.message,
                m.attachment,
                m.send_at,
            ])?;
            count += 1;
        }
        drop(stmt);
        tx.commit()?;
        Ok(count)
    }

    /// Update sync cursor for a chat.
    pub fn update_sync_cursor(&self, chat_id: i64, last_log_id: i64) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE chat_id = ?1",
            params![chat_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO chat_sync (chat_id, last_log_id, synced_at, message_count)
             VALUES (?1, ?2, ?3, ?4)",
            params![chat_id, last_log_id, now, count],
        )?;
        Ok(())
    }

    /// Get the last synced log_id for a chat.
    pub fn get_sync_cursor(&self, chat_id: i64) -> Result<Option<i64>> {
        let result = self
            .conn
            .query_row(
                "SELECT last_log_id FROM chat_sync WHERE chat_id = ?1",
                params![chat_id],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    /// Search messages by text pattern within a chat.
    pub fn search(&self, chat_id: i64, query: &str, limit: usize) -> Result<Vec<CachedMessage>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT chat_id, log_id, author_id, author_name, message_type, message, attachment, send_at
             FROM messages
             WHERE chat_id = ?1 AND message LIKE ?2
             ORDER BY send_at DESC
             LIMIT ?3",
        )?;

        let rows = stmt.query_map(params![chat_id, pattern, limit as i64], |row| {
            Ok(CachedMessage {
                chat_id: row.get(0)?,
                log_id: row.get(1)?,
                author_id: row.get(2)?,
                author_name: row.get(3)?,
                message_type: row.get(4)?,
                message: row.get(5)?,
                attachment: row.get(6)?,
                send_at: row.get(7)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Search messages across all chats.
    pub fn search_all(&self, query: &str, limit: usize) -> Result<Vec<CachedMessage>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT chat_id, log_id, author_id, author_name, message_type, message, attachment, send_at
             FROM messages
             WHERE message LIKE ?1
             ORDER BY send_at DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(CachedMessage {
                chat_id: row.get(0)?,
                log_id: row.get(1)?,
                author_id: row.get(2)?,
                author_name: row.get(3)?,
                message_type: row.get(4)?,
                message: row.get(5)?,
                attachment: row.get(6)?,
                send_at: row.get(7)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get message count per chat.
    pub fn chat_stats(&self) -> Result<Vec<(i64, i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT chat_id, COUNT(*), MAX(send_at) FROM messages GROUP BY chat_id ORDER BY MAX(send_at) DESC",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        let mut stats = Vec::new();
        for row in rows {
            stats.push(row?);
        }
        Ok(stats)
    }

    /// Get total message count.
    pub fn total_count(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .map_err(Into::into)
    }
}

fn db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not resolve home directory")?;
    Ok(home
        .join(".config")
        .join("openkakao")
        .join("messages.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> MessageDb {
        let conn = Connection::open_in_memory().unwrap();
        let db = MessageDb { conn };
        db.init_schema().unwrap();
        db
    }

    #[test]
    fn upsert_and_search() {
        let db = test_db();
        let msgs = vec![
            CachedMessage {
                chat_id: 1,
                log_id: 100,
                author_id: 42,
                author_name: "Alice".into(),
                message_type: 1,
                message: "hello world".into(),
                attachment: String::new(),
                send_at: 1700000000,
            },
            CachedMessage {
                chat_id: 1,
                log_id: 101,
                author_id: 43,
                author_name: "Bob".into(),
                message_type: 1,
                message: "goodbye world".into(),
                attachment: String::new(),
                send_at: 1700000010,
            },
        ];

        assert_eq!(db.upsert_messages(&msgs).unwrap(), 2);
        assert_eq!(db.total_count().unwrap(), 2);

        let results = db.search(1, "hello", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].author_name, "Alice");

        let results = db.search(1, "world", 10).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn sync_cursor_roundtrip() {
        let db = test_db();
        assert!(db.get_sync_cursor(1).unwrap().is_none());
        db.update_sync_cursor(1, 500).unwrap();
        assert_eq!(db.get_sync_cursor(1).unwrap(), Some(500));
    }

    #[test]
    fn upsert_is_idempotent() {
        let db = test_db();
        let msg = CachedMessage {
            chat_id: 1,
            log_id: 100,
            author_id: 42,
            author_name: "Alice".into(),
            message_type: 1,
            message: "hello".into(),
            attachment: String::new(),
            send_at: 1700000000,
        };

        db.upsert_messages(&[msg.clone()]).unwrap();
        db.upsert_messages(&[msg]).unwrap();
        assert_eq!(db.total_count().unwrap(), 1);
    }

    #[test]
    fn cross_chat_search() {
        let db = test_db();
        let msgs = vec![
            CachedMessage {
                chat_id: 1,
                log_id: 100,
                author_id: 42,
                author_name: "Alice".into(),
                message_type: 1,
                message: "meeting at 3pm".into(),
                attachment: String::new(),
                send_at: 1700000000,
            },
            CachedMessage {
                chat_id: 2,
                log_id: 200,
                author_id: 43,
                author_name: "Bob".into(),
                message_type: 1,
                message: "meeting postponed".into(),
                attachment: String::new(),
                send_at: 1700000010,
            },
        ];

        db.upsert_messages(&msgs).unwrap();
        let results = db.search_all("meeting", 10).unwrap();
        assert_eq!(results.len(), 2);
    }
}
