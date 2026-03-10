use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use plist::Value as PlistValue;
use rusqlite::Connection;
use tempfile::tempdir;

use crate::model::KakaoCredentials;

struct ExtractedCredential {
    creds: KakaoCredentials,
    timestamp: f64,
    source_url: String,
    priority: u8,
}

pub fn get_credential_candidates(max_candidates: usize) -> Result<Vec<KakaoCredentials>> {
    let extracted = extract_candidates_from_cache_db(300)?;

    if extracted.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let debug = std::env::var("OPENKAKAO_RS_DEBUG").is_ok();
    for candidate in extracted.into_iter().take(max_candidates.max(1)) {
        if debug {
            eprintln!(
                "[auth] candidate: ts={:.3}, priority={}, url={}",
                candidate.timestamp, candidate.priority, candidate.source_url
            );
        }
        out.push(candidate.creds);
    }

    Ok(out)
}

pub fn get_credentials_interactive() -> Result<KakaoCredentials> {
    eprintln!("Could not auto-extract KakaoTalk credentials.");
    eprintln!("Please provide credentials manually.");

    let oauth_token = prompt("OAuth Token (Authorization header value): ")?;
    let user_id_raw = prompt("User ID (numeric, from talk-user-id header): ")?;

    let user_id = user_id_raw.trim().parse::<i64>().unwrap_or(0);
    let device_uuid = oauth_token
        .split_once('-')
        .map(|(_, suffix)| suffix.to_string())
        .unwrap_or_default();

    Ok(KakaoCredentials::new(
        oauth_token,
        user_id,
        device_uuid,
        "3.7.0".to_string(),
        String::new(),
        String::new(),
    ))
}

fn prompt(label: &str) -> Result<String> {
    print!("{}", label);
    io::stdout().flush().context("Failed to flush stdout")?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read stdin")?;
    Ok(input.trim().to_string())
}

fn extract_candidates_from_cache_db(max_rows: usize) -> Result<Vec<ExtractedCredential>> {
    let home = dirs::home_dir().context("Could not resolve home directory")?;
    let cache_db = home
        .join("Library")
        .join("Containers")
        .join("com.kakao.KakaoTalkMac")
        .join("Data")
        .join("Library")
        .join("Caches")
        .join("Cache.db");

    if !cache_db.exists() {
        return Ok(Vec::new());
    }

    let temp_dir = tempdir().context("Failed to create temporary directory")?;
    let tmp_db = temp_dir.path().join("Cache.db");
    copy_with_timeout(&cache_db, &tmp_db, 5)?;

    copy_companion_file(&cache_db, &tmp_db, "-wal")?;
    copy_companion_file(&cache_db, &tmp_db, "-shm")?;

    let conn = Connection::open(&tmp_db)
        .with_context(|| format!("Failed to open {}", tmp_db.display()))?;

    let mut stmt = conn.prepare(
        "
        SELECT b.request_object, r.request_key, r.time_stamp
        FROM cfurl_cache_blob_data b
        JOIN cfurl_cache_response r ON b.entry_ID = r.entry_ID
        WHERE b.request_object IS NOT NULL
          AND (r.request_key LIKE '%kakao.com%' OR r.request_key LIKE '%kakao%')
        ORDER BY r.time_stamp DESC
        LIMIT ?1
        ",
    )?;

    let mut rows = stmt.query([max_rows as i64])?;

    let mut candidates = Vec::new();
    let mut seen_tokens = HashSet::new();

    while let Some(row) = rows.next()? {
        let request_object: Vec<u8> = row.get(0)?;
        let request_key: String = row.get::<_, String>(1).unwrap_or_default();
        let timestamp = row
            .get::<_, f64>(2)
            .or_else(|_| row.get::<_, i64>(2).map(|v| v as f64))
            .unwrap_or(0.0);

        let plist = match PlistValue::from_reader(Cursor::new(request_object)) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let headers = match find_headers_map(&plist) {
            Some(h) => h,
            None => continue,
        };

        let auth_token = match value_as_string(headers.get("Authorization")) {
            Some(token) if !token.is_empty() => token,
            _ => continue,
        };

        if !seen_tokens.insert(auth_token.clone()) {
            continue;
        }

        let user_id = value_as_string(headers.get("talk-user-id"))
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        let user_agent = value_as_string(headers.get("User-Agent")).unwrap_or_default();
        let a_header = value_as_string(headers.get("A")).unwrap_or_default();
        let app_version = a_header.split('/').nth(1).unwrap_or("3.7.0").to_string();

        let device_uuid = auth_token
            .split_once('-')
            .map(|(_, suffix)| suffix.to_string())
            .unwrap_or_default();

        let priority = url_priority(&request_key);

        candidates.push(ExtractedCredential {
            creds: KakaoCredentials::new(
                auth_token,
                user_id,
                device_uuid,
                app_version,
                user_agent,
                a_header,
            ),
            timestamp,
            source_url: request_key,
            priority,
        });
    }

    candidates.sort_by(|a, b| {
        b.priority.cmp(&a.priority).then_with(|| {
            b.timestamp
                .partial_cmp(&a.timestamp)
                .unwrap_or(Ordering::Equal)
        })
    });

    Ok(candidates)
}

