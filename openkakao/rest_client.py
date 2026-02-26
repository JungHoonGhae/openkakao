"""KakaoTalk REST API client.

The Mac KakaoTalk app uses both LOCO (TCP binary protocol) and REST APIs.
- katalk.kakao.com: account settings, profiles, friends (form-urlencoded POST)
- talk-pilsner.kakao.com: chat list, messages, members, emoticons (JSON GET)
"""

import gzip
import json
import ssl
import sys
import urllib.request
import urllib.error
import urllib.parse
from dataclasses import dataclass, field
from typing import Any

from .auth import KakaoCredentials


BASE_URL = "https://katalk.kakao.com"
PILSNER_URL = "https://talk-pilsner.kakao.com"

_SSL_CTX = ssl.create_default_context()


@dataclass
class Friend:
    user_id: int
    nickname: str = ""
    friend_nickname: str = ""
    phone_number: str = ""
    profile_image_url: str = ""
    status_message: str = ""
    account_id: int = 0
    favorite: bool = False
    hidden: bool = False

    @property
    def display_name(self) -> str:
        return self.friend_nickname or self.nickname


@dataclass
class MyProfile:
    nickname: str = ""
    status_message: str = ""
    account_id: int = 0
    email: str = ""
    user_id: int = 0
    profile_image_url: str = ""
    background_image_url: str = ""


@dataclass
class ChatRoom:
    chat_id: int
    type: str = ""
    title: str = ""
    unread_count: int = 0
    last_message_id: int = 0
    last_seen_log_id: int = 0
    display_members: list[dict] = field(default_factory=list)

    @property
    def display_title(self) -> str:
        if self.title:
            return self.title
        names = [
            m.get("friendNickName") or m.get("nickName", "?")
            for m in self.display_members
        ]
        return ", ".join(names) or "(empty)"


@dataclass
class ChatMessage:
    log_id: int
    chat_id: int
    author_id: int
    type: int = 1
    message: str = ""
    attachment: str = ""
    send_at: int = 0


@dataclass
class ChatMember:
    user_id: int
    nickname: str = ""
    friend_nickname: str = ""
    profile_image_url: str = ""
    country_iso: str = ""

    @property
    def display_name(self) -> str:
        return self.friend_nickname or self.nickname


