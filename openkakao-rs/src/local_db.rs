use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use base64::Engine;
use rusqlite::Connection;
use serde::Serialize;
use sha2::Digest;

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LocalChat {
    pub chat_id: i64,
    pub chat_type: i32,
    pub chat_name: String,
    pub active_members_count: i32,
    pub last_log_id: i64,
    pub last_updated_at: i64,
    pub unread_count: i64,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalMessage {
    pub log_id: i64,
    pub chat_id: i64,
    pub author_id: i64,
    pub sender_name: String,
    pub message: String,
    pub message_type: i32,
    pub sent_at: i64,
}

// ---------------------------------------------------------------------------
// Device info extraction
// ---------------------------------------------------------------------------

fn get_platform_uuid() -> Result<String> {
    let output = Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .context("Failed to run ioreg")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("IOPlatformUUID") {
            // Extract UUID pattern
            if let Some(start) = line.find('"') {
                let rest = &line[start + 1..];
                if let Some(end) = rest.find('"') {
                    let val = &rest[..end];
                    // Find the actual UUID within (skip the key name)
                    if val == "IOPlatformUUID" {
                        continue;
                    }
                }
            }
            // Pattern: "IOPlatformUUID" = "XXXXXXXX-..."
            let parts: Vec<&str> = line.split('"').collect();
            if parts.len() >= 4 {
                let uuid = parts[3].trim();
                if uuid.len() >= 36 {
                    return Ok(uuid.to_string());
                }
            }
        }
    }
    anyhow::bail!("IOPlatformUUID not found in ioreg output")
}

fn get_user_id_from_plist() -> Result<i64> {
    let home = dirs::home_dir().context("No home directory")?;

    // Strategy 1: Container preferences with hex suffix
    let container_prefs =
        home.join("Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Preferences");
    if container_prefs.exists() {
        if let Ok(entries) = std::fs::read_dir(&container_prefs) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("com.kakao.KakaoTalkMac.") && name.ends_with(".plist") {
                    if let Ok(user_id) = extract_user_id_from_plist(&entry.path()) {
                        return Ok(user_id);
                    }
                }
            }
        }
    }

    // Strategy 2: Global preferences
    let global_plist = home.join("Library/Preferences/com.kakao.KakaoTalkMac.plist");
    if global_plist.exists() {
        if let Ok(user_id) = extract_user_id_from_plist(&global_plist) {
            return Ok(user_id);
        }
    }

    anyhow::bail!(
        "Could not extract KakaoTalk user ID from preferences. \
         Is KakaoTalk installed and logged in?"
    )
}

fn extract_user_id_from_plist(path: &std::path::Path) -> Result<i64> {
    let dict: plist::Dictionary = plist::from_file(path).context("Failed to parse plist")?;

    // Strategy A: FSChatWindowTransparency keys → longest common suffix
    let prefix = "FSChatWindowTransparency";
    let suffixes: Vec<String> = dict
        .keys()
        .filter(|k| k.starts_with(prefix) && k.len() > prefix.len())
        .map(|k| k[prefix.len()..].to_string())
        .collect();

    if suffixes.len() >= 2 {
        // Each suffix is chatId + userId. Find the common tail.
        if let Some(common) = longest_common_suffix(&suffixes) {
            if let Ok(id) = common.parse::<i64>() {
                return Ok(id);
            }
        }
    }

    // Strategy B: Direct key lookup
    for key in &["userId", "user_id", "KAKAO_USER_ID", "userID"] {
        if let Some(val) = dict.get(key) {
            if let Some(n) = val.as_signed_integer() {
                return Ok(n);
            }
            if let Some(s) = val.as_string() {
                if let Ok(n) = s.parse::<i64>() {
                    return Ok(n);
                }
            }
        }
    }

    anyhow::bail!("No userId found in plist")
}

fn longest_common_suffix(strings: &[String]) -> Option<String> {
    if strings.is_empty() {
        return None;
    }
    let reversed: Vec<Vec<char>> = strings.iter().map(|s| s.chars().rev().collect()).collect();
    let min_len = reversed.iter().map(|r| r.len()).min().unwrap_or(0);
    let mut common_len = 0;
    for i in 0..min_len {
        let ch = reversed[0][i];
        if reversed.iter().all(|r| r[i] == ch) {
            common_len = i + 1;
        } else {
            break;
        }
    }
    if common_len == 0 {
        return None;
    }
    Some(reversed[0][..common_len].iter().rev().collect())
}

