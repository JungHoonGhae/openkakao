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
    fs::copy(&cache_db, &tmp_db)
        .with_context(|| format!("Failed to copy {}", cache_db.display()))?;

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
        let app_version = a_header
            .split('/')
            .nth(1)
            .unwrap_or("3.7.0")
            .to_string();

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
        b.priority
            .cmp(&a.priority)
            .then_with(|| b.timestamp.partial_cmp(&a.timestamp).unwrap_or(Ordering::Equal))
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

fn copy_companion_file(cache_db: &Path, tmp_db: &Path, suffix: &str) -> Result<()> {
    let src = PathBuf::from(format!("{}{}", cache_db.display(), suffix));
    if src.exists() {
        let dst = PathBuf::from(format!("{}{}", tmp_db.display(), suffix));
        fs::copy(&src, &dst)
            .with_context(|| format!("Failed to copy {}", src.display()))?;
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

fn value_as_string(value: Option<&PlistValue>) -> Option<String> {
    match value {
        Some(PlistValue::String(s)) => Some(s.to_string()),
        Some(PlistValue::Integer(n)) => Some(n.to_string()),
        Some(PlistValue::Real(n)) => Some(n.to_string()),
        _ => None,
    }
}
