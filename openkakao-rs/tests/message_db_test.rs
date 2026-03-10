use openkakao_rs::message_db::{CachedMessage, MessageDb};

fn test_msg(chat_id: i64, log_id: i64, author: &str, msg: &str, send_at: i64) -> CachedMessage {
    CachedMessage {
        chat_id,
        log_id,
        author_id: 42,
        author_name: author.to_string(),
        message_type: 1,
        message: msg.to_string(),
        attachment: String::new(),
        send_at,
    }
}

#[test]
fn open_creates_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let db = MessageDb::open_at(&path).unwrap();

    // Should be able to query without error
    assert_eq!(db.total_count().unwrap(), 0);
}

#[test]
fn insert_query_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let db = MessageDb::open_at(&dir.path().join("test.db")).unwrap();

    let msgs = vec![
        test_msg(1, 100, "Alice", "hello world", 1700000000),
        test_msg(1, 101, "Bob", "goodbye world", 1700000010),
    ];

    assert_eq!(db.upsert_messages(&msgs).unwrap(), 2);
    assert_eq!(db.total_count().unwrap(), 2);

    let results = db.search(1, "hello", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].author_name, "Alice");
    assert_eq!(results[0].log_id, 100);
}

#[test]
fn keyword_search() {
    let dir = tempfile::tempdir().unwrap();
    let db = MessageDb::open_at(&dir.path().join("test.db")).unwrap();

    let msgs = vec![
        test_msg(1, 100, "Alice", "meeting at 3pm", 1700000000),
        test_msg(1, 101, "Bob", "lunch at noon", 1700000010),
        test_msg(1, 102, "Carol", "meeting postponed", 1700000020),
    ];

    db.upsert_messages(&msgs).unwrap();

    let results = db.search(1, "meeting", 10).unwrap();
    assert_eq!(results.len(), 2);

    let results = db.search(1, "lunch", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].author_name, "Bob");
}

#[test]
fn duplicate_insert_is_upsert() {
    let dir = tempfile::tempdir().unwrap();
    let db = MessageDb::open_at(&dir.path().join("test.db")).unwrap();

    let msg = test_msg(1, 100, "Alice", "original", 1700000000);
    db.upsert_messages(&[msg]).unwrap();
    assert_eq!(db.total_count().unwrap(), 1);

    // Upsert with updated message
    let msg_updated = test_msg(1, 100, "Alice", "updated", 1700000000);
    db.upsert_messages(&[msg_updated]).unwrap();
    assert_eq!(db.total_count().unwrap(), 1);

    let results = db.search(1, "updated", 10).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn cross_chat_search() {
    let dir = tempfile::tempdir().unwrap();
    let db = MessageDb::open_at(&dir.path().join("test.db")).unwrap();

    let msgs = vec![
        test_msg(1, 100, "Alice", "meeting at 3pm", 1700000000),
        test_msg(2, 200, "Bob", "meeting postponed", 1700000010),
    ];

    db.upsert_messages(&msgs).unwrap();

    let results = db.search_all("meeting", 10).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn sync_cursor_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let db = MessageDb::open_at(&dir.path().join("test.db")).unwrap();

    assert!(db.get_sync_cursor(1).unwrap().is_none());
    db.update_sync_cursor(1, 500).unwrap();
    assert_eq!(db.get_sync_cursor(1).unwrap(), Some(500));

    // Update cursor
    db.update_sync_cursor(1, 1000).unwrap();
    assert_eq!(db.get_sync_cursor(1).unwrap(), Some(1000));
}

#[test]
fn chat_stats() {
    let dir = tempfile::tempdir().unwrap();
    let db = MessageDb::open_at(&dir.path().join("test.db")).unwrap();

    let msgs = vec![
        test_msg(1, 100, "Alice", "hello", 1700000000),
        test_msg(1, 101, "Bob", "hi", 1700000010),
        test_msg(2, 200, "Carol", "hey", 1700000020),
    ];

    db.upsert_messages(&msgs).unwrap();

    let stats = db.chat_stats().unwrap();
    assert_eq!(stats.len(), 2);
    // Ordered by max(send_at) DESC
    assert_eq!(stats[0].0, 2); // chat_id=2 has the latest message
    assert_eq!(stats[0].1, 1); // 1 message in chat 2
    assert_eq!(stats[1].0, 1); // chat_id=1
    assert_eq!(stats[1].1, 2); // 2 messages in chat 1
}

#[test]
fn fts5_search_via_open_at() {
    let dir = tempfile::tempdir().unwrap();
    let db = MessageDb::open_at(&dir.path().join("test.db")).unwrap();

    let msgs = vec![
        test_msg(1, 100, "Alice", "hello world", 1700000000),
        test_msg(1, 101, "Bob", "goodbye world", 1700000010),
        test_msg(1, 102, "Carol", "hello again", 1700000020),
    ];
    db.upsert_messages(&msgs).unwrap();

    // search() lazily creates FTS5 table and uses it
    let results = db.search(1, "hello", 10).unwrap();
    assert_eq!(results.len(), 2);

    // search_all() also uses FTS5
    let results = db.search_all("world", 10).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn fts5_insert_after_migration() {
    let dir = tempfile::tempdir().unwrap();
    let db = MessageDb::open_at(&dir.path().join("test.db")).unwrap();

    // Trigger FTS migration with initial data
    let msgs = vec![test_msg(1, 100, "Alice", "initial message", 1700000000)];
    db.upsert_messages(&msgs).unwrap();
    let _ = db.search(1, "initial", 10).unwrap(); // triggers FTS

    // Insert new message -- should be auto-indexed by trigger
    let new_msgs = vec![test_msg(1, 101, "Bob", "second message", 1700000010)];
    db.upsert_messages(&new_msgs).unwrap();

    let results = db.search(1, "second", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].author_name, "Bob");
}