// ---------------------------------------------------------------------------
// Key derivation (matches kakaocli KeyDerivation.swift)
// ---------------------------------------------------------------------------

fn hashed_device_uuid(uuid: &str) -> String {
    let sha1_hash = sha1::Sha1::digest(uuid.as_bytes());
    let sha256_hash = sha2::Sha256::digest(uuid.as_bytes());
    let mut combined = Vec::with_capacity(52);
    combined.extend_from_slice(&sha1_hash);
    combined.extend_from_slice(&sha256_hash);
    base64::engine::general_purpose::STANDARD.encode(&combined)
}

/// Derive the database file name from userId and UUID.
fn derive_database_name(user_id: i64, uuid: &str) -> String {
    let reversed_uuid: String = uuid.chars().rev().collect();
    let hawawa = format!("..F.{}.A.F.{}..|", user_id, reversed_uuid);

    // Salt: reversed base64(SHA1 || SHA256) of UUID
    let hashed = hashed_device_uuid(uuid);
    let salt: String = hashed.chars().rev().collect();

    let derived = pbkdf2_sha256(hawawa.as_bytes(), salt.as_bytes(), 100_000, 128);
    let hex_str = hex::encode(&derived);

    // Extract substring [28..106] (78 chars)
    hex_str[28..106].to_string()
}

/// Derive the SQLCipher encryption key.
fn derive_secure_key(user_id: i64, uuid: &str) -> String {
    let hashed = hashed_device_uuid(uuid);

    let uuid_prefix5: String = uuid.chars().take(5).collect();
    let uuid_drop7: String = uuid.chars().skip(7).collect();

    let parts = [
        "A",
        &hashed,
        "|",
        "F",
        &uuid_prefix5,
        "H",
        &user_id.to_string(),
        "|",
        &uuid_drop7,
    ];
    let hawawa: String = parts.join("F");
    let reversed_hawawa: String = hawawa.chars().rev().collect();

    // Salt: UUID from 30% offset to end
    let offset = (uuid.len() as f64 * 0.3) as usize;
    let salt = &uuid[offset..];

    let derived = pbkdf2_sha256(reversed_hawawa.as_bytes(), salt.as_bytes(), 100_000, 128);
    hex::encode(&derived)
}

/// PBKDF2-HMAC-SHA256
fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, key_len: usize) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<sha2::Sha256>;

    let hash_len = 32; // SHA-256 output
    let blocks_needed = key_len.div_ceil(hash_len);
    let mut output = Vec::with_capacity(blocks_needed * hash_len);

    for block_num in 1..=blocks_needed as u32 {
        // U1 = PRF(password, salt || INT_32_BE(block_num))
        let mut mac = HmacSha256::new_from_slice(password).expect("HMAC accepts any key length");
        mac.update(salt);
        mac.update(&block_num.to_be_bytes());
        let mut u = mac.finalize().into_bytes().to_vec();
        let mut result = u.clone();

        for _ in 1..iterations {
            let mut mac =
                HmacSha256::new_from_slice(password).expect("HMAC accepts any key length");
            mac.update(&u);
            u = mac.finalize().into_bytes().to_vec();
            for (r, ui) in result.iter_mut().zip(u.iter()) {
                *r ^= ui;
            }
        }
        output.extend_from_slice(&result);
    }

    output.truncate(key_len);
    output
}

// ---------------------------------------------------------------------------
// Database path resolution
// ---------------------------------------------------------------------------

fn find_database_path(db_name: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("No home directory")?;
    let container_dir = home.join(
        "Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Application Support/com.kakao.KakaoTalkMac",
    );

    if !container_dir.exists() {
        anyhow::bail!(
            "KakaoTalk container directory not found: {}",
            container_dir.display()
        );
    }

    // Look for a file matching the derived database name
    if let Ok(entries) = std::fs::read_dir(&container_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains(db_name) || name == db_name {
                return Ok(entry.path());
            }
        }
    }

    // Fallback: look for any .db or database-like file
    if let Ok(entries) = std::fs::read_dir(&container_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // KakaoTalk database files are hex-named without extension
            if name.chars().all(|c| c.is_ascii_hexdigit()) && name.len() > 20 {
                return Ok(entry.path());
            }
        }
    }

    anyhow::bail!(
        "KakaoTalk database not found in {}. Derived name: {}",
        container_dir.display(),
        db_name
    )
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub struct LocalDbReader {
    conn: Connection,
}

