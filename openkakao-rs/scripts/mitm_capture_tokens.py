"""
mitmdump addon to capture KakaoTalk authentication tokens.

Usage:
    mitmdump -s scripts/mitm_capture_tokens.py -p 8080

Then configure macOS system proxy to localhost:8080, restart KakaoTalk.
Captured tokens are saved to /tmp/kakao_tokens.json.
"""

import json
import os
from datetime import datetime

OUTPUT = "/tmp/kakao_tokens.json"

INTERESTING_PATHS = [
    "/mac/account/login.json",
    "/mac/account/renew_token.json",
    "/mac/account/oauth2_token.json",
    "/mac/account/more_settings.json",
]


def response(flow):
    url = flow.request.pretty_url
    for path in INTERESTING_PATHS:
        if path in url:
            break
    else:
        return

    entry = {
        "timestamp": datetime.now().isoformat(),
        "url": url,
        "method": flow.request.method,
        "request_headers": dict(flow.request.headers),
        "response_status": flow.response.status_code,
    }

    # Capture request body (POST params)
    if flow.request.content:
        try:
            entry["request_body"] = flow.request.content.decode("utf-8")
        except UnicodeDecodeError:
            entry["request_body"] = flow.request.content.hex()

    # Capture response body
    if flow.response.content:
        try:
            body = flow.response.content.decode("utf-8")
            try:
                entry["response_json"] = json.loads(body)
            except json.JSONDecodeError:
                entry["response_body"] = body
        except UnicodeDecodeError:
            entry["response_body_hex"] = flow.response.content.hex()[:500]

    # Load existing captures
    captures = []
    if os.path.exists(OUTPUT):
        try:
            with open(OUTPUT) as f:
                captures = json.load(f)
        except (json.JSONDecodeError, IOError):
            captures = []

    captures.append(entry)

    with open(OUTPUT, "w") as f:
        json.dump(captures, f, indent=2, ensure_ascii=False)

    # Print summary
    status = entry.get("response_json", {}).get("status", "?")
    print(f"[CAPTURED] {url} → status={status}")
    if "response_json" in entry:
        resp = entry["response_json"]
        if "access_token" in resp:
            print(f"  access_token: {resp['access_token'][:40]}...")
        if "refresh_token" in resp:
            print(f"  refresh_token: {resp['refresh_token'][:40]}...")
