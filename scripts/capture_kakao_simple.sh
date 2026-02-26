#!/bin/bash
# Simple KakaoTalk token capture using mitmproxy as HTTP proxy
#
# This approach captures the HTTPS REST API calls that KakaoTalk makes
# to katalk.kakao.com (login.json, more_settings.json, etc.)
#
# Steps:
# 1. Start mitmproxy as regular HTTP proxy on port 9090
# 2. Set macOS system proxy to 127.0.0.1:9090
# 3. Install mitmproxy CA cert if not done
# 4. Restart KakaoTalk → captures login.json with tokens
# 5. Reset proxy when done

set -e

MITM_PORT=9090
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ADDON="$SCRIPT_DIR/mitm_kakao.py"
NETWORK_SERVICE="Wi-Fi"  # Change if using Ethernet

cleanup() {
    echo ""
    echo "[*] Resetting system proxy..."
    networksetup -setwebproxystate "$NETWORK_SERVICE" off 2>/dev/null || true
    networksetup -setsecurewebproxystate "$NETWORK_SERVICE" off 2>/dev/null || true
    echo "[*] Done. Check /tmp/kakao_captured_tokens.json for captured tokens."
}

trap cleanup EXIT INT TERM

echo "=== KakaoTalk Token Capture (Simple Mode) ==="
echo ""

# Check if mitmproxy CA is installed
if ! security find-certificate -a -p ~/Library/Keychains/login.keychain-db 2>/dev/null | grep -q "mitmproxy"; then
    echo "[!] mitmproxy CA certificate may not be installed."
    echo "    Run: open ~/.mitmproxy/mitmproxy-ca-cert.pem"
    echo "    Then trust it in Keychain Access (Always Trust)."
    echo ""
fi

echo "[*] Setting system proxy to 127.0.0.1:${MITM_PORT}..."
networksetup -setwebproxy "$NETWORK_SERVICE" 127.0.0.1 "$MITM_PORT"
networksetup -setsecurewebproxy "$NETWORK_SERVICE" 127.0.0.1 "$MITM_PORT"

echo "[*] Starting mitmproxy on port ${MITM_PORT}..."
echo ""
echo ">>> Now restart KakaoTalk to capture login tokens <<<"
echo ">>> Press Ctrl+C when done <<<"
echo ""

mitmdump \
    --listen-port "$MITM_PORT" \
    --ssl-insecure \
    --set stream_large_bodies=0 \
    -s "$ADDON" \
    2>&1
