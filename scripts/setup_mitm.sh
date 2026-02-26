#!/bin/bash
# Setup mitmproxy transparent proxy for KakaoTalk LOCO traffic capture
# Requires: sudo (for pf rules), mitmproxy installed
#
# This script:
# 1. Enables IP forwarding
# 2. Adds pf (packet filter) rules to redirect KakaoTalk traffic to mitmproxy
# 3. Starts mitmdump with the KakaoTalk addon
# 4. Cleans up on exit

set -e

MITM_PORT=8080
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ADDON="$SCRIPT_DIR/mitm_kakao.py"
PF_ANCHOR="kakao_mitm"

# KakaoTalk server IPs (from booking response)
KAKAO_IPS=(
    "211.183.215.108"  # Current LOCO server
    "211.183.222.24"   # booking-loco.kakao.com
    "211.183.222.6"    # ticket-loco.kakao.com
    "211.183.211.10"   # ticket-loco fallback
    "121.53.93.47"     # ticket-loco fallback
)

cleanup() {
    echo ""
    echo "[*] Cleaning up..."
    # Remove pf rules
    sudo pfctl -a "$PF_ANCHOR" -F all 2>/dev/null || true
    sudo pfctl -d 2>/dev/null || true
    # Disable IP forwarding
    sudo sysctl -w net.inet.ip.forwarding=0 > /dev/null 2>&1 || true
    echo "[*] Cleanup done"
}

trap cleanup EXIT INT TERM

echo "=== KakaoTalk mitmproxy Setup ==="
echo ""
echo "This will:"
echo "  1. Redirect KakaoTalk traffic through mitmproxy (port $MITM_PORT)"
echo "  2. Capture LOCO protocol tokens"
echo "  3. Save tokens to /tmp/kakao_captured_tokens.json"
echo ""
echo "After starting, restart KakaoTalk to capture the login flow."
echo ""
read -p "Press Enter to continue (requires sudo)..."

# Enable IP forwarding
echo "[*] Enabling IP forwarding..."
sudo sysctl -w net.inet.ip.forwarding=1 > /dev/null

# Create pf rules
echo "[*] Setting up packet filter rules..."
PF_RULES=""
for ip in "${KAKAO_IPS[@]}"; do
    PF_RULES="$PF_RULES
rdr pass on lo0 proto tcp from any to $ip -> 127.0.0.1 port $MITM_PORT"
done

echo "$PF_RULES" | sudo pfctl -a "$PF_ANCHOR" -f - 2>/dev/null
sudo pfctl -e 2>/dev/null || true

echo "[*] PF rules active for IPs: ${KAKAO_IPS[*]}"
echo ""

# Start mitmproxy
echo "[*] Starting mitmdump on port $MITM_PORT..."
echo "[*] Restart KakaoTalk now to capture login traffic!"
echo "[*] Press Ctrl+C to stop"
echo ""

mitmdump \
    --mode transparent \
    --listen-port "$MITM_PORT" \
    --tcp-hosts ".*" \
    --ssl-insecure \
    --set stream_large_bodies=0 \
    -s "$ADDON" \
    2>&1
