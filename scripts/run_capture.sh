#!/bin/bash
# One-step KakaoTalk token capture
#
# Prerequisites:
# 1. mitmproxy CA cert must be trusted in Keychain Access:
#    - Open: open ~/.mitmproxy/mitmproxy-ca-cert.pem
#    - Double-click the cert in Keychain Access
#    - Set "Secure Sockets Layer (SSL)" to "Always Trust"
#
# This script:
# 1. Sets macOS Wi-Fi proxy to mitmproxy
# 2. Starts mitmdump capturing kakao traffic
# 3. Kills and restarts KakaoTalk
# 4. Saves captured tokens to /tmp/kakao_captured_tokens.json
# 5. Restores proxy settings on exit

set -e

MITM_PORT=9090
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ADDON="$SCRIPT_DIR/mitm_kakao.py"

# Detect active network service
NETWORK_SERVICE=$(networksetup -listallnetworkservices | grep -E "Wi-Fi|Ethernet" | head -1)
if [ -z "$NETWORK_SERVICE" ]; then
    echo "Error: No Wi-Fi or Ethernet service found"
    exit 1
fi
echo "[*] Network service: $NETWORK_SERVICE"

cleanup() {
    echo ""
    echo "[*] Restoring proxy settings..."
    networksetup -setwebproxystate "$NETWORK_SERVICE" off 2>/dev/null || true
    networksetup -setsecurewebproxystate "$NETWORK_SERVICE" off 2>/dev/null || true
    echo "[*] Proxy disabled"

    if [ -f /tmp/kakao_captured_tokens.json ]; then
        echo ""
        echo "=== Captured Tokens ==="
        cat /tmp/kakao_captured_tokens.json
    fi
}

trap cleanup EXIT INT TERM

echo ""
echo "=== KakaoTalk Token Capture ==="
echo ""
echo "Step 1: Checking mitmproxy CA trust..."

# Check if cert is trusted
if security verify-cert -c ~/.mitmproxy/mitmproxy-ca-cert.pem 2>/dev/null; then
    echo "  CA cert is trusted."
else
    echo "  CA cert is NOT yet trusted!"
    echo "  Opening cert for manual trust..."
    open ~/.mitmproxy/mitmproxy-ca-cert.pem
    echo ""
    echo "  In Keychain Access:"
    echo "  1. Double-click 'mitmproxy' certificate"
    echo "  2. Expand 'Trust' section"
    echo "  3. Set 'Secure Sockets Layer (SSL)' to 'Always Trust'"
    echo "  4. Close and enter password"
    echo ""
    read -p "  Press Enter after trusting the certificate..."
fi

echo ""
echo "Step 2: Setting system proxy to 127.0.0.1:${MITM_PORT}..."
networksetup -setwebproxy "$NETWORK_SERVICE" 127.0.0.1 "$MITM_PORT"
networksetup -setsecurewebproxy "$NETWORK_SERVICE" 127.0.0.1 "$MITM_PORT"
echo "  Proxy set."

echo ""
echo "Step 3: Starting mitmdump..."
echo ""

# Start mitmdump in background
mitmdump \
    --listen-port "$MITM_PORT" \
    --ssl-insecure \
    --set console_eventlog_verbosity=info \
    -s "$ADDON" &
MITM_PID=$!
sleep 2

echo ""
echo "Step 4: Restarting KakaoTalk..."
killall KakaoTalk 2>/dev/null || true
sleep 2
open -a KakaoTalk
echo "  KakaoTalk restarted. Waiting for login traffic..."
echo ""
echo "  Press Ctrl+C when you see tokens captured."
echo ""

# Wait for mitmdump
wait $MITM_PID 2>/dev/null || true
