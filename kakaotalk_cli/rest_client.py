"""KakaoTalk REST API client.

The Mac KakaoTalk app uses both LOCO (TCP binary protocol) and REST APIs.
REST endpoints on katalk.kakao.com handle account settings, profiles, and friends.
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
        """Get own profile and account info."""
        r = self._request("POST", f"{BASE_URL}/mac/account/more_settings.json",
                          "since=0&locale_country=KR")
        profile = r.get("profile", {})
        return MyProfile(
            nickname=profile.get("nickname", ""),
            status_message=profile.get("statusMessage", ""),
            account_id=r.get("accountId", 0),
            email=r.get("emailAddress", ""),
            user_id=self.creds.user_id,
        )

    def verify_token(self) -> bool:
        """Check if the current token is valid."""
        r = self._request("POST", f"{BASE_URL}/mac/account/more_settings.json",
                          "since=0&locale_country=KR")
        return r.get("status") == 0

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

    def get_settings(self) -> dict[str, Any]:
        """Get full account settings."""
        return self._request("POST", f"{BASE_URL}/mac/account/more_settings.json",
                             "since=0&locale_country=KR")
