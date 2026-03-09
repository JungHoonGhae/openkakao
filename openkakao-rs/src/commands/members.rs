use anyhow::Result;
use owo_colors::OwoColorize;
use serde::Serialize;
use std::time::Duration;

use crate::loco;
use crate::loco_helpers::{
    loco_connect_with_auto_refresh, reconnect_loco_probe_client, should_retry_loco_probe_error,
};
use crate::model::ChatMember;
use crate::util::{
    color_enabled, get_bson_bool, get_bson_i32, get_bson_i32_array, get_bson_i64,
    get_bson_i64_array, get_bson_str, get_creds, get_rest_client, print_section_title, print_table,
    truncate,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocoMemberProfile {
    pub user_id: i64,
    pub account_id: i64,
    pub nickname: String,
    pub country_iso: String,
    pub status_message: String,
    pub profile_image_url: String,
    pub full_profile_image_url: String,
    pub original_profile_image_url: String,
    pub access_permit: String,
    pub suspicion: String,
    pub suspended: bool,
    pub memorial: bool,
    pub member_type: i32,
    pub ut: i64,
}

#[derive(Debug, Clone)]
pub struct LocoGetMemSnapshot {
    pub token: Option<i64>,
    pub members: Vec<LocoMemberProfile>,
}

impl LocoMemberProfile {
    pub fn from_getmem_doc(doc: &bson::Document) -> Self {
        Self {
            user_id: get_bson_i64(doc, &["userId"]),
            account_id: get_bson_i64(doc, &["accountId"]),
            nickname: get_bson_str(doc, &["nickName", "nickname"]),
            country_iso: get_bson_str(doc, &["countryIso"]),
            status_message: get_bson_str(doc, &["statusMessage"]),
            profile_image_url: get_bson_str(doc, &["profileImageUrl"]),
            full_profile_image_url: get_bson_str(doc, &["fullProfileImageUrl"]),
            original_profile_image_url: get_bson_str(doc, &["originalProfileImageUrl"]),
            access_permit: get_bson_str(doc, &["accessPermit"]),
            suspicion: get_bson_str(doc, &["suspicion"]),
            suspended: get_bson_bool(doc, &["suspended"]),
            memorial: get_bson_bool(doc, &["memorial"]),
            member_type: get_bson_i32(doc, &["type"]),
            ut: get_bson_i64(doc, &["ut"]),
        }
    }

    pub fn as_chat_member(&self) -> ChatMember {
        ChatMember {
            user_id: self.user_id,
            nickname: self.nickname.clone(),
            friend_nickname: String::new(),
            country_iso: self.country_iso.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LocoBlockedMember {
    pub user_id: i64,
    pub nickname: String,
    pub profile_image_url: String,
    pub full_profile_image_url: String,
    pub suspended: bool,
    pub suspicion: String,
    pub block_type: i32,
    pub is_plus: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocoBlockedSnapshot {
    pub revision: i64,
    pub plus_revision: i64,
    pub members: Vec<LocoBlockedMember>,
}

pub async fn fetch_loco_member_profiles_with_client(
    client: &mut loco::client::LocoClient,
    chat_id: i64,
) -> Result<LocoGetMemSnapshot> {
    let mut last_error = None;
    for attempt in 0..3 {
        let response = match client
            .send_command("GETMEM", bson::doc! { "chatId": chat_id })
            .await
        {
            Ok(response) => response,
            Err(error) if should_retry_loco_probe_error(&error) && attempt < 2 => {
                last_error = Some(error);
                reconnect_loco_probe_client(client).await?;
                continue;
            }
            Err(error) => return Err(error),
        };

        if response.status() != 0 {
            anyhow::bail!("GETMEM failed (status={})", response.status());
        }

        let members = response
            .body
            .get_array("members")
            .map(|a| a.to_vec())
            .unwrap_or_default();
        let token = response.body.get_i64("token").ok();

        return Ok(LocoGetMemSnapshot {
            token,
            members: members
                .iter()
                .filter_map(|member| member.as_document().map(LocoMemberProfile::from_getmem_doc))
                .collect::<Vec<_>>(),
        });
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("GETMEM retry loop exhausted")))
}

pub async fn fetch_loco_member_profiles_only_with_client(
    client: &mut loco::client::LocoClient,
    chat_id: i64,
) -> Result<Vec<LocoMemberProfile>> {
    Ok(fetch_loco_member_profiles_with_client(client, chat_id)
        .await?
        .members)
}

pub fn fetch_loco_member_profiles(chat_id: i64) -> Result<Vec<LocoMemberProfile>> {
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;
        fetch_loco_member_profiles_only_with_client(&mut client, chat_id).await
    })
}

pub async fn fetch_loco_blocked_snapshot(
    client: &mut loco::client::LocoClient,
) -> Result<LocoBlockedSnapshot> {
    let sync_result = client
        .send_command_collect(
            "BLSYNC",
            bson::doc! { "r": 0_i32, "pr": 0_i32 },
            Duration::from_secs(3),
        )
        .await?;

    let sync_packet = sync_result
        .response
        .as_ref()
        .or_else(|| {
            sync_result
                .pushes
                .iter()
                .find(|packet| packet.method == "BLSYNC")
        })
        .ok_or_else(|| anyhow::anyhow!("BLSYNC returned neither a direct response nor a push"))?;

    let revision = get_bson_i64(&sync_packet.body, &["r", "revision"]);
    let plus_revision = get_bson_i64(&sync_packet.body, &["pr", "plusRevision"]);
    let ids = get_bson_i64_array(&sync_packet.body, &["l", "blockIds"]);
    let types = get_bson_i32_array(&sync_packet.body, &["ts", "blockTypes"]);
    let plus_ids = get_bson_i64_array(&sync_packet.body, &["pl", "plusBlockIds"]);
    let plus_types = get_bson_i32_array(&sync_packet.body, &["pts", "plusBlockTypes"]);

    if ids.is_empty() && plus_ids.is_empty() {
        return Ok(LocoBlockedSnapshot {
            revision,
            plus_revision,
            members: Vec::new(),
        });
    }

    let member_body = bson::doc! {
        "l": bson::Bson::Array(ids.iter().copied().map(bson::Bson::Int64).collect()),
        "pl": bson::Bson::Array(plus_ids.iter().copied().map(bson::Bson::Int64).collect()),
    };
    let member_result = client
        .send_command_collect("BLMEMBER", member_body, Duration::from_secs(3))
        .await?;
    let member_packet = member_result
        .response
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("BLMEMBER did not return a direct response"))?;

    if member_packet.status() != 0 {
        anyhow::bail!("BLMEMBER failed (status={})", member_packet.status());
    }

    let mut members = Vec::new();
    if let Ok(entries) = member_packet.body.get_array("l") {
        for (idx, entry) in entries.iter().enumerate() {
            if let Some(doc) = entry.as_document() {
                members.push(LocoBlockedMember {
                    user_id: get_bson_i64(doc, &["userId"]),
                    nickname: get_bson_str(doc, &["nickName", "nickname"]),
                    profile_image_url: get_bson_str(doc, &["profileImageUrl"]),
                    full_profile_image_url: get_bson_str(doc, &["fullProfileImageUrl"]),
                    suspended: get_bson_bool(doc, &["suspended"]),
                    suspicion: get_bson_str(doc, &["suspicion"]),
                    block_type: types.get(idx).copied().unwrap_or(0),
                    is_plus: false,
                });
            }
        }
    }
    if let Ok(entries) = member_packet.body.get_array("pl") {
        for (idx, entry) in entries.iter().enumerate() {
            if let Some(doc) = entry.as_document() {
                members.push(LocoBlockedMember {
                    user_id: get_bson_i64(doc, &["userId"]),
                    nickname: get_bson_str(doc, &["nickName", "nickname"]),
                    profile_image_url: get_bson_str(doc, &["profileImageUrl"]),
                    full_profile_image_url: get_bson_str(doc, &["fullProfileImageUrl"]),
                    suspended: get_bson_bool(doc, &["suspended"]),
                    suspicion: get_bson_str(doc, &["suspicion"]),
                    block_type: plus_types.get(idx).copied().unwrap_or(0),
                    is_plus: true,
                });
            }
        }
    }

    Ok(LocoBlockedSnapshot {
        revision,
        plus_revision,
        members,
    })
}

pub fn cmd_loco_members(chat_id: i64, full: bool, json: bool) -> Result<()> {
    let profiles = fetch_loco_member_profiles(chat_id)?;

    if json {
        if full {
            println!("{}", serde_json::to_string_pretty(&profiles)?);
        } else {
            let members = profiles
                .iter()
                .map(LocoMemberProfile::as_chat_member)
                .collect::<Vec<_>>();
            println!("{}", serde_json::to_string_pretty(&members)?);
        }
        return Ok(());
    }

    if full {
        print_section_title(&format!(
            "Members of chat {} ({} members)",
            chat_id,
            profiles.len()
        ));
        let rows = profiles
            .iter()
            .map(|profile| {
                vec![
                    profile.nickname.clone(),
                    truncate(&profile.status_message, 30),
                    profile.country_iso.clone(),
                    if profile.suspended {
                        "yes".into()
                    } else {
                        "no".into()
                    },
                    profile.user_id.to_string(),
                ]
            })
            .collect::<Vec<_>>();
        print_table(&["Name", "Status", "Country", "Suspended", "User ID"], rows);
        return Ok(());
    }

    print_section_title(&format!(
        "Members of chat {} ({} members)",
        chat_id,
        profiles.len()
    ));
    for profile in &profiles {
        if color_enabled() {
            println!(
                "  {} {}",
                format!("{}", profile.user_id).dimmed(),
                profile.nickname.bold()
            );
        } else {
            println!("  {} {}", profile.user_id, profile.nickname);
        }
    }

    Ok(())
}

pub fn cmd_members_rest(chat_id: i64, json: bool) -> Result<()> {
    let client = get_rest_client()?;
    let members = client.get_chat_members(chat_id)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&members)?);
        return Ok(());
    }

    let mut rows = Vec::new();
    for m in members {
        rows.push(vec![m.display_name(), m.user_id.to_string(), m.country_iso]);
    }

    print_section_title(&format!("Members ({})", rows.len()));
    print_table(&["Name", "User ID", "Country"], rows);
    Ok(())
}

