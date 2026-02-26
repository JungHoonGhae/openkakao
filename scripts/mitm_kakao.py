"""mitmproxy addon to intercept KakaoTalk traffic and extract LOCO tokens.

Usage:
    mitmdump -s scripts/mitm_kakao.py --set stream_large_bodies=0

This captures both:
1. HTTPS REST API calls (katalk.kakao.com) - Authorization headers
2. Raw TCP LOCO packets (if transparent mode) - oauthToken from LOGINLIST
"""

import json
import struct
import sys
from datetime import datetime
from pathlib import Path

from mitmproxy import http, tcp, ctx

OUTPUT_FILE = Path("/tmp/kakao_captured_tokens.json")


class KakaoInterceptor:
    def __init__(self):
        self.captured = {
            "timestamp": "",
            "rest_tokens": {},
            "loco_tokens": {},
            "headers": {},
        }

    def _save(self):
        self.captured["timestamp"] = datetime.now().isoformat()
        with open(OUTPUT_FILE, "w") as f:
            json.dump(self.captured, f, indent=2, default=str)
        ctx.log.info(f"[kakao] Saved to {OUTPUT_FILE}")

    # ── HTTPS interception ──────────────────────────────────────────────

    def request(self, flow: http.HTTPFlow):
        """Intercept HTTP/HTTPS requests to KakaoTalk servers."""
        url = flow.request.pretty_url
        host = flow.request.pretty_host

        if "kakao" not in host:
            return

        ctx.log.info(f"[kakao] {flow.request.method} {url}")

        # Capture all headers
        headers = dict(flow.request.headers)
        self.captured["headers"][url] = headers

        # Extract auth tokens from headers
        if "Authorization" in headers:
            token = headers["Authorization"]
            self.captured["rest_tokens"]["authorization"] = token
            ctx.log.info(f"[kakao] Authorization: {token[:50]}...")

        if "talk-user-id" in headers:
            uid = headers["talk-user-id"]
            self.captured["rest_tokens"]["user_id"] = uid
            ctx.log.info(f"[kakao] User ID: {uid}")

        # Capture request body for login endpoints
        if "login" in url.lower() or "account" in url.lower():
            body = flow.request.get_text()
            if body:
                ctx.log.info(f"[kakao] Login body: {body[:200]}")
                self.captured["rest_tokens"]["login_body"] = body

        self._save()

    def response(self, flow: http.HTTPFlow):
        """Intercept HTTP/HTTPS responses from KakaoTalk servers."""
        url = flow.request.pretty_url
        host = flow.request.pretty_host

        if "kakao" not in host:
            return

        # Capture login response (contains session tokens)
        if "login" in url.lower() or "account" in url.lower():
            body = flow.response.get_text()
            if body:
                ctx.log.info(f"[kakao] Login response ({flow.response.status_code}): {body[:300]}")
                self.captured["rest_tokens"]["login_response"] = body
                try:
                    data = json.loads(body)
                    for key in ("oauthToken", "accessToken", "token", "access_token", "userId"):
                        if key in data:
                            self.captured["loco_tokens"][key] = data[key]
                            ctx.log.info(f"[kakao] Found {key}: {str(data[key])[:50]}")
                except json.JSONDecodeError:
                    pass

            self._save()

    # ── Raw TCP / LOCO interception (transparent mode only) ─────────

    def tcp_message(self, flow: tcp.TCPFlow):
        """Intercept raw TCP messages (LOCO protocol)."""
        msg = flow.messages[-1]
        data = msg.content

        if len(data) < 22:
            return

        # Try to parse as LOCO packet
        try:
            packet_id, status = struct.unpack_from("<Ih", data, 0)
            method_raw = data[6:17]
            method = method_raw.rstrip(b"\x00").decode("ascii", errors="ignore")
            body_type = data[17]
            body_length = struct.unpack_from("<I", data, 18)[0]

            if not method.isalpha():
                return

            direction = ">>>" if msg.from_client else "<<<"
            ctx.log.info(f"[loco] {direction} {method} (id={packet_id}, status={status}, body={body_length})")

            # Try BSON decode for interesting commands
            if method in ("LOGINLIST", "CHECKIN", "GETCONF", "LOGIN"):
                body_bytes = data[22:22 + body_length]
                if body_bytes:
                    try:
                        import bson
                        body = bson.BSON(body_bytes).decode()

                        ctx.log.info(f"[loco] {method} body keys: {list(body.keys())}")

                        # Extract tokens
                        for key in ("oauthToken", "accessToken", "userId", "duuid", "host", "port"):
                            if key in body:
                                value = body[key]
                                self.captured["loco_tokens"][key] = value
                                ctx.log.info(f"[loco] {key}: {str(value)[:80]}")

                        self._save()
                    except Exception as e:
                        ctx.log.info(f"[loco] BSON decode error: {e}")

            # Also capture MSG commands for chat data
            elif method == "MSG" and not msg.from_client:
                body_bytes = data[22:22 + body_length]
                if body_bytes:
                    try:
                        import bson
                        body = bson.BSON(body_bytes).decode()
                        chat_id = body.get("chatId", "?")
                        author = body.get("authorNickname", body.get("authorId", "?"))
                        message = body.get("message", "")[:50]
                        ctx.log.info(f"[loco] MSG [{chat_id}] {author}: {message}")
                    except Exception:
                        pass

        except Exception:
            pass


addons = [KakaoInterceptor()]