fn url_priority(url: &str) -> u8 {
    if url.contains("/mac/account/more_settings.json") {
        3
    } else if url.contains("/messaging/chats") || url.contains("/mac/profile3/me.json") {
        2
    } else {
        1
    }
}

/// Extract refresh_token from Cache.db by looking at renew_token.json POST body.
/// The POST body is stored as <data> inside the request_object plist.
pub fn extract_refresh_token() -> Result<Option<String>> {
    let home = dirs::home_dir().context("Could not resolve home directory")?;
    let cache_db = home
        .join("Library")
        .join("Containers")
        .join("com.kakao.KakaoTalkMac")
        .join("Data")
        .join("Library")
        .join("Caches")
        .join("Cache.db");

    if !cache_db.exists() {
        return Ok(None);
    }

    let temp_dir = tempdir().context("Failed to create temporary directory")?;
    let tmp_db = temp_dir.path().join("Cache.db");
    copy_with_timeout(&cache_db, &tmp_db, 5)?;
    copy_companion_file(&cache_db, &tmp_db, "-wal")?;
    copy_companion_file(&cache_db, &tmp_db, "-shm")?;

    let conn = Connection::open(&tmp_db)
        .with_context(|| format!("Failed to open {}", tmp_db.display()))?;

    let mut stmt = conn.prepare(
        "
        SELECT b.request_object
        FROM cfurl_cache_blob_data b
        JOIN cfurl_cache_response r ON b.entry_ID = r.entry_ID
        WHERE r.request_key LIKE '%renew_token%'
          AND b.request_object IS NOT NULL
        ORDER BY r.time_stamp DESC
        LIMIT 1
        ",
    )?;

    let mut rows = stmt.query([])?;

    if let Some(row) = rows.next()? {
        let request_object: Vec<u8> = row.get(0)?;
        let plist = match PlistValue::from_reader(Cursor::new(request_object)) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        if let Some(token) = extract_refresh_token_from_plist(&plist) {
            return Ok(Some(token));
        }
    }

    Ok(None)
}

