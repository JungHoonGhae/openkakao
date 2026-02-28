use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

use crate::model::{json_i64, json_string, ChatMember, ChatMessage, ChatRoom, Friend, KakaoCredentials, MyProfile};

const BASE_URL: &str = "https://katalk.kakao.com";
const PILSNER_URL: &str = "https://talk-pilsner.kakao.com";

pub struct KakaoRestClient {
    creds: KakaoCredentials,
    client: Client,
}

impl KakaoRestClient {
    pub fn new(creds: KakaoCredentials) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { creds, client })
    }

    pub fn verify_token(&self) -> Result<bool> {
        let r = self.request_raw(
            "POST",
            &format!("{BASE_URL}/mac/account/more_settings.json"),
            Some("since=0&locale_country=KR"),
        )?;
        Ok(json_i64(&r, "status") == 0)
    }

    pub fn get_my_profile(&self) -> Result<MyProfile> {
        let profile = self.request(
            "POST",
            &format!("{BASE_URL}/mac/profile3/me.json"),
            Some("since=0"),
        )?;
        let settings = self.request(
            "POST",
            &format!("{BASE_URL}/mac/account/more_settings.json"),
            Some("since=0&locale_country=KR"),
        )?;

        let p = profile.get("profile").cloned().unwrap_or(Value::Null);

        Ok(MyProfile {
            nickname: json_string(&p, "nickname"),
            status_message: json_string(&p, "statusMessage"),
            account_id: json_i64(&settings, "accountId"),
            email: json_string(&settings, "emailAddress"),
            user_id: {
                let id = json_i64(&p, "userId");
                if id == 0 { self.creds.user_id } else { id }
            },
            profile_image_url: json_string(&p, "fullProfileImageUrl"),
        })
    }

    pub fn get_friends(&self) -> Result<Vec<Friend>> {
        let r = self.request(
            "POST",
            &format!("{BASE_URL}/mac/friends/update.json"),
            Some("since=0"),
        )?;

        let mut out = Vec::new();
        if let Some(arr) = r.get("friends").and_then(Value::as_array) {
            for item in arr {
                out.push(Friend::from_json(item));
            }
        } else if let Some(arr) = r.get("added").and_then(Value::as_array) {
            for item in arr {
                out.push(Friend::from_json(item));
            }
        }

        Ok(out)
    }

    pub fn get_chats(&self, cursor: Option<i64>) -> Result<(Vec<ChatRoom>, Option<i64>)> {
        let url = if let Some(c) = cursor {
            format!("{PILSNER_URL}/messaging/chats?cursor={c}")
        } else {
            format!("{PILSNER_URL}/messaging/chats")
        };

        let r = self.request("GET", &url, None)?;
        let mut rooms = Vec::new();

        if let Some(chats) = r.get("chats").and_then(Value::as_array) {
            for chat in chats {
                rooms.push(ChatRoom::from_json(chat));
            }
        }

        let next_cursor = if r.get("last").and_then(Value::as_bool).unwrap_or(false) {
            None
        } else {
            let n = json_i64(&r, "nextCursor");
            if n == 0 { None } else { Some(n) }
        };

        Ok((rooms, next_cursor))
    }

    pub fn get_all_chats(&self) -> Result<Vec<ChatRoom>> {
        let mut all = Vec::new();
        let mut cursor: Option<i64> = None;

        loop {
            let (rooms, next_cursor) = self.get_chats(cursor)?;
            all.extend(rooms);
            if next_cursor.is_none() {
                break;
            }
            cursor = next_cursor;
        }

        Ok(all)
    }

    pub fn get_chat_members(&self, chat_id: i64) -> Result<Vec<ChatMember>> {
        let r = self.request(
            "GET",
            &format!("{PILSNER_URL}/messaging/chats/{chat_id}/members"),
            None,
        )?;

        let mut members = Vec::new();
        if let Some(arr) = r.get("members").and_then(Value::as_array) {
            for member in arr {
                members.push(ChatMember::from_json(member));
            }
        }

        Ok(members)
    }

    /// Get one page of messages. Returns (messages, next_cursor).
    /// next_cursor=0 means no more pages.
    ///
    /// NOTE: `fromLogId` and `sinceMessageId` do NOT work for pagination.
    /// Only `?cursor=` works.
    pub fn get_messages(&self, chat_id: i64, cursor: Option<i64>) -> Result<(Vec<ChatMessage>, i64)> {
        let url = if let Some(c) = cursor {
            format!("{PILSNER_URL}/messaging/chats/{chat_id}/messages?cursor={c}")
        } else {
            format!("{PILSNER_URL}/messaging/chats/{chat_id}/messages")
        };

        let r = self.request("GET", &url, None)?;

        let mut messages = Vec::new();
        if let Some(arr) = r.get("chatLogs").and_then(Value::as_array) {
            for msg in arr {
                messages.push(ChatMessage::from_json(msg));
            }
        }

        let next_cursor = r.get("nextCursor").and_then(Value::as_i64).unwrap_or(0);
        Ok((messages, next_cursor))
    }

    /// Fetch all available messages using cursor pagination.
    ///
    /// The pilsner server only caches messages for chats recently opened
    /// in the KakaoTalk Mac app. Most chats will return empty results.
    pub fn get_all_messages(&self, chat_id: i64, max_pages: usize) -> Result<Vec<ChatMessage>> {
        let mut all = Vec::new();
        let mut cursor: Option<i64> = None;

        for _ in 0..max_pages {
            let (messages, next_cursor) = self.get_messages(chat_id, cursor)?;
            if messages.is_empty() {
                break;
            }
            all.extend(messages);
            if next_cursor == 0 {
                break;
            }
            cursor = Some(next_cursor);
        }

        all.sort_by_key(|m| m.log_id);
        all.dedup_by_key(|m| m.log_id);
        Ok(all)
    }

    pub fn get_settings(&self) -> Result<Value> {
        self.request(
            "POST",
            &format!("{BASE_URL}/mac/account/more_settings.json"),
            Some("since=0&locale_country=KR"),
        )
    }

    pub fn get_scrap_preview(&self, url: &str) -> Result<Value> {
        let encoded = urlencoding::encode(url);
        let body = format!("url={encoded}");
        self.request(
            "POST",
            &format!("{BASE_URL}/mac/scrap/preview.json"),
            Some(&body),
        )
    }

    fn request(&self, method: &str, url: &str, body: Option<&str>) -> Result<Value> {
        let parsed = self.request_raw(method, url, body)?;
        if let Some(status) = parsed.get("status").and_then(Value::as_i64) {
            if status != 0 {
                let message = parsed
                    .get("message")
                    .or_else(|| parsed.get("msg"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let details = if message.is_empty() {
                    String::new()
                } else {
                    format!(" ({message})")
                };
                return Err(anyhow!(
                    "Kakao API returned status {status}{details} for {method} {url}"
                ));
            }
        }
        Ok(parsed)
    }

    fn request_raw(&self, method: &str, url: &str, body: Option<&str>) -> Result<Value> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("ko"));

        let auth = HeaderValue::from_str(&self.creds.oauth_token)
            .context("Invalid Authorization header")?;
        headers.insert(AUTHORIZATION, auth);

        let a_header = if self.creds.a_header.is_empty() {
            format!("mac/{}/ko", self.creds.app_version)
        } else {
            self.creds.a_header.clone()
        };
        headers.insert("A", HeaderValue::from_str(&a_header).context("Invalid A header")?);

        let user_agent = if self.creds.user_agent.is_empty() {
            format!("KT/{} Mc/26.1.0 ko", self.creds.app_version)
        } else {
            self.creds.user_agent.clone()
        };
        headers.insert("User-Agent", HeaderValue::from_str(&user_agent).context("Invalid User-Agent header")?);

        let request = match method {
            "GET" => self.client.get(url).headers(headers),
            "POST" => self
                .client
                .post(url)
                .headers(headers)
                .body(body.unwrap_or_default().to_string()),
            _ => return Err(anyhow!("Unsupported HTTP method: {method}")),
        };

        let response = request.send().with_context(|| format!("HTTP request failed: {method} {url}"))?;
        let status = response.status();
        let text = response.text().context("Failed to read HTTP response body")?;

        let parsed: Value = serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse JSON response (HTTP {status}): {}", text.chars().take(200).collect::<String>()))?;

        Ok(parsed)
    }
}
