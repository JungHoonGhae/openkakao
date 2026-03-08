#!/bin/sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
BIN="${ROOT}/openkakao-rs/target/debug/openkakao-rs"
UID_VALUE="$(id -u)"
LABEL="com.openkakao.watch-smoke"
TMP_DIR="$(mktemp -d)"
PLIST="${TMP_DIR}/${LABEL}.plist"
STDOUT_LOG="${TMP_DIR}/stdout.log"
STDERR_LOG="${TMP_DIR}/stderr.log"
MODE="${OPENKAKAO_LAUNCHD_MODE:-auto}"

cleanup() {
  launchctl bootout "gui/${UID_VALUE}" "${PLIST}" >/dev/null 2>&1 || true
  rm -rf "${TMP_DIR}"
}

trap cleanup EXIT INT TERM

cargo build --manifest-path "${ROOT}/openkakao-rs/Cargo.toml" >/dev/null

if [ "${MODE}" = "auto" ]; then
  if "${BIN}" auth >/dev/null 2>&1; then
    MODE="watch"
  else
    MODE="watch-cache"
  fi
fi

if [ "${MODE}" = "watch" ]; then
  PROGRAM_BLOCK='
    <string>'"${BIN}"'</string>
    <string>watch</string>
    <string>--max-reconnect</string>
    <string>2</string>'
else
  PROGRAM_BLOCK='
    <string>'"${BIN}"'</string>
    <string>watch-cache</string>
    <string>--interval</string>
    <string>15</string>'
fi

cat > "${PLIST}" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>${LABEL}</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>WorkingDirectory</key>
  <string>${ROOT}</string>
  <key>StandardOutPath</key>
  <string>${STDOUT_LOG}</string>
  <key>StandardErrorPath</key>
  <string>${STDERR_LOG}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
  </dict>
  <key>ProgramArguments</key>
  <array>${PROGRAM_BLOCK}
  </array>
</dict>
</plist>
EOF

launchctl bootout "gui/${UID_VALUE}" "${PLIST}" >/dev/null 2>&1 || true
launchctl bootstrap "gui/${UID_VALUE}" "${PLIST}"
launchctl kickstart -k "gui/${UID_VALUE}/${LABEL}"
sleep 8

SERVICE_PRINT="$(launchctl print "gui/${UID_VALUE}/${LABEL}" 2>/dev/null || true)"
echo "mode=${MODE}"
echo "${SERVICE_PRINT}"
echo "---stderr---"
tail -n 40 "${STDERR_LOG}" 2>/dev/null || true

if printf '%s' "${SERVICE_PRINT}" | rg -q "state = running"; then
  echo "launchd smoke validation passed"
else
  echo "launchd smoke validation failed" >&2
  exit 1
fi