impl LocalDbReader {
    pub fn open() -> Result<Self> {
        let uuid = get_platform_uuid().context("Failed to get IOPlatformUUID")?;
        let user_id = get_user_id_from_plist().context("Failed to get KakaoTalk user ID")?;

        let db_name = derive_database_name(user_id, &uuid);
        let db_path =
            find_database_path(&db_name).context("Failed to locate KakaoTalk local database")?;

        let secure_key = derive_secure_key(user_id, &uuid);

        let conn = Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

        // Configure SQLCipher
        conn.pragma_update(None, "cipher_compatibility", 3)?;
        conn.pragma_update(None, "key", &secure_key)?;

        // Verify the key works
        conn.execute_batch("SELECT count(*) FROM sqlite_master")
            .context(
                "Failed to decrypt KakaoTalk database. Key derivation may have failed. \
                 Ensure KakaoTalk is installed and logged in.",
            )?;

        Ok(Self { conn })
    }

    /// Check if the local database is accessible (for doctor command).
    pub fn check_access() -> Result<LocalDbStatus> {
        let home = dirs::home_dir().context("No home directory")?;
        let container_dir = home.join(
            "Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Application Support/com.kakao.KakaoTalkMac",
        );

        let uuid_ok = get_platform_uuid().is_ok();
        let user_id_ok = get_user_id_from_plist().is_ok();
        let container_exists = container_dir.exists();

        let db_file = if uuid_ok && user_id_ok {
            let uuid = get_platform_uuid().ok();
            let uid = get_user_id_from_plist().ok();
            if let (Some(u), Some(id)) = (uuid, uid) {
                let name = derive_database_name(id, &u);
                find_database_path(&name).ok()
            } else {
                None
            }
        } else {
            None
        };

        let decryptable = if db_file.is_some() {
            Self::open().is_ok()
        } else {
            false
        };

        Ok(LocalDbStatus {
            uuid_available: uuid_ok,
            user_id_available: user_id_ok,
            container_exists,
            db_file_found: db_file.is_some(),
            db_path: db_file.map(|p| p.to_string_lossy().to_string()),
            decryptable,
        })
    }

