use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::loco;
use crate::loco_helpers::loco_connect_with_auto_refresh;
use crate::commands::chats::fetch_loco_chat_listings_with_client;
use crate::commands::members::{
    LocoBlockedSnapshot,
    fetch_loco_blocked_snapshot, fetch_loco_member_profiles, fetch_loco_member_profiles_with_client,
};
use crate::commands::probe::{MethodProbeResult, probe_method_variants};
use crate::commands::rest::filter_friend_search;
use crate::model::json_string;
use crate::util::{
    get_creds, get_rest_client,
    print_section_title, print_table, truncate,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProfileCacheHint {
    pub entry_id: i64,
    pub kind: String,
    pub request_key: String,
    pub user_ids: Vec<i64>,
    pub chat_id: Option<i64>,
    pub access_permit: Option<String>,
    pub category: Option<String>,
    pub data_on_fs: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProfileRevisionHints {
    pub profile_list_revision: Option<i64>,
    pub designated_friends_revision: Option<i64>,
    pub block_friends_sync_enabled: Option<bool>,
    pub block_channels_sync_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileHintsSnapshot {
    pub revisions: ProfileRevisionHints,
    pub cached_requests: Vec<ProfileCacheHint>,
    pub app_state: Option<KakaoAppStateSnapshot>,
    pub app_state_diff: Option<Vec<KakaoAppStateDiffEntry>>,
    pub local_graph: Option<LocalFriendGraphHintSummary>,
    pub syncmainpf_candidates: Vec<SyncMainPfCandidate>,
    pub syncmainpf_probe_results: Vec<SyncMainPfProbeResult>,
    pub uplinkprof_probe_results: Vec<MethodProbeResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileHintsBaseline {
    pub app_state: Option<KakaoAppStateSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalFriendGraphEntry {
    pub user_id: i64,
    pub account_id: i64,
    pub nickname: String,
    pub country_iso: String,
    pub status_message: String,
    pub profile_image_url: String,
    pub full_profile_image_url: String,
    pub original_profile_image_url: String,
    pub access_permits: Vec<String>,
    pub suspicion: String,
    pub suspended: bool,
    pub memorial: bool,
    pub member_type: i32,
    pub chat_ids: Vec<i64>,
    pub chat_titles: Vec<String>,
    pub is_self: bool,
    pub hidden_like: bool,
    pub hidden_block_type: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalFriendGraphChatMeta {
    pub chat_id: i64,
    pub title: String,
    pub getmem_token: Option<i64>,
    pub member_count: usize,
}

#[derive(Debug, Clone)]
pub struct LocalFriendGraphSnapshot {
    pub user_count: usize,
    pub chat_count: usize,
    pub failed_chat_ids: Vec<i64>,
    pub chat_meta: Vec<LocalFriendGraphChatMeta>,
    pub entries: Vec<LocalFriendGraphEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalFriendGraphHintSummary {
    pub user_count: usize,
    pub chat_count: usize,
    pub failed_chat_ids: Vec<i64>,
    pub chat_meta: Vec<LocalFriendGraphChatMeta>,
    pub candidate_matches: Vec<LocalFriendGraphHintMatch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalFriendGraphHintMatch {
    pub entry_id: i64,
    pub kind: String,
    pub requested_user_ids: Vec<i64>,
    pub matched_user_ids: Vec<i64>,
    pub candidate_chat_ids: Vec<i64>,
    pub candidate_access_permits: Vec<String>,
    pub candidate_getmem_tokens: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncMainPfCandidate {
    pub user_id: i64,
    pub account_id: i64,
    pub is_self: bool,
    pub source_entry_ids: Vec<i64>,
    pub getmem_tokens: Vec<i64>,
    pub bodies: Vec<serde_json::Value>,
    pub uplinkprof_bodies: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncMainPfProbeResult {
    pub body: serde_json::Value,
    pub packet_status_code: i16,
    pub body_status: Option<i32>,
    pub push_count: usize,
    pub push_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KakaoAppStateFile {
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified_unix: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KakaoAppStateSnapshot {
    pub root: String,
    pub preferences_dir: String,
    pub cache_db: String,
    pub files: Vec<KakaoAppStateFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KakaoAppStateDiffEntry {
    pub path: String,
    pub change: String,
    pub before_size: Option<u64>,
    pub after_size: Option<u64>,
    pub before_modified_unix: Option<u64>,
    pub after_modified_unix: Option<u64>,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

pub fn cmd_friends_local(
    favorites: bool,
    hidden: bool,
    search: Option<String>,
    chat_id: Option<i64>,
    user_id: Option<i64>,
    json: bool,
) -> Result<()> {
    if favorites {
        anyhow::bail!("friends --local does not support --favorites yet");
    }

    let mut snapshot = build_local_friend_graph()?;
    if hidden {
        let creds = get_creds()?;
        let rt = tokio::runtime::Runtime::new()?;
        let blocked = rt.block_on(async move {
            let mut client = loco::client::LocoClient::new(creds);
            loco_connect_with_auto_refresh(&mut client).await?;
            fetch_loco_blocked_snapshot(&mut client).await
        })?;
        merge_blocked_members_into_local_graph(&mut snapshot, blocked);
    }

    snapshot.entries.retain(|entry| !entry.is_self);
    if let Some(chat_id) = chat_id {
        snapshot
            .entries
            .retain(|entry| entry.chat_ids.contains(&chat_id));
    }
    if let Some(user_id) = user_id {
        snapshot.entries.retain(|entry| entry.user_id == user_id);
    }
    if hidden {
        snapshot.entries.retain(|entry| entry.hidden_like);
    }
    filter_friend_search(&mut snapshot.entries, search, |entry| {
        (entry.nickname.clone(), entry.status_message.clone())
    });

    if json {
        println!("{}", serde_json::to_string_pretty(&snapshot.entries)?);
        return Ok(());
    }

    let rows = snapshot
        .entries
        .iter()
        .map(|entry| {
            vec![
                entry.nickname.clone(),
                truncate(&entry.status_message, 30),
                entry.chat_ids.len().to_string(),
                entry.country_iso.clone(),
                entry
                    .hidden_block_type
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
                entry.user_id.to_string(),
            ]
        })
        .collect::<Vec<_>>();

    let title = if hidden {
        format!("Local hidden-like friends ({})", rows.len())
    } else {
        format!("Local friends ({})", rows.len())
    };
    print_section_title(&title);
    if !snapshot.failed_chat_ids.is_empty() {
        println!(
            "  note: skipped {} chats with GETMEM failures",
            snapshot.failed_chat_ids.len()
        );
    }
    if hidden {
        println!("  note: hidden output is inferred from LOCO BLSYNC/BLMEMBER and may include blocked-style entries.");
    }
    print_table(
        &["Name", "Status", "Chats", "Country", "Type", "User ID"],
        rows,
    );
    Ok(())
}

pub fn cmd_profile_rest(user_id: i64, json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let data = client.get_friend_profile(user_id)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&data)?);
        return Ok(());
    }

    let profile = data.get("profile").cloned().unwrap_or(Value::Null);
    print_section_title("Friend Profile");
    println!("  Nickname: {}", json_string(&profile, "nickname"));
    let status = json_string(&profile, "statusMessage");
    if !status.is_empty() {
        println!("  Status:   {}", status);
    }
    let image = json_string(&profile, "fullProfileImageUrl");
    if !image.is_empty() {
        println!("  Image:    {}", image);
    }

    Ok(())
}

pub fn cmd_profile_loco(chat_id: i64, user_id: i64, json: bool) -> Result<()> {
    let profiles = fetch_loco_member_profiles(chat_id)?;
    let profile = profiles
        .into_iter()
        .find(|profile| profile.user_id == user_id)
        .ok_or_else(|| anyhow::anyhow!("user {} not found in chat {}", user_id, chat_id))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
        return Ok(());
    }

    print_section_title("Friend Profile");
    println!("  Source:   LOCO GETMEM");
    println!("  Chat ID:  {}", chat_id);
    println!("  User ID:  {}", profile.user_id);
    println!("  Account:  {}", profile.account_id);
    println!("  Nickname: {}", profile.nickname);
    if !profile.status_message.is_empty() {
        println!("  Status:   {}", profile.status_message);
    }
    if !profile.country_iso.is_empty() {
        println!("  Country:  {}", profile.country_iso);
    }
    if !profile.full_profile_image_url.is_empty() {
        println!("  Image:    {}", profile.full_profile_image_url);
    } else if !profile.profile_image_url.is_empty() {
        println!("  Image:    {}", profile.profile_image_url);
    }
    if !profile.access_permit.is_empty() {
        println!("  Permit:   {}", profile.access_permit);
    }
    if !profile.suspicion.is_empty() {
        println!("  Suspicion: {}", profile.suspicion);
    }
    println!(
        "  Flags:    suspended={}, memorial={}",
        profile.suspended, profile.memorial
    );

    Ok(())
}

pub fn cmd_profile_local(user_id: i64, json: bool) -> Result<()> {
    let hint_chat_ids = load_profile_cache_hints(12)
        .ok()
        .map(|hints| collect_hint_chat_ids(&hints, user_id))
        .filter(|ids| !ids.is_empty());
    let snapshot = build_local_friend_graph_for_chat_ids(hint_chat_ids.as_deref())?;
    let profile = snapshot
        .entries
        .into_iter()
        .find(|entry| entry.user_id == user_id)
        .ok_or_else(|| anyhow::anyhow!("user {} not found in local LOCO friend graph", user_id))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&profile)?);
        return Ok(());
    }

    print_section_title("Friend Profile");
    println!("  Source:   local LOCO friend graph");
    println!("  User ID:  {}", profile.user_id);
    println!("  Account:  {}", profile.account_id);
    println!("  Nickname: {}", profile.nickname);
    if !profile.status_message.is_empty() {
        println!("  Status:   {}", profile.status_message);
    }
    if !profile.country_iso.is_empty() {
        println!("  Country:  {}", profile.country_iso);
    }
    if !profile.full_profile_image_url.is_empty() {
        println!("  Image:    {}", profile.full_profile_image_url);
    } else if !profile.profile_image_url.is_empty() {
        println!("  Image:    {}", profile.profile_image_url);
    }
    if !profile.access_permits.is_empty() {
        println!("  Permit(s): {}", profile.access_permits.join(", "));
    }
    if !profile.chat_ids.is_empty() {
        println!(
            "  Seen in:  {} chat(s) [{}]",
            profile.chat_ids.len(),
            profile
                .chat_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

pub fn cmd_profile(user_id: i64, chat_id: Option<i64>, local: bool, json: bool) -> Result<()> {
    if let Some(chat_id) = chat_id {
        match cmd_profile_loco(chat_id, user_id, json) {
            Ok(()) => return Ok(()),
            Err(err) => {
                eprintln!(
                    "[profile] LOCO chat-scoped profile failed: {err:#}. Falling back to local graph / REST profile."
                );
            }
        }
    }

    if local {
        match cmd_profile_local(user_id, json) {
            Ok(()) => return Ok(()),
            Err(err) => {
                eprintln!(
                    "[profile] local LOCO friend graph lookup failed: {err:#}. Falling back to REST profile."
                );
            }
        }
    }

    match cmd_profile_rest(user_id, json) {
        Ok(()) => Ok(()),
        Err(rest_err) => {
            eprintln!(
                "[profile] REST profile failed: {rest_err:#}. Trying local LOCO friend graph."
            );
            cmd_profile_local(user_id, json).map_err(|local_err| {
                anyhow::anyhow!(
                    "REST profile failed: {rest_err:#}\nlocal LOCO fallback also failed: {local_err:#}"
                )
            })
        }
    }
}

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
// Helper functions
// ---------------------------------------------------------------------------

pub fn merge_unique_string(values: &mut Vec<String>, candidate: &str) {
    if candidate.is_empty() || values.iter().any(|value| value == candidate) {
        return;
    }
    values.push(candidate.to_string());
}

pub fn merge_unique_i64(values: &mut Vec<i64>, candidate: i64) {
    if candidate <= 0 || values.contains(&candidate) {
        return;
    }
    values.push(candidate);
}

pub fn merge_preferred_string(current: &mut String, candidate: &str) {
    if current.is_empty() && !candidate.is_empty() {
        *current = candidate.to_string();
    }
}

pub async fn build_local_friend_graph_with_client(
    client: &mut loco::client::LocoClient,
    login_data: &bson::Document,
    self_user_id: i64,
    allowed_chat_ids: Option<&HashSet<i64>>,
) -> Result<LocalFriendGraphSnapshot> {
    let chats = fetch_loco_chat_listings_with_client(client, login_data, true)
        .await?
        .into_iter()
        .filter(|chat| {
            allowed_chat_ids
                .map(|ids| ids.contains(&chat.chat_id))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    let mut graph = BTreeMap::<i64, LocalFriendGraphEntry>::new();
    let mut failed_chat_ids = Vec::new();
    let mut chat_meta = Vec::new();

    for chat in &chats {
        match fetch_loco_member_profiles_with_client(client, chat.chat_id).await {
            Ok(getmem) => {
                chat_meta.push(LocalFriendGraphChatMeta {
                    chat_id: chat.chat_id,
                    title: chat.title.clone(),
                    getmem_token: getmem.token,
                    member_count: getmem.members.len(),
                });

                for member in getmem.members {
                    let entry =
                        graph
                            .entry(member.user_id)
                            .or_insert_with(|| LocalFriendGraphEntry {
                                user_id: member.user_id,
                                account_id: member.account_id,
                                nickname: member.nickname.clone(),
                                country_iso: member.country_iso.clone(),
                                status_message: member.status_message.clone(),
                                profile_image_url: member.profile_image_url.clone(),
                                full_profile_image_url: member.full_profile_image_url.clone(),
                                original_profile_image_url: member
                                    .original_profile_image_url
                                    .clone(),
                                access_permits: Vec::new(),
                                suspicion: member.suspicion.clone(),
                                suspended: member.suspended,
                                memorial: member.memorial,
                                member_type: member.member_type,
                                chat_ids: Vec::new(),
                                chat_titles: Vec::new(),
                                is_self: member.user_id == self_user_id,
                                hidden_like: false,
                                hidden_block_type: None,
                            });

                    if entry.account_id == 0 && member.account_id != 0 {
                        entry.account_id = member.account_id;
                    }
                    merge_preferred_string(&mut entry.nickname, &member.nickname);
                    merge_preferred_string(&mut entry.country_iso, &member.country_iso);
                    merge_preferred_string(&mut entry.status_message, &member.status_message);
                    merge_preferred_string(&mut entry.profile_image_url, &member.profile_image_url);
                    merge_preferred_string(
                        &mut entry.full_profile_image_url,
                        &member.full_profile_image_url,
                    );
                    merge_preferred_string(
                        &mut entry.original_profile_image_url,
                        &member.original_profile_image_url,
                    );
                    merge_preferred_string(&mut entry.suspicion, &member.suspicion);
                    if member.suspended {
                        entry.suspended = true;
                    }
                    if member.memorial {
                        entry.memorial = true;
                    }
                    if entry.member_type == 0 && member.member_type != 0 {
                        entry.member_type = member.member_type;
                    }
                    merge_unique_i64(&mut entry.chat_ids, chat.chat_id);
                    merge_unique_string(&mut entry.chat_titles, &chat.title);
                    merge_unique_string(&mut entry.access_permits, &member.access_permit);
                }
            }
            Err(err) => {
                eprintln!("[friends/local] GETMEM {} failed: {}", chat.chat_id, err);
                failed_chat_ids.push(chat.chat_id);
            }
        }
    }

    let entries = graph.into_values().collect::<Vec<_>>();
    Ok(LocalFriendGraphSnapshot {
        user_count: entries.len(),
        chat_count: chats.len(),
        failed_chat_ids,
        chat_meta,
        entries,
    })
}

pub fn merge_blocked_members_into_local_graph(
    snapshot: &mut LocalFriendGraphSnapshot,
    blocked: LocoBlockedSnapshot,
) {
    let mut graph = snapshot
        .entries
        .drain(..)
        .map(|entry| (entry.user_id, entry))
        .collect::<BTreeMap<_, _>>();

    for member in blocked.members {
        let entry = graph
            .entry(member.user_id)
            .or_insert_with(|| LocalFriendGraphEntry {
                user_id: member.user_id,
                account_id: 0,
                nickname: member.nickname.clone(),
                country_iso: String::new(),
                status_message: String::new(),
                profile_image_url: member.profile_image_url.clone(),
                full_profile_image_url: member.full_profile_image_url.clone(),
                original_profile_image_url: String::new(),
                access_permits: Vec::new(),
                suspicion: member.suspicion.clone(),
                suspended: member.suspended,
                memorial: false,
                member_type: -1,
                chat_ids: Vec::new(),
                chat_titles: Vec::new(),
                is_self: false,
                hidden_like: true,
                hidden_block_type: Some(member.block_type),
            });

        merge_preferred_string(&mut entry.nickname, &member.nickname);
        merge_preferred_string(&mut entry.profile_image_url, &member.profile_image_url);
        merge_preferred_string(
            &mut entry.full_profile_image_url,
            &member.full_profile_image_url,
        );
        merge_preferred_string(&mut entry.suspicion, &member.suspicion);
        if member.suspended {
            entry.suspended = true;
        }
        entry.hidden_like = true;
        entry.hidden_block_type = Some(member.block_type);
    }

    snapshot.user_count = graph.len();
    snapshot.entries = graph.into_values().collect();
}

pub fn build_local_friend_graph_for_chat_ids(
    allowed_chat_ids: Option<&[i64]>,
) -> Result<LocalFriendGraphSnapshot> {
    let creds = get_creds()?;
    let self_user_id = creds.user_id;
    let allowed_chat_ids = allowed_chat_ids.map(|ids| ids.iter().copied().collect::<HashSet<_>>());

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = loco_connect_with_auto_refresh(&mut client).await?;
        build_local_friend_graph_with_client(
            &mut client,
            &login_data,
            self_user_id,
            allowed_chat_ids.as_ref(),
        )
        .await
    })
}

pub fn build_local_friend_graph() -> Result<LocalFriendGraphSnapshot> {
    build_local_friend_graph_for_chat_ids(None)
}

pub fn collect_hint_chat_ids(cached_requests: &[ProfileCacheHint], user_id: i64) -> Vec<i64> {
    let mut chat_ids = cached_requests
        .iter()
        .filter(|hint| hint.user_ids.contains(&user_id))
        .filter_map(|hint| hint.chat_id)
        .collect::<Vec<_>>();
    chat_ids.sort_unstable();
    chat_ids.dedup();
    chat_ids
}

pub fn local_graph_hint_summary(
    snapshot: &LocalFriendGraphSnapshot,
    cached_requests: &[ProfileCacheHint],
) -> LocalFriendGraphHintSummary {
    let by_user_id = snapshot
        .entries
        .iter()
        .map(|entry| (entry.user_id, entry))
        .collect::<HashMap<_, _>>();

    let candidate_matches = cached_requests
        .iter()
        .filter(|hint| !hint.user_ids.is_empty())
        .map(|hint| {
            let matched = hint
                .user_ids
                .iter()
                .filter_map(|user_id| by_user_id.get(user_id).copied())
                .collect::<Vec<_>>();

            let mut candidate_chat_ids = Vec::new();
            let mut candidate_access_permits = Vec::new();
            let mut candidate_getmem_tokens = Vec::new();
            for entry in &matched {
                for chat_id in &entry.chat_ids {
                    merge_unique_i64(&mut candidate_chat_ids, *chat_id);
                }
                for permit in &entry.access_permits {
                    merge_unique_string(&mut candidate_access_permits, permit);
                }
            }
            for chat in &snapshot.chat_meta {
                if candidate_chat_ids.contains(&chat.chat_id) {
                    if let Some(token) = chat.getmem_token {
                        merge_unique_i64(&mut candidate_getmem_tokens, token);
                    }
                }
            }

            LocalFriendGraphHintMatch {
                entry_id: hint.entry_id,
                kind: hint.kind.clone(),
                requested_user_ids: hint.user_ids.clone(),
                matched_user_ids: matched.iter().map(|entry| entry.user_id).collect(),
                candidate_chat_ids,
                candidate_access_permits,
                candidate_getmem_tokens,
            }
        })
        .collect::<Vec<_>>();

    LocalFriendGraphHintSummary {
        user_count: snapshot.user_count,
        chat_count: snapshot.chat_count,
        failed_chat_ids: snapshot.failed_chat_ids.clone(),
        chat_meta: snapshot.chat_meta.clone(),
        candidate_matches,
    }
}

pub fn push_unique_candidate_body(
    bodies: &mut Vec<serde_json::Value>,
    seen: &mut HashSet<String>,
    body: serde_json::Value,
) {
    if let Ok(key) = serde_json::to_string(&body) {
        if seen.insert(key) {
            bodies.push(body);
        }
    }
}

pub fn build_syncmainpf_candidate(
    snapshot: &LocalFriendGraphSnapshot,
    cached_requests: &[ProfileCacheHint],
    user_id: i64,
) -> Option<SyncMainPfCandidate> {
    let entry = snapshot
        .entries
        .iter()
        .find(|entry| entry.user_id == user_id)?;

    let mut source_entry_ids = cached_requests
        .iter()
        .filter(|hint| hint.user_ids.contains(&user_id))
        .map(|hint| hint.entry_id)
        .collect::<Vec<_>>();
    source_entry_ids.sort_unstable();
    source_entry_ids.dedup();

    let pfids = [entry.user_id, entry.account_id]
        .into_iter()
        .filter(|value| *value > 0)
        .collect::<Vec<_>>();
    let string_pfids = {
        let mut values = Vec::new();
        for candidate in [
            Some(entry.user_id.to_string()),
            (entry.account_id > 0).then(|| entry.account_id.to_string()),
        ]
        .into_iter()
        .flatten()
        {
            if !values.contains(&candidate) {
                values.push(candidate);
            }
        }
        values
    };
    let chat_ids = if entry.chat_ids.is_empty() {
        vec![None]
    } else {
        entry.chat_ids.iter().copied().map(Some).collect::<Vec<_>>()
    };
    let access_permits = if entry.access_permits.is_empty() {
        vec![None]
    } else {
        entry
            .access_permits
            .iter()
            .cloned()
            .map(Some)
            .collect::<Vec<_>>()
    };
    let getmem_tokens = snapshot
        .chat_meta
        .iter()
        .filter(|chat| entry.chat_ids.contains(&chat.chat_id))
        .filter_map(|chat| chat.getmem_token)
        .collect::<Vec<_>>();

    let mut bodies = Vec::new();
    let mut uplinkprof_bodies = Vec::new();
    let mut seen = HashSet::new();
    let mut uplink_seen = HashSet::new();

    if entry.is_self {
        for pfid in &pfids {
            push_unique_candidate_body(
                &mut bodies,
                &mut seen,
                serde_json::json!({
                    "ct": "me",
                    "pfid": pfid,
                }),
            );
        }
        for pfid in &string_pfids {
            push_unique_candidate_body(
                &mut bodies,
                &mut seen,
                serde_json::json!({
                    "ct": "me",
                    "pfid": pfid,
                }),
            );
        }
    }

    for pfid in &pfids {
        for chat_id in &chat_ids {
            for access_permit in &access_permits {
                for ct in ["d", "p"] {
                    let mut body = serde_json::Map::new();
                    body.insert("ct".into(), serde_json::json!(ct));
                    body.insert("pfid".into(), serde_json::json!(pfid));
                    if let Some(chat_id) = chat_id {
                        body.insert("chatId".into(), serde_json::json!(chat_id));
                    }
                    if let Some(access_permit) = access_permit {
                        body.insert("accessPermit".into(), serde_json::json!(access_permit));
                    }
                    push_unique_candidate_body(
                        &mut bodies,
                        &mut seen,
                        serde_json::Value::Object(body),
                    );
                }
            }
        }
    }

    for pfid in &string_pfids {
        for chat_id in &chat_ids {
            for access_permit in &access_permits {
                for ct in ["d", "p"] {
                    let mut body = serde_json::Map::new();
                    body.insert("ct".into(), serde_json::json!(ct));
                    body.insert("pfid".into(), serde_json::json!(pfid));
                    if let Some(chat_id) = chat_id {
                        body.insert("chatId".into(), serde_json::json!(chat_id));
                    }
                    if let Some(access_permit) = access_permit {
                        body.insert("accessPermit".into(), serde_json::json!(access_permit));
                    }
                    push_unique_candidate_body(
                        &mut bodies,
                        &mut seen,
                        serde_json::Value::Object(body),
                    );
                }
            }
        }
    }

    for token in &getmem_tokens {
        for chat_id in &chat_ids {
            for access_permit in &access_permits {
                for ct in ["d", "p"] {
                    let mut token_body = serde_json::Map::new();
                    token_body.insert("ct".into(), serde_json::json!(ct));
                    token_body.insert("token".into(), serde_json::json!(token));
                    if let Some(chat_id) = chat_id {
                        token_body.insert("chatId".into(), serde_json::json!(chat_id));
                    }
                    if let Some(access_permit) = access_permit {
                        token_body.insert("accessPermit".into(), serde_json::json!(access_permit));
                    }
                    push_unique_candidate_body(
                        &mut bodies,
                        &mut seen,
                        serde_json::Value::Object(token_body),
                    );

                    let mut profile_token_body = serde_json::Map::new();
                    profile_token_body.insert("ct".into(), serde_json::json!(ct));
                    profile_token_body.insert("profileToken".into(), serde_json::json!(token));
                    if let Some(chat_id) = chat_id {
                        profile_token_body.insert("chatId".into(), serde_json::json!(chat_id));
                    }
                    if let Some(access_permit) = access_permit {
                        profile_token_body
                            .insert("accessPermit".into(), serde_json::json!(access_permit));
                    }
                    push_unique_candidate_body(
                        &mut bodies,
                        &mut seen,
                        serde_json::Value::Object(profile_token_body),
                    );
                }
            }
        }
    }

    for pfid in &pfids {
        push_unique_candidate_body(
            &mut uplinkprof_bodies,
            &mut uplink_seen,
            serde_json::json!({ "pfid": pfid }),
        );
        for relation in ["n", "r"] {
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "pfid": pfid, "r": relation }),
            );
        }
        for access_permit in access_permits.iter().flatten() {
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "pfid": pfid, "F": access_permit }),
            );
            for relation in ["n", "r"] {
                push_unique_candidate_body(
                    &mut uplinkprof_bodies,
                    &mut uplink_seen,
                    serde_json::json!({ "pfid": pfid, "F": access_permit, "r": relation }),
                );
            }
        }

        for profile_type in 0..=4 {
            for key in ["t", "profileType"] {
                push_unique_candidate_body(
                    &mut uplinkprof_bodies,
                    &mut uplink_seen,
                    serde_json::json!({ "pfid": pfid, key: profile_type }),
                );
                push_unique_candidate_body(
                    &mut uplinkprof_bodies,
                    &mut uplink_seen,
                    serde_json::json!({ "pfid": pfid, key: profile_type, "mp": "y" }),
                );
                for relation in ["n", "r"] {
                    push_unique_candidate_body(
                        &mut uplinkprof_bodies,
                        &mut uplink_seen,
                        serde_json::json!({ "pfid": pfid, key: profile_type, "r": relation }),
                    );
                    push_unique_candidate_body(
                        &mut uplinkprof_bodies,
                        &mut uplink_seen,
                        serde_json::json!({ "pfid": pfid, key: profile_type, "r": relation, "mp": "y" }),
                    );
                }
                for access_permit in access_permits.iter().flatten() {
                    push_unique_candidate_body(
                        &mut uplinkprof_bodies,
                        &mut uplink_seen,
                        serde_json::json!({ "pfid": pfid, "F": access_permit, key: profile_type }),
                    );
                    push_unique_candidate_body(
                        &mut uplinkprof_bodies,
                        &mut uplink_seen,
                        serde_json::json!({ "pfid": pfid, "F": access_permit, key: profile_type, "mp": "y" }),
                    );
                    for relation in ["n", "r"] {
                        push_unique_candidate_body(
                            &mut uplinkprof_bodies,
                            &mut uplink_seen,
                            serde_json::json!({ "pfid": pfid, "F": access_permit, key: profile_type, "r": relation }),
                        );
                        push_unique_candidate_body(
                            &mut uplinkprof_bodies,
                            &mut uplink_seen,
                            serde_json::json!({ "pfid": pfid, "F": access_permit, key: profile_type, "r": relation, "mp": "y" }),
                        );
                    }
                }
            }
        }
    }

    for pfid in &string_pfids {
        push_unique_candidate_body(
            &mut uplinkprof_bodies,
            &mut uplink_seen,
            serde_json::json!({ "pfid": pfid }),
        );
        for access_permit in access_permits.iter().flatten() {
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "pfid": pfid, "F": access_permit }),
            );
        }
    }

    for token in &getmem_tokens {
        push_unique_candidate_body(
            &mut uplinkprof_bodies,
            &mut uplink_seen,
            serde_json::json!({ "token": token }),
        );
        push_unique_candidate_body(
            &mut uplinkprof_bodies,
            &mut uplink_seen,
            serde_json::json!({ "profileToken": token }),
        );
        for access_permit in access_permits.iter().flatten() {
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "token": token, "F": access_permit }),
            );
            push_unique_candidate_body(
                &mut uplinkprof_bodies,
                &mut uplink_seen,
                serde_json::json!({ "profileToken": token, "F": access_permit }),
            );
        }
    }

    Some(SyncMainPfCandidate {
        user_id: entry.user_id,
        account_id: entry.account_id,
        is_self: entry.is_self,
        source_entry_ids,
        getmem_tokens,
        bodies,
        uplinkprof_bodies,
    })
}

