#!/bin/sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
HOME_DIR="${TMP}/home"
FAKE_BIN_DIR="${TMP}/bin"
STATE_ROOT="${HOME_DIR}/Library/Application Support/openkakao/bujamentor"
LAUNCH_AGENTS_DIR="${HOME_DIR}/Library/LaunchAgents"
LAUNCHCTL_LOG="${TMP}/launchctl.log"
PLUTIL_LOG="${TMP}/plutil.log"
BIN_PATH="${TMP}/openkakao-cli"
HEALTH_BIN_PATH="${TMP}/openkakao-bujamentor-health"
HOOK_PATH="${TMP}/hook.sh"
SYMLINK_HOOK="${TMP}/hook-link.sh"

cleanup() {
  rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

mkdir -p "$HOME_DIR" "$FAKE_BIN_DIR"
: > "$LAUNCHCTL_LOG"
: > "$PLUTIL_LOG"
printf '#!/bin/sh\nexit 0\n' > "$BIN_PATH"
printf '#!/bin/sh\nexit 0\n' > "$HEALTH_BIN_PATH"
printf '#!/bin/sh\nexit 0\n' > "$HOOK_PATH"
chmod 700 "$BIN_PATH" "$HEALTH_BIN_PATH" "$HOOK_PATH"
ln -s "$HOOK_PATH" "$SYMLINK_HOOK"

cat > "${FAKE_BIN_DIR}/launchctl" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$LAUNCHCTL_LOG"
if [ "\${1:-}" = "print" ]; then
  printf 'service=%s\n' "\${2:-unknown}"
fi
exit 0
EOF
chmod 700 "${FAKE_BIN_DIR}/launchctl"

cat > "${FAKE_BIN_DIR}/plutil" <<EOF
#!/bin/sh
printf '%s\n' "\$*" >> "$PLUTIL_LOG"
[ "\$1" = "-lint" ]
[ -f "\$2" ]
exit 0
EOF
chmod 700 "${FAKE_BIN_DIR}/plutil"

for script in \
  scripts/install-bujamentor-launchd.sh \
  scripts/uninstall-bujamentor-launchd.sh \
  scripts/status-bujamentor-launchd.sh \
  scripts/doctor-bujamentor-launchd.sh \
  scripts/test-bujamentor-launchd-artifacts.sh
 do
  sh -n "${ROOT}/$script"
done

HOME="$HOME_DIR" \
OPENKAKAO_BUJAMENTOR_LAUNCHCTL="${FAKE_BIN_DIR}/launchctl" \
OPENKAKAO_BUJAMENTOR_PLUTIL="${FAKE_BIN_DIR}/plutil" \
sh "${ROOT}/scripts/install-bujamentor-launchd.sh" \
  --mode preflight \
  --bin "$BIN_PATH" \
  --health-bin "$HEALTH_BIN_PATH" \
  --state-root "$STATE_ROOT"

[ -d "$STATE_ROOT" ]
[ -d "$LAUNCH_AGENTS_DIR" ]
[ -f "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist" ]
[ -f "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.health.plist" ]
[ ! -e "${STATE_ROOT}/watch-status.json" ]
[ ! -e "${STATE_ROOT}/health-alerts.json" ]
[ ! -e "${STATE_ROOT}/watch.log" ]
[ ! -e "${STATE_ROOT}/health.log" ]

grep -F -- 'com.openkakao.bujamentor.watch' "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist" >/dev/null
grep -F -- '--interval' "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist" >/dev/null
grep -F -- 'watch-status.json' "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist" >/dev/null
if grep -F -- '--allow-watch-side-effects' "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist" >/dev/null; then
  echo 'preflight watch plist unexpectedly contains production flags' >&2
  exit 1
fi
grep -F -- 'com.openkakao.bujamentor.health' "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.health.plist" >/dev/null
grep -F -- 'health-alerts.json' "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.health.plist" >/dev/null

grep -F -- 'bootstrap gui/' "$LAUNCHCTL_LOG" >/dev/null
grep -F -- 'kickstart -k gui/' "$LAUNCHCTL_LOG" >/dev/null
grep -F -- 'print gui/' "$LAUNCHCTL_LOG" >/dev/null
grep -F -- '-lint' "$PLUTIL_LOG" >/dev/null

HOME="$HOME_DIR" \
OPENKAKAO_BUJAMENTOR_LAUNCHCTL="${FAKE_BIN_DIR}/launchctl" \
OPENKAKAO_BUJAMENTOR_PLUTIL="${FAKE_BIN_DIR}/plutil" \
sh "${ROOT}/scripts/install-bujamentor-launchd.sh" \
  --mode production \
  --bin "$BIN_PATH" \
  --health-bin "$HEALTH_BIN_PATH" \
  --hook-path "$HOOK_PATH" \
  --state-root "$STATE_ROOT"

grep -F -- '--allow-watch-side-effects' "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist" >/dev/null
grep -F -- '--unattended' "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist" >/dev/null
grep -F -- "$HOOK_PATH" "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist" >/dev/null

HOME="$HOME_DIR" \
OPENKAKAO_BUJAMENTOR_LAUNCHCTL="${FAKE_BIN_DIR}/launchctl" \
sh "${ROOT}/scripts/status-bujamentor-launchd.sh" >/dev/null
HOME="$HOME_DIR" \
sh "${ROOT}/scripts/doctor-bujamentor-launchd.sh" >/dev/null

if HOME="$HOME_DIR" sh "${ROOT}/scripts/install-bujamentor-launchd.sh" --mode production --bin "$BIN_PATH" --health-bin "$HEALTH_BIN_PATH" --hook-path "$SYMLINK_HOOK" --state-root "$STATE_ROOT"; then
  echo 'expected symlink hook path rejection' >&2
  exit 1
fi

HOME="$HOME_DIR" \
OPENKAKAO_BUJAMENTOR_LAUNCHCTL="${FAKE_BIN_DIR}/launchctl" \
sh "${ROOT}/scripts/uninstall-bujamentor-launchd.sh" --purge-state --state-root "$STATE_ROOT"

[ ! -e "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist" ]
[ ! -e "${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.health.plist" ]

printf 'launchd artifact harness passed\n'