/// Parse the request_object plist to find POST body data containing refresh_token.
/// The structure is: Root → "Array" → [..., array of <data> elements (POST body chunks)]
fn extract_refresh_token_from_plist(plist: &PlistValue) -> Option<String> {
    let root = plist.as_dictionary()?;
    let arr = root.get("Array")?.as_array()?;

    // Look for inner arrays containing Data elements (POST body)
    for item in arr {
        if let Some(inner_arr) = item.as_array() {
            let mut body_bytes = Vec::new();
            for chunk in inner_arr {
                if let Some(data) = chunk.as_data() {
                    body_bytes.extend_from_slice(data);
                }
            }
            if !body_bytes.is_empty() {
                let body_str = String::from_utf8_lossy(&body_bytes);
                // Parse URL-encoded body for refresh_token parameter
                for param in body_str.split('&') {
                    if let Some(value) = param.strip_prefix("refresh_token=") {
                        return Some(value.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Login parameters cached from login.json POST body.
#[derive(Debug)]
pub struct CachedLoginParams {
    pub email: String,
    pub password: String,
    pub device_uuid: String,
    pub device_name: String,
    pub x_vc: String,
}

/// Extract login.json POST body + X-VC header from Cache.db.
pub fn extract_login_params() -> Result<Option<CachedLoginParams>> {
    let home = dirs::home_dir().context("Could not resolve home directory")?;
    let cache_db = home
        .join("Library")
        .join("Containers")
        .join("com.kakao.KakaoTalkMac")
        .join("Data")
        .join("Library")
        .join("Caches")
        .join("Cache.db");

    if !cache_db.exists() {
        return Ok(None);
    }

    let temp_dir = tempdir().context("Failed to create temporary directory")?;
    let tmp_db = temp_dir.path().join("Cache.db");
    copy_with_timeout(&cache_db, &tmp_db, 5)?;
    copy_companion_file(&cache_db, &tmp_db, "-wal")?;
    copy_companion_file(&cache_db, &tmp_db, "-shm")?;

    let conn = Connection::open(&tmp_db)
        .with_context(|| format!("Failed to open {}", tmp_db.display()))?;

    let mut stmt = conn.prepare(
        "
        SELECT b.request_object
        FROM cfurl_cache_blob_data b
        JOIN cfurl_cache_response r ON b.entry_ID = r.entry_ID
        WHERE r.request_key LIKE '%login.json%'
          AND b.request_object IS NOT NULL
        ORDER BY r.time_stamp DESC
        LIMIT 1
        ",
    )?;

    let mut rows = stmt.query([])?;

    if let Some(row) = rows.next()? {
        let request_object: Vec<u8> = row.get(0)?;
        let plist = match PlistValue::from_reader(Cursor::new(request_object)) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        return Ok(extract_login_params_from_plist(&plist));
    }

    Ok(None)
}

fn extract_login_params_from_plist(plist: &PlistValue) -> Option<CachedLoginParams> {
    let root = plist.as_dictionary()?;
    let arr = root.get("Array")?.as_array()?;

    // Extract X-VC from headers dict (login.json has X-VC instead of Authorization)
    let headers = find_any_headers_map(plist)?;
    let x_vc = value_as_string(headers.get("X-VC")).unwrap_or_default();

    // Extract POST body from inner array with Data elements
    for item in arr {
        if let Some(inner_arr) = item.as_array() {
            let mut body_bytes = Vec::new();
            for chunk in inner_arr {
                if let Some(data) = chunk.as_data() {
                    body_bytes.extend_from_slice(data);
                }
            }
            if !body_bytes.is_empty() {
                let body_str = String::from_utf8_lossy(&body_bytes);
                let mut email = String::new();
                let mut password = String::new();
                let mut device_uuid = String::new();
                let mut device_name = String::new();

                for param in body_str.split('&') {
                    if let Some((key, val)) = param.split_once('=') {
                        let decoded = urlencoding::decode(val).unwrap_or_default().to_string();
                        match key {
                            "email" => email = decoded,
                            "password" => password = decoded,
                            "device_uuid" => device_uuid = decoded,
                            "device_name" => device_name = decoded,
                            _ => {}
                        }
                    }
                }

                if !email.is_empty() {
                    return Some(CachedLoginParams {
                        email,
                        password,
                        device_uuid,
                        device_name,
                        x_vc,
                    });
                }
            }
        }
    }

    None
}

fn copy_with_timeout(src: &Path, dst: &Path, timeout_secs: u64) -> Result<()> {
    let src_owned = src.to_path_buf();
    let dst_owned = dst.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = std::fs::copy(&src_owned, &dst_owned);
        let _ = tx.send(result);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(timeout_secs)) {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => {
            Err(anyhow::anyhow!("{}", e).context(format!("Failed to copy {}", src.display())))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(anyhow::anyhow!(
            "Cache.db copy timed out after {}s (KakaoTalk may be locking the directory). \
             Try quitting KakaoTalk or use \'relogin\' instead.",
            timeout_secs
        )),
        Err(e) => Err(anyhow::anyhow!("Cache.db copy failed: {}", e)),
    }
}

fn copy_companion_file(cache_db: &Path, tmp_db: &Path, suffix: &str) -> Result<()> {
    let src = PathBuf::from(format!("{}{}", cache_db.display(), suffix));
    if src.exists() {
        let dst = PathBuf::from(format!("{}{}", tmp_db.display(), suffix));
        fs::copy(&src, &dst).with_context(|| format!("Failed to copy {}", src.display()))?;
    }
    Ok(())
}

fn find_headers_map(plist: &PlistValue) -> Option<&plist::Dictionary> {
    let root = plist.as_dictionary()?;
    let arr = root.get("Array")?.as_array()?;

    for item in arr {
        if let Some(dict) = item.as_dictionary() {
            if dict.contains_key("Authorization") {
                return Some(dict);
            }
        }
    }

    None
}

/// Find any dict in the Array that has Content-Type (works for both auth and non-auth requests)
fn find_any_headers_map(plist: &PlistValue) -> Option<&plist::Dictionary> {
    let root = plist.as_dictionary()?;
    let arr = root.get("Array")?.as_array()?;

    for item in arr {
        if let Some(dict) = item.as_dictionary() {
            if dict.contains_key("Content-Type") {
                return Some(dict);
            }
        }
    }

    None
}

fn value_as_string(value: Option<&PlistValue>) -> Option<String> {
    match value {
        Some(PlistValue::String(s)) => Some(s.to_string()),
        Some(PlistValue::Integer(n)) => Some(n.to_string()),
        Some(PlistValue::Real(n)) => Some(n.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_priority_more_settings() {
        assert_eq!(
            url_priority("https://katalk.kakao.com/mac/account/more_settings.json"),
            3
        );
    }

    #[test]
    fn test_url_priority_chats() {
        assert_eq!(
            url_priority("https://talk-pilsner.kakao.com/messaging/chats"),
            2
        );
    }

    #[test]
    fn test_url_priority_profile() {
        assert_eq!(
            url_priority("https://katalk.kakao.com/mac/profile3/me.json"),
            2
        );
    }

    #[test]
    fn test_url_priority_other() {
        assert_eq!(
            url_priority("https://katalk.kakao.com/mac/friends/update.json"),
            1
        );
    }

    #[test]
    fn test_value_as_string_string() {
        let v = PlistValue::String("hello".to_string());
        assert_eq!(value_as_string(Some(&v)), Some("hello".to_string()));
    }

    #[test]
    fn test_value_as_string_integer() {
        let v = PlistValue::Integer(42.into());
        assert_eq!(value_as_string(Some(&v)), Some("42".to_string()));
    }

    #[test]
    fn test_value_as_string_none() {
        assert_eq!(value_as_string(None), None);
    }

    #[test]
    fn login_params_can_be_recovered_without_cached_password() {
        let plist = PlistValue::from_reader_xml(
            br#"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Array</key>
    <array>
      <dict>
        <key>Content-Type</key>
        <string>application/x-www-form-urlencoded</string>
        <key>X-VC</key>
        <string>test-xvc</string>
      </dict>
      <array>
        <data>ZGV2aWNlX3V1aWQ9ZGV2LXV1aWQmZGV2aWNlX25hbWU9S2FrYW9UYWxrJmVtYWlsPXRlc3RAZXhhbXBsZS5jb20=</data>
      </array>
    </array>
  </dict>
</plist>
"#.as_slice(),
        )
        .expect("plist should parse");

        let params = extract_login_params_from_plist(&plist).expect("params should exist");
        assert_eq!(params.email, "test@example.com");
        assert_eq!(params.password, "");
        assert_eq!(params.device_uuid, "dev-uuid");
        assert_eq!(params.device_name, "KakaoTalk");
        assert_eq!(params.x_vc, "test-xvc");
    }
}
