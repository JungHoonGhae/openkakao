use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::Result;

use super::{
    LocalFriendGraphChatMeta, LocalFriendGraphEntry, LocalFriendGraphHintMatch,
    LocalFriendGraphHintSummary, LocalFriendGraphSnapshot, ProfileCacheHint,
};
use crate::commands::chats::fetch_loco_chat_listings_with_client;
use crate::commands::members::{fetch_loco_member_profiles_with_client, LocoBlockedSnapshot};
use crate::loco;
use crate::loco_helpers::loco_connect_with_auto_refresh;
use crate::util::get_creds;

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