pub fn cmd_members(chat_id: i64, rest: bool, full: bool, json: bool) -> Result<()> {
    if rest {
        return cmd_members_rest(chat_id, json);
    }

    match cmd_loco_members(chat_id, full, json) {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!(
                "[members] LOCO member list failed: {err:#}. Falling back to REST member list."
            );
            cmd_members_rest(chat_id, json)
        }
    }
}

pub fn cmd_loco_blocked(json: bool) -> Result<()> {
    let creds = get_creds()?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        loco_connect_with_auto_refresh(&mut client).await?;

        let snapshot = fetch_loco_blocked_snapshot(&mut client).await?;

        if json {
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            return Ok(());
        }

        print_section_title(&format!(
            "LOCO blocked members ({})",
            snapshot.members.len()
        ));
        println!(
            "  revision={} plus_revision={}",
            snapshot.revision, snapshot.plus_revision
        );

        let rows = snapshot
            .members
            .iter()
            .map(|member| {
                vec![
                    member.nickname.clone(),
                    member.block_type.to_string(),
                    if member.is_plus {
                        "plus".into()
                    } else {
                        "user".into()
                    },
                    if member.suspended {
                        "yes".into()
                    } else {
                        "no".into()
                    },
                    member.user_id.to_string(),
                ]
            })
            .collect::<Vec<_>>();
        print_table(&["Name", "Type", "Scope", "Suspended", "User ID"], rows);
        Ok(())
    })
}
