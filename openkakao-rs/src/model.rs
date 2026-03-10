use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KakaoCredentials {
    pub oauth_token: String,
    pub user_id: i64,
    pub device_uuid: String,
    #[serde(default = "default_device_name")]
    pub device_name: String,
    pub app_version: String,
    pub user_agent: String,
    pub a_header: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

fn default_device_name() -> String {
    "openkakao-rs".to_string()
}

impl KakaoCredentials {
    pub fn new(
        oauth_token: String,
        user_id: i64,
        device_uuid: String,
        app_version: String,
        user_agent: String,
        a_header: String,
    ) -> Self {
        Self {
            oauth_token,
            user_id,
            device_uuid,
            device_name: "openkakao-rs".to_string(),
            app_version,
            user_agent,
            a_header,
            refresh_token: None,
            email: None,
        }
    }
}

impl Drop for KakaoCredentials {
    fn drop(&mut self) {
        self.oauth_token.zeroize();
        if let Some(ref mut rt) = self.refresh_token {
            rt.zeroize();
        }
    }
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct MyProfile {
    pub nickname: String,
    pub status_message: String,
    pub account_id: i64,
    pub email: String,
    pub user_id: i64,
    pub profile_image_url: String,
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub log_id: i64,
    pub author_id: i64,
    pub message_type: i64,
    pub message: String,
    pub attachment: String,
    pub send_at: i64,
}

impl ChatMessage {
    pub fn from_json(v: &Value) -> Self {
        Self {
            log_id: json_i64(v, "logId"),
            author_id: json_i64(v, "authorId"),
            message_type: json_i64(v, "type"),
            message: json_string(v, "message"),
            attachment: json_string(v, "attachment"),
            send_at: json_i64(v, "sendAt"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_json_i64_integer() {
        let v = json!({"n": 42});
        assert_eq!(json_i64(&v, "n"), 42);
    }

    #[test]
    fn test_json_i64_string() {
        let v = json!({"n": "123"});
        assert_eq!(json_i64(&v, "n"), 123);
    }

    #[test]
    fn test_json_i64_missing() {
        let v = json!({});
        assert_eq!(json_i64(&v, "n"), 0);
    }

    #[test]
    fn test_json_string_present() {
        let v = json!({"name": "hello"});
        assert_eq!(json_string(&v, "name"), "hello");
    }

    #[test]
    fn test_json_string_missing() {
        let v = json!({});
        assert_eq!(json_string(&v, "name"), "");
    }

    #[test]
    fn test_friend_display_name_uses_friend_nickname() {
        let f = Friend {
            user_id: 1,
            nickname: "Original".to_string(),
            friend_nickname: "Custom".to_string(),
            phone_number: String::new(),
            status_message: String::new(),
            favorite: false,
            hidden: false,
        };
        assert_eq!(f.display_name(), "Custom");
    }

    #[test]
    fn test_friend_display_name_falls_back_to_nickname() {
        let f = Friend {
            user_id: 1,
            nickname: "Original".to_string(),
            friend_nickname: String::new(),
            phone_number: String::new(),
            status_message: String::new(),
            favorite: false,
            hidden: false,
        };
        assert_eq!(f.display_name(), "Original");
    }

    #[test]
    fn test_friend_from_json() {
        let v = json!({
            "userId": 12345,
            "nickName": "Nick",
            "friendNickName": "Friend",
            "phoneNumber": "010-1234-5678",
            "statusMessage": "Hello",
            "favorite": true,
            "hidden": false,
        });
        let f = Friend::from_json(&v);
        assert_eq!(f.user_id, 12345);
        assert_eq!(f.nickname, "Nick");
        assert_eq!(f.friend_nickname, "Friend");
        assert!(f.favorite);
        assert!(!f.hidden);
    }

    #[test]
    fn test_chatroom_display_title_with_title() {
        let room = ChatRoom {
            chat_id: 1,
            kind: "DirectChat".to_string(),
            title: "My Chat".to_string(),
            unread_count: 0,
            display_members: vec![],
        };
        assert_eq!(room.display_title(), "My Chat");
    }

    #[test]
    fn test_chatroom_display_title_from_members() {
        let room = ChatRoom {
            chat_id: 1,
            kind: "DirectChat".to_string(),
            title: String::new(),
            unread_count: 0,
            display_members: vec![
                json!({"friendNickName": "Alice", "nickName": "A"}),
                json!({"friendNickName": "", "nickName": "Bob"}),
            ],
        };
        assert_eq!(room.display_title(), "Alice, Bob");
    }

    #[test]
    fn test_chatroom_display_title_empty() {
        let room = ChatRoom {
            chat_id: 1,
            kind: "DirectChat".to_string(),
            title: String::new(),
            unread_count: 0,
            display_members: vec![],
        };
        assert_eq!(room.display_title(), "(empty)");
    }

    #[test]
    fn test_chatroom_from_json() {
        let v = json!({
            "chatId": 999,
            "type": "MultiChat",
            "title": "Group",
            "unreadCount": 5,
            "displayMembers": [],
        });
        let room = ChatRoom::from_json(&v);
        assert_eq!(room.chat_id, 999);
        assert_eq!(room.kind, "MultiChat");
        assert_eq!(room.unread_count, 5);
    }

    #[test]
    fn test_chat_message_from_json() {
        let v = json!({
            "logId": 100,
            "authorId": 200,
            "type": 1,
            "message": "Hello",
            "attachment": "",
            "sendAt": 1700000000,
        });
        let msg = ChatMessage::from_json(&v);
        assert_eq!(msg.log_id, 100);
        assert_eq!(msg.author_id, 200);
        assert_eq!(msg.message, "Hello");
        assert_eq!(msg.send_at, 1700000000);
    }

    #[test]
    fn test_chat_member_display_name() {
        let m = ChatMember {
            user_id: 1,
            nickname: "Nick".to_string(),
            friend_nickname: "Custom".to_string(),
            country_iso: "KR".to_string(),
        };
        assert_eq!(m.display_name(), "Custom");
    }

    #[test]
    fn test_credentials_serialize_roundtrip() {
        let creds = KakaoCredentials::new(
            "token123".to_string(),
            42,
            "uuid".to_string(),
            "3.7.0".to_string(),
            "agent".to_string(),
            "mac/3.7.0/ko".to_string(),
        );
        let json = serde_json::to_string(&creds).unwrap();
        let parsed: KakaoCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.oauth_token, "token123");
        assert_eq!(parsed.user_id, 42);
        assert_eq!(parsed.device_name, "openkakao-rs");
    }
}
