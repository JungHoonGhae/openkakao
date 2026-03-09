use std::collections::HashSet;

use anyhow::Result;

use super::{
    LocalFriendGraphSnapshot, ProfileCacheHint, SyncMainPfCandidate, SyncMainPfProbeResult,
};
use crate::commands::probe::probe_method_variants;

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
