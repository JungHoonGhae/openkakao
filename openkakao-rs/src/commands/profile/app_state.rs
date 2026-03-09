use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};

use super::{
    KakaoAppStateDiffEntry, KakaoAppStateFile, KakaoAppStateSnapshot, ProfileHintsBaseline,
};

pub fn kakao_container_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("Library/Containers/com.kakao.KakaoTalkMac/Data")
}

pub fn kakao_cache_db_path() -> PathBuf {
    kakao_container_dir().join("Library/Caches/Cache.db")
}

pub fn kakao_preferences_dir() -> PathBuf {
    kakao_container_dir().join("Library/Preferences")
}

pub fn metadata_modified_unix(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
}

pub fn collect_kakao_app_state_files(
    dir: &std::path::Path,
    relative_to: &std::path::Path,
    files: &mut Vec<KakaoAppStateFile>,
    depth: usize,
) -> Result<()> {
    if depth == 0 || !dir.exists() {
        return Ok(());
    }

    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let relative = path
            .strip_prefix(relative_to)
            .unwrap_or(&path)
            .display()
            .to_string();

        if metadata.is_dir() {
            files.push(KakaoAppStateFile {
                path: relative.clone(),
                kind: "dir".into(),
                size: 0,
                modified_unix: metadata_modified_unix(&metadata),
            });
            collect_kakao_app_state_files(&path, relative_to, files, depth.saturating_sub(1))?;
        } else if metadata.is_file() {
            files.push(KakaoAppStateFile {
                path: relative,
                kind: "file".into(),
                size: metadata.len(),
                modified_unix: metadata_modified_unix(&metadata),
            });
        }
    }

    Ok(())
}

pub fn load_kakao_app_state_snapshot() -> Result<KakaoAppStateSnapshot> {
    let root = kakao_container_dir().join("Library/Application Support/com.kakao.KakaoTalkMac");
    let preferences_dir = kakao_preferences_dir();
    let cache_db = kakao_cache_db_path();
    let mut files = Vec::new();

    collect_kakao_app_state_files(&root, &root, &mut files, 2)?;
    collect_kakao_app_state_files(&preferences_dir, &preferences_dir, &mut files, 1)?;
    if cache_db.exists() {
        let metadata = std::fs::metadata(&cache_db)
            .with_context(|| format!("failed to stat {}", cache_db.display()))?;
        files.push(KakaoAppStateFile {
            path: cache_db.display().to_string(),
            kind: "file".into(),
            size: metadata.len(),
            modified_unix: metadata_modified_unix(&metadata),
        });
    }

    files.sort_by(|a, b| {
        b.modified_unix
            .cmp(&a.modified_unix)
            .then_with(|| a.path.cmp(&b.path))
    });

    Ok(KakaoAppStateSnapshot {
        root: root.display().to_string(),
        preferences_dir: preferences_dir.display().to_string(),
        cache_db: cache_db.display().to_string(),
        files,
    })
}

pub fn load_profile_hints_baseline(path: &str) -> Result<ProfileHintsBaseline> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path))
}

pub fn diff_kakao_app_state(
    before: &KakaoAppStateSnapshot,
    after: &KakaoAppStateSnapshot,
) -> Vec<KakaoAppStateDiffEntry> {
    let before_map = before
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<HashMap<_, _>>();
    let after_map = after
        .files
        .iter()
        .map(|file| (file.path.clone(), file))
        .collect::<HashMap<_, _>>();
    let mut paths = before_map
        .keys()
        .chain(after_map.keys())
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();

    let mut diff = Vec::new();
    for path in paths {
        let before = before_map.get(&path).copied();
        let after = after_map.get(&path).copied();
        let change = match (before, after) {
            (None, Some(_)) => Some("added"),
            (Some(_), None) => Some("removed"),
            (Some(before), Some(after))
                if before.size != after.size
                    || before.modified_unix != after.modified_unix
                    || before.kind != after.kind =>
            {
                Some("changed")
            }
            _ => None,
        };
        if let Some(change) = change {
            diff.push(KakaoAppStateDiffEntry {
                path,
                change: change.into(),
                before_size: before.map(|file| file.size),
                after_size: after.map(|file| file.size),
                before_modified_unix: before.and_then(|file| file.modified_unix),
                after_modified_unix: after.and_then(|file| file.modified_unix),
            });
        }
    }

    diff.sort_by(|a, b| a.path.cmp(&b.path));
    diff
}