pub fn build_syncmainpf_probe_variants(candidate: &SyncMainPfCandidate) -> Vec<serde_json::Value> {
    let mut variants = Vec::new();
    let mut seen = HashSet::new();

    for body in &candidate.bodies {
        push_unique_candidate_body(&mut variants, &mut seen, body.clone());

        for profile_type in 0..=4 {
            let with_profile_type = match body {
                serde_json::Value::Object(map) => {
                    let mut body = map.clone();
                    body.insert("profileType".into(), serde_json::json!(profile_type));
                    serde_json::Value::Object(body)
                }
                _ => continue,
            };
            push_unique_candidate_body(&mut variants, &mut seen, with_profile_type.clone());
            let with_t = match &with_profile_type {
                serde_json::Value::Object(map) => {
                    let mut body = map.clone();
                    body.insert("t".into(), serde_json::json!(profile_type));
                    serde_json::Value::Object(body)
                }
                _ => continue,
            };
            push_unique_candidate_body(&mut variants, &mut seen, with_t.clone());
            let with_mp = match &with_t {
                serde_json::Value::Object(map) => {
                    let mut body = map.clone();
                    body.insert("mp".into(), serde_json::json!("y"));
                    serde_json::Value::Object(body)
                }
                _ => continue,
            };
            push_unique_candidate_body(&mut variants, &mut seen, with_mp.clone());

            for relation in ["n", "r"] {
                let with_relation = match &with_profile_type {
                    serde_json::Value::Object(map) => {
                        let mut body = map.clone();
                        body.insert("r".into(), serde_json::json!(relation));
                        serde_json::Value::Object(body)
                    }
                    _ => continue,
                };
                push_unique_candidate_body(&mut variants, &mut seen, with_relation);
                let with_t_relation = match &with_t {
                    serde_json::Value::Object(map) => {
                        let mut body = map.clone();
                        body.insert("r".into(), serde_json::json!(relation));
                        serde_json::Value::Object(body)
                    }
                    _ => continue,
                };
                push_unique_candidate_body(&mut variants, &mut seen, with_t_relation);
                let with_mp_relation = match &with_mp {
                    serde_json::Value::Object(map) => {
                        let mut body = map.clone();
                        body.insert("r".into(), serde_json::json!(relation));
                        serde_json::Value::Object(body)
                    }
                    _ => continue,
                };
                push_unique_candidate_body(&mut variants, &mut seen, with_mp_relation);
            }
        }
    }

    variants
}

pub async fn probe_syncmainpf_variants(
    variants: &[serde_json::Value],
) -> Result<Vec<SyncMainPfProbeResult>> {
    let raw = probe_method_variants("SYNCMAINPF", variants).await?;
    Ok(raw
        .into_iter()
        .map(|result| SyncMainPfProbeResult {
            body: result.body,
            packet_status_code: result.packet_status_code,
            body_status: result.body_status,
            push_count: result.push_count,
            push_methods: result.push_methods,
        })
        .collect())
}

pub fn build_uplinkprof_probe_variants(candidate: &SyncMainPfCandidate) -> Vec<serde_json::Value> {
    candidate.uplinkprof_bodies.clone()
}

// ---------------------------------------------------------------------------
// File system helpers
// ---------------------------------------------------------------------------

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

pub fn parse_i64_list(raw: &str) -> Vec<i64> {
    raw.trim_matches(&['[', ']'][..])
        .split(',')
        .filter_map(|part| part.trim().parse::<i64>().ok())
        .collect()
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

pub fn load_profile_cache_hints(limit: usize) -> Result<Vec<ProfileCacheHint>> {
    let cache_db = kakao_cache_db_path();
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
    let prefs_dir = kakao_preferences_dir();
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
