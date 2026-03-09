pub(crate) mod app_state;
pub(crate) mod graph;
pub(crate) mod hints;
pub(crate) mod probe;

// Re-export public API items used from main.rs, rest.rs, and tests.
pub use app_state::*;
pub use graph::*;
pub use hints::*;
pub use probe::*;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::members::fetch_loco_member_profiles;
use crate::commands::probe::MethodProbeResult;
use crate::commands::rest::filter_friend_search;
use crate::loco;
use crate::loco_helpers::loco_connect_with_auto_refresh;
use crate::model::json_string;
use crate::util::{get_creds, get_rest_client, print_section_title, print_table, truncate};

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
            crate::commands::members::fetch_loco_blocked_snapshot(&mut client).await
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
