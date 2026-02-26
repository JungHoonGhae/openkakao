use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KakaoCredentials {
    pub oauth_token: String,
    pub user_id: i64,
    pub device_uuid: String,
    pub device_name: String,
    pub app_version: String,
    pub user_agent: String,
    pub a_header: String,
}

impl KakaoCredentials {
    pub fn new(oauth_token: String, user_id: i64, device_uuid: String, app_version: String, user_agent: String, a_header: String) -> Self {
        Self {
            oauth_token,
            user_id,
            device_uuid,
            device_name: "openkakao-rs".to_string(),
            app_version,
            user_agent,
            a_header,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Friend {
    pub user_id: i64,
    pub nickname: String,
    pub friend_nickname: String,
    pub phone_number: String,
    pub status_message: String,
    pub favorite: bool,
    pub hidden: bool,
}

impl Friend {
    pub fn display_name(&self) -> String {
        if self.friend_nickname.is_empty() {
            self.nickname.clone()
        } else {
            self.friend_nickname.clone()
        }
    }

    pub fn from_json(v: &Value) -> Self {
        Self {
            user_id: json_i64(v, "userId"),
            nickname: json_string(v, "nickName"),
            friend_nickname: json_string(v, "friendNickName"),
            phone_number: json_string(v, "phoneNumber"),
            status_message: json_string(v, "statusMessage"),
            favorite: v.get("favorite").and_then(Value::as_bool).unwrap_or(false),
            hidden: v.get("hidden").and_then(Value::as_bool).unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MyProfile {
    pub nickname: String,
    pub status_message: String,
    pub account_id: i64,
    pub email: String,
    pub user_id: i64,
    pub profile_image_url: String,
}

#[derive(Debug, Clone)]
pub struct ChatRoom {
    pub chat_id: i64,
    pub kind: String,
    pub title: String,
    pub unread_count: i64,
    pub display_members: Vec<Value>,
}

impl ChatRoom {
    pub fn display_title(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }

        let mut names = Vec::new();
        for member in &self.display_members {
            if let Some(name) = member.get("friendNickName").and_then(Value::as_str) {
                if !name.is_empty() {
                    names.push(name.to_string());
                    continue;
                }
            }
            if let Some(name) = member.get("nickName").and_then(Value::as_str) {
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }

        if names.is_empty() {
            "(empty)".to_string()
        } else {
            names.join(", ")
        }
    }

    pub fn from_json(v: &Value) -> Self {
        let display_members = v
            .get("displayMembers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Self {
            chat_id: json_i64(v, "chatId"),
            kind: json_string(v, "type"),
            title: json_string(v, "title"),
            unread_count: json_i64(v, "unreadCount"),
            display_members,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub log_id: i64,
    pub author_id: i64,
    pub message_type: i64,
    pub message: String,
    pub send_at: i64,
}

impl ChatMessage {
    pub fn from_json(v: &Value) -> Self {
        Self {
            log_id: json_i64(v, "logId"),
            author_id: json_i64(v, "authorId"),
            message_type: json_i64(v, "type"),
            message: json_string(v, "message"),
            send_at: json_i64(v, "sendAt"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMember {
    pub user_id: i64,
    pub nickname: String,
    pub friend_nickname: String,
    pub country_iso: String,
}

impl ChatMember {
    pub fn display_name(&self) -> String {
        if self.friend_nickname.is_empty() {
            self.nickname.clone()
        } else {
            self.friend_nickname.clone()
        }
    }

    pub fn from_json(v: &Value) -> Self {
        Self {
            user_id: json_i64(v, "userId"),
            nickname: json_string(v, "nickName"),
            friend_nickname: json_string(v, "friendNickName"),
            country_iso: json_string(v, "countryIso"),
        }
    }
}

pub fn json_i64(v: &Value, key: &str) -> i64 {
    if let Some(n) = v.get(key).and_then(Value::as_i64) {
        return n;
    }
    if let Some(n) = v.get(key).and_then(Value::as_u64) {
        return n as i64;
    }
    if let Some(s) = v.get(key).and_then(Value::as_str) {
        return s.parse::<i64>().unwrap_or(0);
    }
    0
}

pub fn json_string(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