    pub fn list_chats(&self, limit: usize) -> Result<Vec<LocalChat>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.chatId, r.type, r.chatName, r.activeMembersCount,
                    r.lastLogId, r.lastUpdatedAt, r.countOfNewMessage,
                    COALESCE(u.displayName, u.friendNickName, u.nickName, '') as displayName
             FROM NTChatRoom r
             LEFT JOIN NTUser u ON r.directChatMemberUserId = u.userId AND u.linkId = 0
             ORDER BY r.lastUpdatedAt DESC
             LIMIT ?",
        )?;

        let rows = stmt
            .query_map([limit as i64], |row| {
                let chat_name: String = row.get::<_, String>(2).unwrap_or_default();
                let display_name: String = row.get::<_, String>(7).unwrap_or_default();
                let title = if chat_name.is_empty() {
                    display_name.clone()
                } else {
                    chat_name
                };
                Ok(LocalChat {
                    chat_id: row.get(0)?,
                    chat_type: row.get(1)?,
                    chat_name: title,
                    active_members_count: row.get(3).unwrap_or(0),
                    last_log_id: row.get(4).unwrap_or(0),
                    last_updated_at: row.get(5).unwrap_or(0),
                    unread_count: row.get(6).unwrap_or(0),
                    display_name,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn read_messages(
        &self,
        chat_id: i64,
        limit: usize,
        since_ts: Option<i64>,
    ) -> Result<Vec<LocalMessage>> {
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(ts) = since_ts {
                (
                    "SELECT m.logId, m.chatId, m.authorId,
                        COALESCE(u.displayName, u.friendNickName, u.nickName, '') as senderName,
                        COALESCE(m.message, '') as message, m.type, m.sentAt
                 FROM NTChatMessage m
                 LEFT JOIN NTUser u ON m.authorId = u.userId AND u.linkId = 0
                 WHERE m.chatId = ? AND m.sentAt >= ?
                 ORDER BY m.sentAt DESC
                 LIMIT ?"
                        .to_string(),
                    vec![Box::new(chat_id), Box::new(ts), Box::new(limit as i64)],
                )
            } else {
                (
                    "SELECT m.logId, m.chatId, m.authorId,
                        COALESCE(u.displayName, u.friendNickName, u.nickName, '') as senderName,
                        COALESCE(m.message, '') as message, m.type, m.sentAt
                 FROM NTChatMessage m
                 LEFT JOIN NTUser u ON m.authorId = u.userId AND u.linkId = 0
                 WHERE m.chatId = ?
                 ORDER BY m.sentAt DESC
                 LIMIT ?"
                        .to_string(),
                    vec![Box::new(chat_id), Box::new(limit as i64)],
                )
            };

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(LocalMessage {
                    log_id: row.get(0)?,
                    chat_id: row.get(1)?,
                    author_id: row.get(2).unwrap_or(0),
                    sender_name: row.get(3).unwrap_or_default(),
                    message: row.get(4).unwrap_or_default(),
                    message_type: row.get(5).unwrap_or(0),
                    sent_at: row.get(6).unwrap_or(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn search_messages(&self, query: &str, limit: usize) -> Result<Vec<LocalMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.logId, m.chatId, m.authorId,
                    COALESCE(u.displayName, u.friendNickName, u.nickName, '') as senderName,
                    COALESCE(m.message, '') as message, m.type, m.sentAt
             FROM NTChatMessage m
             LEFT JOIN NTUser u ON m.authorId = u.userId AND u.linkId = 0
             WHERE m.message LIKE ?
             ORDER BY m.sentAt DESC
             LIMIT ?",
        )?;

        let pattern = format!("%{}%", query);
        let rows = stmt
            .query_map(rusqlite::params![pattern, limit as i64], |row| {
                Ok(LocalMessage {
                    log_id: row.get(0)?,
                    chat_id: row.get(1)?,
                    author_id: row.get(2).unwrap_or(0),
                    sender_name: row.get(3).unwrap_or_default(),
                    message: row.get(4).unwrap_or_default(),
                    message_type: row.get(5).unwrap_or(0),
                    sent_at: row.get(6).unwrap_or(0),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn schema(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1).unwrap_or_default(),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Find the memo chat (나와의 채팅) ID. Type 0 with activeMembersCount = 1.
    pub fn find_memo_chat_id(&self) -> Result<Option<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT chatId FROM NTChatRoom WHERE type = 0 AND activeMembersCount = 1 LIMIT 1",
        )?;
        let result = stmt.query_row([], |row| row.get::<_, i64>(0)).ok();
        Ok(result)
    }
}

#[derive(Debug, Serialize)]
pub struct LocalDbStatus {
    pub uuid_available: bool,
    pub user_id_available: bool,
    pub container_exists: bool,
    pub db_file_found: bool,
    pub db_path: Option<String>,
    pub decryptable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_sha256_produces_expected_length() {
        let result = pbkdf2_sha256(b"password", b"salt", 1, 32);
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn pbkdf2_sha256_128_bytes() {
        let result = pbkdf2_sha256(b"test", b"salt", 1, 128);
        assert_eq!(result.len(), 128);
    }

    #[test]
    fn hashed_device_uuid_produces_base64() {
        let result = hashed_device_uuid("TEST-UUID");
        assert!(!result.is_empty());
        // SHA1 (20) + SHA256 (32) = 52 bytes → base64 ≈ 72 chars
        assert!(result.len() > 50);
    }

    #[test]
    fn longest_common_suffix_works() {
        let strings = vec!["abc123".to_string(), "def123".to_string()];
        assert_eq!(longest_common_suffix(&strings), Some("123".to_string()));
    }

    #[test]
    fn longest_common_suffix_none_when_empty() {
        let strings: Vec<String> = vec![];
        assert_eq!(longest_common_suffix(&strings), None);
    }

    #[test]
    fn longest_common_suffix_no_match() {
        let strings = vec!["abc".to_string(), "def".to_string()];
        assert_eq!(longest_common_suffix(&strings), None);
    }
}
