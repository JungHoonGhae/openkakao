use anyhow::Result;
use serde::Serialize;
use tokio::runtime::Runtime;

use crate::loco;
use crate::loco_helpers::loco_connect_with_auto_refresh;
use crate::util::{
    get_bson_i32, get_bson_i64, get_bson_str, get_bson_str_array, get_creds, get_rest_client,
    print_section_title, print_table, type_label,
};

#[derive(Debug, Clone, Serialize)]
pub struct ChatListing {
    pub chat_id: i64,
    pub kind: String,
    pub title: String,
    pub has_unread: bool,
    pub unread_count: Option<i64>,
    pub active_members: Option<i32>,
    pub last_log_id: Option<i64>,
    pub last_seen_log_id: Option<i64>,
}

pub fn cmd_chats_rest(
    show_all: bool,
    unread: bool,
    search: Option<String>,
    chat_type: Option<String>,
    json: bool,
) -> Result<()> {
    let client = get_rest_client()?;

    let mut chats = if show_all {
        client.get_all_chats()?
    } else {
        client.get_chats(None)?.0
    };

    if unread {
        chats.retain(|c| c.unread_count > 0);
    }

    if let Some(ref query) = search {
        let q = query.to_lowercase();
        chats.retain(|c| c.display_title().to_lowercase().contains(&q));
    }

    if let Some(ref t) = chat_type {
        let lowered = t.to_lowercase();
        let kind = match lowered.as_str() {
            "dm" => "DirectChat".to_string(),
            "group" => "MultiChat".to_string(),
            "memo" => "MemoChat".to_string(),
            "open" => "OpenMultiChat".to_string(),
            "opendm" => "OpenDirectChat".to_string(),
            other => other.to_string(),
        };
        chats.retain(|c| c.kind == kind);
    }

    let listings = chats
        .into_iter()
        .map(|chat| {
            let title = chat.display_title();
            let active_members = chat.display_members.len() as i32;
            ChatListing {
                chat_id: chat.chat_id,
                kind: chat.kind,
                title,
                has_unread: chat.unread_count > 0,
                unread_count: Some(chat.unread_count),
                active_members: Some(active_members),
                last_log_id: None,
                last_seen_log_id: None,
            }
        })
        .collect::<Vec<_>>();

    if json {
        println!("{}", serde_json::to_string_pretty(&listings)?);
        return Ok(());
    }

    let mut rows = Vec::new();
    for c in listings {
        let kind = type_label(&c.kind);
        let unread_str = if c.has_unread {
            c.unread_count.unwrap_or(1).to_string()
        } else {
            String::new()
        };

        rows.push(vec![
            kind.to_string(),
            c.title,
            unread_str,
            c.chat_id.to_string(),
        ]);
    }

    print_section_title(&format!("Chats ({})", rows.len()));
    print_table(&["Type", "Name", "Unread", "Chat ID"], rows);
    Ok(())
}

pub fn cmd_chats(
    show_all: bool,
    unread: bool,
    search: Option<String>,
    chat_type: Option<String>,
    rest: bool,
    json: bool,
) -> Result<()> {
    if rest {
        return cmd_chats_rest(show_all, unread, search, chat_type, json);
    }

    match cmd_loco_chats(show_all, unread, search.clone(), chat_type.clone(), json) {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!(
                "[chats] LOCO chat list failed: {}. Falling back to REST recent chat list.",
                err
            );
            cmd_chats_rest(show_all, unread, search, chat_type, json)
        }
    }
}

pub async fn fetch_loco_chat_listings_with_client(
    client: &mut loco::client::LocoClient,
    login_data: &bson::Document,
    show_all: bool,
) -> Result<Vec<ChatListing>> {
    let response = client
        .send_command(
            "LCHATLIST",
            bson::doc! {
                "chatIds": bson::Bson::Array(vec![]),
                "maxIds": bson::Bson::Array(vec![]),
                "lastTokenId": 0_i64,
                "lastChatId": 0_i64,
            },
        )
        .await?;

    let lchat_status = response.status();
    eprintln!("[loco-chats] LCHATLIST status={}", lchat_status);

    let chat_datas = if lchat_status == 0 {
        response.body.get_array("chatDatas").ok()
    } else {
        None
    };
    let chat_datas = chat_datas.or_else(|| login_data.get_array("chatDatas").ok());
    let Some(chat_datas) = chat_datas else {
        return Ok(Vec::new());
    };

    let mut chats = Vec::new();
    for cd in chat_datas {
        if let Some(doc) = cd.as_document() {
            let chat_id = get_bson_i64(doc, &["c", "chatId"]);
            let kind = get_bson_str(doc, &["t", "type"]);
            let last_log_id = get_bson_i64(doc, &["s", "lastLogId"]);
            let last_seen = get_bson_i64(doc, &["ll", "lastSeenLogId"]);
            let has_unread = last_log_id > last_seen;
            let active_member_count = get_bson_i32(doc, &["a", "activeMembersCount"]);

            let title = doc
                .get_document("chatInfo")
                .ok()
                .and_then(|ci| ci.get_str("name").ok())
                .map(String::from)
                .unwrap_or_default();
            let title = if title.is_empty() {
                get_bson_str_array(doc, &["k"]).join(", ")
            } else {
                title
            };

            if !show_all && !has_unread && title.is_empty() {
                continue;
            }

            chats.push(ChatListing {
                chat_id,
                kind,
                title,
                has_unread,
                unread_count: None,
                active_members: Some(active_member_count),
                last_log_id: Some(last_log_id),
                last_seen_log_id: Some(last_seen),
            });
        }
    }

    Ok(chats)
}

pub fn cmd_loco_chats(
    show_all: bool,
    unread: bool,
    search: Option<String>,
    chat_type: Option<String>,
    json: bool,
) -> Result<()> {
    let creds = get_creds()?;

    let rt = Runtime::new()?;
    rt.block_on(async {
        let mut client = loco::client::LocoClient::new(creds);
        let login_data = loco_connect_with_auto_refresh(&mut client).await?;
        let mut chats =
            fetch_loco_chat_listings_with_client(&mut client, &login_data, show_all).await?;

        if unread {
            chats.retain(|chat| chat.has_unread);
        }

        if let Some(ref query) = search {
            let q = query.to_lowercase();
            chats.retain(|chat| chat.title.to_lowercase().contains(&q));
        }

        if let Some(ref t) = chat_type {
            let lowered = t.to_lowercase();
            let expected = match lowered.as_str() {
                "dm" => "DirectChat".to_string(),
                "group" => "MultiChat".to_string(),
                "memo" => "MemoChat".to_string(),
                "open" => "OpenMultiChat".to_string(),
                "opendm" => "OpenDirectChat".to_string(),
                other => other.to_string(),
            };
            chats.retain(|chat| chat.kind == expected);
        }

        if json {
            println!("{}", serde_json::to_string_pretty(&chats)?);
            return Ok(());
        }

        let rows = chats
            .iter()
            .map(|chat| {
                vec![
                    type_label(&chat.kind).to_string(),
                    chat.title.clone(),
                    if chat.has_unread {
                        "*".to_string()
                    } else {
                        String::new()
                    },
                    chat.chat_id.to_string(),
                ]
            })
            .collect::<Vec<_>>();

        print_section_title(&format!("Chats ({})", rows.len()));
        print_table(&["Type", "Name", "Unread", "Chat ID"], rows);

        Ok(())
    })
}
