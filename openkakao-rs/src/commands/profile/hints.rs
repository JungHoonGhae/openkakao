use anyhow::{Context, Result};

use super::probe::probe_syncmainpf_variants;
use super::{
    build_local_friend_graph_for_chat_ids, build_syncmainpf_candidate,
    build_syncmainpf_probe_variants, build_uplinkprof_probe_variants, collect_hint_chat_ids,
    diff_kakao_app_state, load_kakao_app_state_snapshot, load_profile_hints_baseline,
    local_graph_hint_summary, ProfileCacheHint, ProfileHintsSnapshot, ProfileRevisionHints,
};
use crate::commands::probe::probe_method_variants;
use crate::util::{print_section_title, print_table};

pub fn cmd_profile_hints(
    app_state: bool,
    app_state_diff: Option<String>,
    local_graph: bool,
    user_id: Option<i64>,
    probe_syncmainpf: bool,
    probe_uplinkprof: bool,
    json: bool,
) -> Result<()> {
    if app_state_diff.is_some() && !app_state {
        anyhow::bail!("--app-state-diff requires --app-state");
    }
    if (probe_syncmainpf || probe_uplinkprof) && (!local_graph || user_id.is_none()) {
        anyhow::bail!(
            "--probe-syncmainpf/--probe-uplinkprof require both --local-graph and --user-id"
        );
    }

    let cached_requests = load_profile_cache_hints(12)?;
    let app_state_snapshot = if app_state {
        Some(load_kakao_app_state_snapshot()?)
    } else {
        None
    };
    let app_state_diff_entries = match (&app_state_snapshot, app_state_diff.as_deref()) {
        (Some(current), Some(path)) => {
            let baseline = load_profile_hints_baseline(path)?;
            let Some(previous) = baseline.app_state else {
                anyhow::bail!("baseline snapshot does not contain app_state");
            };
            Some(diff_kakao_app_state(&previous, current))
        }
        _ => None,
    };
    let local_graph_snapshot = if local_graph {
        let targeted_chat_ids = user_id
            .map(|user_id| collect_hint_chat_ids(&cached_requests, user_id))
            .filter(|ids| !ids.is_empty());
        Some(build_local_friend_graph_for_chat_ids(
            targeted_chat_ids.as_deref(),
        )?)
    } else {
        None
    };
    let local_graph_summary = local_graph_snapshot
        .as_ref()
        .map(|graph| local_graph_hint_summary(graph, &cached_requests));
    let syncmainpf_candidates = match (&local_graph_snapshot, user_id) {
        (Some(graph), Some(user_id)) => {
            build_syncmainpf_candidate(graph, &cached_requests, user_id)
                .into_iter()
                .collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };
    let syncmainpf_probe_results = if probe_syncmainpf {
        let variants = syncmainpf_candidates
            .iter()
            .flat_map(build_syncmainpf_probe_variants)
            .collect::<Vec<_>>();
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { probe_syncmainpf_variants(&variants).await })?
    } else {
        Vec::new()
    };
    let uplinkprof_probe_results = if probe_uplinkprof {
        let variants = syncmainpf_candidates
            .iter()
            .flat_map(build_uplinkprof_probe_variants)
            .collect::<Vec<_>>();
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { probe_method_variants("UPLINKPROF", &variants).await })?
    } else {
        Vec::new()
    };
    let snapshot = ProfileHintsSnapshot {
        revisions: load_profile_revision_hints()?,
        cached_requests,
        app_state: app_state_snapshot,
        app_state_diff: app_state_diff_entries,
        local_graph: local_graph_summary,
        syncmainpf_candidates,
        syncmainpf_probe_results,
        uplinkprof_probe_results,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
        return Ok(());
    }

    print_section_title("Profile hints");
    println!(
        "  profile_list_revision: {}",
        snapshot
            .revisions
            .profile_list_revision
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  designated_friends_revision: {}",
        snapshot
            .revisions
            .designated_friends_revision
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  block_friends_sync: {}",
        snapshot
            .revisions
            .block_friends_sync_enabled
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into())
    );
    println!(
        "  block_channels_sync: {}",
        snapshot
            .revisions
            .block_channels_sync_enabled
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".into())
    );
    if let Some(local_graph) = &snapshot.local_graph {
        println!(
            "  local_graph: users={} chats={} failed_chats={}",
            local_graph.user_count,
            local_graph.chat_count,
            local_graph.failed_chat_ids.len()
        );
        let token_preview = local_graph
            .chat_meta
            .iter()
            .filter_map(|chat| {
                chat.getmem_token.map(|token| {
                    format!(
                        "{}:{} ({})",
                        chat.chat_id,
                        token,
                        if chat.title.is_empty() {
                            "-"
                        } else {
                            chat.title.as_str()
                        }
                    )
                })
            })
            .take(5)
            .collect::<Vec<_>>();
        if !token_preview.is_empty() {
            println!("  local_graph_tokens: {}", token_preview.join(", "));
        }
    }
    if let Some(app_state) = &snapshot.app_state {
        println!("  app_state_files: {}", app_state.files.len());
        let recent = app_state
            .files
            .iter()
            .take(5)
            .map(|file| format!("{} [{} bytes]", file.path, file.size))
            .collect::<Vec<_>>();
        if !recent.is_empty() {
            println!("  app_state_recent: {}", recent.join(", "));
        }
    }
    if let Some(diff) = &snapshot.app_state_diff {
        println!("  app_state_diff: {} changed entries", diff.len());
    }
    if let Some(candidate) = snapshot.syncmainpf_candidates.first() {
        println!(
            "  syncmainpf_candidates: {}  uplinkprof_candidates: {}",
            candidate.bodies.len(),
            candidate.uplinkprof_bodies.len()
        );
        if !candidate.getmem_tokens.is_empty() {
            println!(
                "  syncmainpf_getmem_tokens: {}",
                candidate
                    .getmem_tokens
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    println!();

    let rows = snapshot
        .cached_requests
        .iter()
        .map(|hint| {
            let ids = if hint.user_ids.is_empty() {
                "-".to_string()
            } else if hint.user_ids.len() == 1 {
                hint.user_ids[0].to_string()
            } else {
                format!(
                    "{} (+{})",
                    hint.user_ids[0],
                    hint.user_ids.len().saturating_sub(1)
                )
            };
            let access = hint
                .access_permit
                .as_deref()
                .map(|value| value.chars().take(8).collect::<String>())
                .unwrap_or_else(|| "-".into());
            let local_match = snapshot
                .local_graph
                .as_ref()
                .and_then(|summary| {
                    summary
                        .candidate_matches
                        .iter()
                        .find(|candidate| candidate.entry_id == hint.entry_id)
                })
                .map(|matched| {
                    if matched.matched_user_ids.is_empty() {
                        "-".to_string()
                    } else {
                        format!(
                            "{} chat(s), {} permit(s), {} token(s)",
                            matched.candidate_chat_ids.len(),
                            matched.candidate_access_permits.len(),
                            matched.candidate_getmem_tokens.len()
                        )
                    }
                })
                .unwrap_or_else(|| "-".into());
            vec![
                hint.entry_id.to_string(),
                hint.kind.clone(),
                ids,
                hint.chat_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".into()),
                access,
                hint.category.clone().unwrap_or_else(|| "-".into()),
                if hint.data_on_fs {
                    "fs".into()
                } else {
                    "inline".into()
                },
                local_match,
            ]
        })
        .collect::<Vec<_>>();

    print_table(
        &[
            "Entry",
            "Kind",
            "User IDs",
            "Chat ID",
            "Permit",
            "Category",
            "Body",
            "Local graph",
        ],
        rows,
    );

    if let Some(candidate) = snapshot.syncmainpf_candidates.first() {
        println!();
        print_section_title(&format!(
            "SYNCMAINPF candidate bodies for {}",
            candidate.user_id
        ));
        println!(
            "  account_id: {}  self: {}  source_entry_ids: {}",
            candidate.account_id,
            candidate.is_self,
            if candidate.source_entry_ids.is_empty() {
                "-".to_string()
            } else {
                candidate
                    .source_entry_ids
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        if !candidate.getmem_tokens.is_empty() {
            println!(
                "  getmem_tokens: {}",
                candidate
                    .getmem_tokens
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        for body in &candidate.bodies {
            println!("  {}", serde_json::to_string(body)?);
        }

        println!();
        print_section_title(&format!(
            "UPLINKPROF candidate bodies for {}",
            candidate.user_id
        ));
        for body in &candidate.uplinkprof_bodies {
            println!("  {}", serde_json::to_string(body)?);
        }
    }

    if !snapshot.syncmainpf_probe_results.is_empty() {
        println!();
        print_section_title("SYNCMAINPF probe results");
        for result in &snapshot.syncmainpf_probe_results {
            let body_status = result
                .body_status
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into());
            let pushes = if result.push_methods.is_empty() {
                "-".to_string()
            } else {
                result.push_methods.join(",")
            };
            println!(
                "  packet_status={} body_status={} pushes={} methods={} body={}",
                result.packet_status_code,
                body_status,
                result.push_count,
                pushes,
                serde_json::to_string(&result.body)?
            );
        }
    }

    if !snapshot.uplinkprof_probe_results.is_empty() {
        println!();
        print_section_title("UPLINKPROF probe results");
        for result in &snapshot.uplinkprof_probe_results {
            let body_status = result
                .body_status
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".into());
            let pushes = if result.push_methods.is_empty() {
                "-".to_string()
            } else {
                result.push_methods.join(",")
            };
            println!(
                "  packet_status={} body_status={} pushes={} methods={} body={}",
                result.packet_status_code,
                body_status,
                result.push_count,
                pushes,
                serde_json::to_string(&result.body)?
            );
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Hint loading helpers
// ---------------------------------------------------------------------------

pub fn load_profile_cache_hints(limit: usize) -> Result<Vec<ProfileCacheHint>> {
    let cache_db = super::kakao_cache_db_path();
    let conn = rusqlite::Connection::open_with_flags(
        &cache_db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("failed to open {}", cache_db.display()))?;

    let sql = r#"
        SELECT
            r.entry_ID,
            r.request_key,
            COALESCE(d.isDataOnFS, 0)
        FROM cfurl_cache_response r
        LEFT JOIN cfurl_cache_receiver_data d ON d.entry_ID = r.entry_ID
        WHERE r.request_key LIKE '%/mac/profile3/friend.json%'
           OR r.request_key LIKE '%/mac/profile3/friends.json%'
           OR r.request_key LIKE '%/mac/profile/designated_friends.json%'
        ORDER BY r.entry_ID DESC
        LIMIT ?1
    "#;
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([limit as i64], |row| {
        let entry_id: i64 = row.get(0)?;
        let request_key: String = row.get(1)?;
        let data_on_fs: i64 = row.get(2)?;
        Ok(parse_profile_cache_hint(
            entry_id,
            &request_key,
            data_on_fs != 0,
        ))
    })?;

    let mut hints = Vec::new();
    for row in rows {
        hints.push(row?);
    }
    Ok(hints)
}

pub fn parse_profile_cache_hint(
    entry_id: i64,
    request_key: &str,
    data_on_fs: bool,
) -> ProfileCacheHint {
    let mut kind = "other".to_string();
    let mut user_ids = Vec::new();
    let mut chat_id = None;
    let mut access_permit = None;
    let mut category = None;

    if let Ok(url) = reqwest::Url::parse(request_key) {
        let path = url.path();
        kind = match path {
            "/mac/profile3/friend.json" => "friend".to_string(),
            "/mac/profile3/friends.json" => "friends".to_string(),
            "/mac/profile/designated_friends.json" => "designated-friends".to_string(),
            _ => path.rsplit('/').next().unwrap_or("other").to_string(),
        };

        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "id" => {
                    if let Ok(user_id) = value.parse::<i64>() {
                        user_ids.push(user_id);
                    }
                }
                "ids" => {
                    user_ids.extend(parse_i64_list(&value));
                }
                "chatId" => {
                    if let Ok(parsed) = value.parse::<i64>() {
                        chat_id = Some(parsed);
                    }
                }
                "accessPermit" => {
                    access_permit = Some(value.to_string());
                }
                "category" => {
                    category = Some(value.to_string());
                }
                _ => {}
            }
        }
    }

    ProfileCacheHint {
        entry_id,
        kind,
        request_key: request_key.to_string(),
        user_ids,
        chat_id,
        access_permit,
        category,
        data_on_fs,
    }
}

pub fn parse_i64_list(raw: &str) -> Vec<i64> {
    raw.trim_matches(&['[', ']'][..])
        .split(',')
        .filter_map(|part| part.trim().parse::<i64>().ok())
        .collect()
}

pub fn plist_i64(value: &plist::Value) -> Option<i64> {
    match value {
        plist::Value::Integer(num) => num.as_signed(),
        plist::Value::Real(num) => Some(*num as i64),
        _ => None,
    }
}

pub fn plist_bool(value: &plist::Value) -> Option<bool> {
    match value {
        plist::Value::Boolean(value) => Some(*value),
        _ => None,
    }
}

pub fn load_profile_revision_hints() -> Result<ProfileRevisionHints> {
    let prefs_dir = super::kakao_preferences_dir();
    let mut hints = ProfileRevisionHints::default();

    for entry in std::fs::read_dir(&prefs_dir)
        .with_context(|| format!("failed to read {}", prefs_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("plist") {
            continue;
        }

        let plist = match plist::Value::from_file(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(dict) = plist.as_dictionary() else {
            continue;
        };

        for (key, value) in dict {
            if key.starts_with("PROFILELISTREVISION:") {
                if let Some(revision) = plist_i64(value).filter(|value| *value > 0) {
                    hints.profile_list_revision = Some(
                        hints
                            .profile_list_revision
                            .map_or(revision, |cur| cur.max(revision)),
                    );
                }
            } else if key.starts_with("DESIGNATEDFRIENDSREVISION:") {
                if let Some(revision) = plist_i64(value).filter(|value| *value > 0) {
                    hints.designated_friends_revision = Some(
                        hints
                            .designated_friends_revision
                            .map_or(revision, |cur| cur.max(revision)),
                    );
                }
            } else if key == "kLocoBlockFriendsSyncKey" {
                hints.block_friends_sync_enabled = plist_bool(value);
            } else if key == "kLocoBlockChannelsSyncKey" {
                hints.block_channels_sync_enabled = plist_bool(value);
            }
        }
    }

    Ok(hints)
}