class KakaoRestClient:
    """REST API client for KakaoTalk."""

    def __init__(self, credentials: KakaoCredentials):
        self.creds = credentials

    def _form_headers(self) -> dict[str, str]:
        return {
            "Content-Type": "application/x-www-form-urlencoded",
            "Accept": "application/json",
            "Authorization": self.creds.oauth_token,
            "A": self.creds.a_header or f"mac/{self.creds.app_version}/ko",
            "User-Agent": self.creds.user_agent or f"KT/{self.creds.app_version} Mc/26.1.0 ko",
            "Accept-Language": "ko",
        }

    def _request(
        self,
        method: str,
        url: str,
        body: str = "",
    ) -> dict[str, Any]:
        headers = self._form_headers()
        if method == "GET":
            data = None
        else:
            data = body.encode("utf-8") if body else b""

        req = urllib.request.Request(url, data=data, headers=headers, method=method)

        try:
            with urllib.request.urlopen(req, context=_SSL_CTX, timeout=15) as resp:
                raw = resp.read()
                try:
                    raw = gzip.decompress(raw)
                except Exception:
                    pass
                return json.loads(raw)
        except urllib.error.HTTPError as e:
            raw = e.read()
            try:
                raw = gzip.decompress(raw)
            except Exception:
                pass
            try:
                return json.loads(raw)
            except Exception:
                print(f"[rest] HTTP {e.code}: {raw[:200]}", file=sys.stderr)
                raise
        except urllib.error.URLError as e:
            print(f"[rest] URL Error: {e.reason}", file=sys.stderr)
            raise

    # ── Account / Profile ───────────────────────────────────────────────

    def get_my_profile(self) -> MyProfile:
        """Get own profile and account info via profile3 API."""
        r = self._request("POST", f"{BASE_URL}/mac/profile3/me.json", "since=0")
        p = r.get("profile", {})
        settings = self._request("POST", f"{BASE_URL}/mac/account/more_settings.json",
                                 "since=0&locale_country=KR")
        return MyProfile(
            nickname=p.get("nickname", ""),
            status_message=p.get("statusMessage", ""),
            account_id=settings.get("accountId", 0),
            email=settings.get("emailAddress", ""),
            user_id=p.get("userId", self.creds.user_id),
            profile_image_url=p.get("fullProfileImageUrl", ""),
            background_image_url=p.get("backgroundImageUrl", ""),
        )

    def get_friend_profile(self, user_id: int) -> dict[str, Any]:
        """Get a friend's profile."""
        return self._request("POST", f"{BASE_URL}/mac/profile3/friend.json",
                             f"id={user_id}")

    def verify_token(self) -> bool:
        """Check if the current token is valid."""
        r = self._request("POST", f"{BASE_URL}/mac/account/more_settings.json",
                          "since=0&locale_country=KR")
        return r.get("status") == 0

    def get_profiles(self) -> list[dict]:
        """Get all profile cards (multi-profile)."""
        r = self._request("GET", f"{BASE_URL}/mac/profile/list.json")
        return r.get("profiles", [])

    # ── Friends ─────────────────────────────────────────────────────────

    def get_friends(self) -> list[Friend]:
        """Get full friends list."""
        r = self._request("POST", f"{BASE_URL}/mac/friends/update.json", "since=0")
        raw_friends = r.get("friends", r.get("added", []))
        friends = []
        for f in raw_friends:
            friends.append(Friend(
                user_id=f.get("userId", 0),
                nickname=f.get("nickName", ""),
                friend_nickname=f.get("friendNickName", ""),
                phone_number=f.get("phoneNumber", ""),
                profile_image_url=f.get("profileImageUrl", ""),
                status_message=f.get("statusMessage", ""),
                account_id=f.get("accountId", 0),
                favorite=f.get("favorite", False),
                hidden=f.get("hidden", False),
            ))
        return friends

    def add_favorite(self, user_id: int) -> dict[str, Any]:
        """Add a friend to favorites."""
        return self._request("POST", f"{BASE_URL}/mac/friends/add_favorite.json",
                             f"id={user_id}")

    def remove_favorite(self, user_id: int) -> dict[str, Any]:
        """Remove a friend from favorites."""
        return self._request("POST", f"{BASE_URL}/mac/friends/remove_favorite.json",
                             f"id={user_id}")

    def hide_friend(self, user_id: int) -> dict[str, Any]:
        """Hide a friend."""
        return self._request("POST", f"{BASE_URL}/mac/friends/hide.json",
                             f"id={user_id}")

    def unhide_friend(self, user_id: int) -> dict[str, Any]:
        """Unhide a friend."""
        return self._request("POST", f"{BASE_URL}/mac/friends/unhide.json",
                             f"id={user_id}")

    # ── Settings ───────────────────────────────────────────────────────

    def get_settings(self) -> dict[str, Any]:
        """Get full account settings."""
        return self._request("POST", f"{BASE_URL}/mac/account/more_settings.json",
                             "since=0&locale_country=KR")

    def get_alarm_keywords(self) -> list[str]:
        """Get notification alarm keywords."""
        r = self._request("GET", f"{BASE_URL}/mac/alarm_keywords/list.json")
        return r.get("alarm_keywords", [])

    # ── Chat Rooms (pilsner) ──────────────────────────────────────────

    def get_chats(self, cursor: int | None = None) -> tuple[list[ChatRoom], int | None]:
        """Get chat room list. Returns (rooms, next_cursor)."""
        url = f"{PILSNER_URL}/messaging/chats"
        if cursor is not None:
            url += f"?cursor={cursor}"
        r = self._request("GET", url)
        rooms = []
        for c in r.get("chats", []):
            rooms.append(ChatRoom(
                chat_id=c.get("chatId", 0),
                type=c.get("type", ""),
                title=c.get("title", "") or "",
                unread_count=c.get("unreadCount", 0),
                last_message_id=c.get("lastMessageId", 0),
                last_seen_log_id=c.get("lastSeenLogId", 0),
                display_members=c.get("displayMembers", []),
            ))
        next_cursor = r.get("nextCursor") if not r.get("last") else None
        return rooms, next_cursor

    def get_all_chats(self) -> list[ChatRoom]:
        """Get all chat rooms (handles pagination)."""
        all_rooms = []
        cursor = None
        while True:
            rooms, next_cursor = self.get_chats(cursor)
            all_rooms.extend(rooms)
            if next_cursor is None:
                break
            cursor = next_cursor
        return all_rooms

    def get_chat_members(self, chat_id: int) -> list[ChatMember]:
        """Get members of a chat room."""
        r = self._request("GET", f"{PILSNER_URL}/messaging/chats/{chat_id}/members")
        members = []
        for m in r.get("members", []):
            members.append(ChatMember(
                user_id=m.get("userId", 0),
                nickname=m.get("nickName", ""),
                friend_nickname=m.get("friendNickName", ""),
                profile_image_url=m.get("profileImageUrl", ""),
                country_iso=m.get("countryIso", ""),
            ))
        return members

    # ── Messages (pilsner) ────────────────────────────────────────────

    def get_messages(self, chat_id: int, from_log_id: int | None = None) -> list[ChatMessage]:
        """Get messages from a chat room (newest first, 30 per page)."""
        url = f"{PILSNER_URL}/messaging/chats/{chat_id}/messages"
        if from_log_id is not None:
            url += f"?fromLogId={from_log_id}"
        r = self._request("GET", url)
        messages = []
        for m in r.get("chatLogs", []):
            messages.append(ChatMessage(
                log_id=m.get("logId", 0),
                chat_id=m.get("chatId", chat_id),
                author_id=m.get("authorId", 0),
                type=m.get("type", 1),
                message=m.get("message", ""),
                attachment=m.get("attachment", ""),
                send_at=m.get("sendAt", 0),
            ))
        return messages

    # ── Link Preview ──────────────────────────────────────────────────

    def get_scrap_preview(self, url: str) -> dict[str, Any]:
        """Get link preview (OG tags) for a URL."""
        encoded = urllib.parse.quote(url, safe="")
        return self._request("POST", f"{BASE_URL}/mac/scrap/preview.json",
                             f"url={encoded}")
