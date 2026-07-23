#!/bin/sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
LAUNCHCTL="${OPENKAKAO_BUJAMENTOR_LAUNCHCTL:-launchctl}"
PLUTIL="${OPENKAKAO_BUJAMENTOR_PLUTIL:-plutil}"
WATCH_TEMPLATE="${ROOT}/examples/launchd/com.openkakao.bujamentor.watch.plist.in"
HEALTH_TEMPLATE="${ROOT}/examples/launchd/com.openkakao.bujamentor.health.plist.in"
MODE=""
BIN=""
HEALTH_BIN=""
HOOK_PATH=""
STATE_ROOT="${HOME}/Library/Application Support/openkakao/bujamentor"

usage() {
  cat <<'EOF'
usage:
  install-bujamentor-launchd.sh --mode preflight --bin ABS --health-bin ABS [--state-root ABS]
  install-bujamentor-launchd.sh --mode production --bin ABS --health-bin ABS --hook-path ABS [--state-root ABS]
EOF
}

xml_escape() {
  printf '%s' "$1" | sed \
    -e 's/&/\&amp;/g' \
    -e 's/</\&lt;/g' \
    -e 's/>/\&gt;/g' \
    -e 's/"/\&quot;/g' \
    -e "s/'/\&apos;/g"
}

require_absolute_file() {
  path="$1"
  label="$2"
  [ -n "$path" ] || { echo "$label is required" >&2; exit 1; }
  case "$path" in
    /*) ;;
    *) echo "$label must be an absolute path" >&2; exit 1 ;;
  esac
  case "$path" in
    *"
"*) echo "$label must not contain newlines" >&2; exit 1 ;;
  esac
  [ -e "$path" ] || { echo "$label does not exist: $path" >&2; exit 1; }
  [ ! -L "$path" ] || { echo "$label must not be a symlink: $path" >&2; exit 1; }
  [ -f "$path" ] || { echo "$label must be a regular file: $path" >&2; exit 1; }
  [ -x "$path" ] || { echo "$label must be executable: $path" >&2; exit 1; }
  mode=$(python3 - "$path" <<'PY'
import os, stat, sys
print(oct(os.stat(sys.argv[1]).st_mode & 0o777))
PY
)
  case "$mode" in
    *2|*3|*6|*7|*12|*13|*16|*17|*22|*23|*26|*27|*32|*33|*36|*37|*42|*43|*46|*47|*52|*53|*56|*57|*62|*63|*66|*67|*72|*73|*76|*77)
      echo "$label must not be group- or world-writable: $path" >&2
      exit 1
      ;;
  esac
}

require_absolute_dir_path() {
  path="$1"
  label="$2"
  case "$path" in
    /*) ;;
    *) echo "$label must be an absolute path" >&2; exit 1 ;;
  esac
  case "$path" in
    *"
"*) echo "$label must not contain newlines" >&2; exit 1 ;;
  esac
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --mode)
      MODE="$2"
      shift 2
      ;;
    --bin)
      BIN="$2"
      shift 2
      ;;
    --health-bin)
      HEALTH_BIN="$2"
      shift 2
      ;;
    --hook-path)
      HOOK_PATH="$2"
      shift 2
      ;;
    --state-root)
      STATE_ROOT="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

case "$MODE" in
  preflight|production) ;;
  *) echo "--mode must be preflight or production" >&2; exit 1 ;;
esac

require_absolute_file "$BIN" "--bin"
require_absolute_file "$HEALTH_BIN" "--health-bin"
require_absolute_dir_path "$STATE_ROOT" "--state-root"
if [ "$MODE" = "production" ]; then
  require_absolute_file "$HOOK_PATH" "--hook-path"
fi

STATE_ROOT_DIR="$STATE_ROOT"
LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
WATCH_PLIST="${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist"
HEALTH_PLIST="${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.health.plist"
UID_VALUE="$(id -u)"
WATCH_SERVICE="gui/${UID_VALUE}/com.openkakao.bujamentor.watch"
HEALTH_SERVICE="gui/${UID_VALUE}/com.openkakao.bujamentor.health"

mkdir -p "$STATE_ROOT_DIR" "$LAUNCH_AGENTS_DIR"
chmod 700 "$STATE_ROOT_DIR" "$LAUNCH_AGENTS_DIR"

watch_extra_args() {
  if [ "$MODE" = "production" ]; then
    cat <<EOF
    <string>--unattended</string>
    <string>--allow-watch-side-effects</string>
    <string>ax-watch</string>
    <string>--interval</string>
    <string>5</string>
    <string>--status-path</string>
    <string>$(xml_escape "$STATE_ROOT_DIR")/watch-status.json</string>
    <string>--log-path</string>
    <string>$(xml_escape "$STATE_ROOT_DIR")/watch.log</string>
    <string>--hook-chat</string>
    <string>Bujamentor</string>
    <string>--hook-cmd</string>
    <string>$(xml_escape "$HOOK_PATH")</string>
EOF
  else
    cat <<EOF
    <string>ax-watch</string>
    <string>--interval</string>
    <string>5</string>
    <string>--status-path</string>
    <string>$(xml_escape "$STATE_ROOT_DIR")/watch-status.json</string>
    <string>--log-path</string>
    <string>$(xml_escape "$STATE_ROOT_DIR")/watch.log</string>
    <string>--hook-chat</string>
    <string>Bujamentor</string>
EOF
  fi
}

render_watch() {
  cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.openkakao.bujamentor.watch</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>ProgramArguments</key>
  <array>
    <string>$(xml_escape "$BIN")</string>
$(watch_extra_args)
  </array>
</dict>
</plist>
EOF
}

render_health() {
  cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.openkakao.bujamentor.health</string>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>ProgramArguments</key>
  <array>
    <string>$(xml_escape "$HEALTH_BIN")</string>
    <string>--status-path</string>
    <string>$(xml_escape "$STATE_ROOT_DIR")/watch-status.json</string>
    <string>--alerts-path</string>
    <string>$(xml_escape "$STATE_ROOT_DIR")/health-alerts.json</string>
    <string>--log-path</string>
    <string>$(xml_escape "$STATE_ROOT_DIR")/health.log</string>
    <string>--interval-secs</string>
    <string>15</string>
  </array>
</dict>
</plist>
EOF
}

render_tmp_and_install() {
  target="$1"
  tmp="${target}.tmp.$$"
  umask 077
  "$2" > "$tmp"
  chmod 600 "$tmp"
  "$PLUTIL" -lint "$tmp" >/dev/null
  mv "$tmp" "$target"
}

"$LAUNCHCTL" bootout "$WATCH_SERVICE" >/dev/null 2>&1 || true
"$LAUNCHCTL" bootout "$HEALTH_SERVICE" >/dev/null 2>&1 || true
render_tmp_and_install "$HEALTH_PLIST" render_health
"$LAUNCHCTL" bootstrap "gui/${UID_VALUE}" "$HEALTH_PLIST"
"$LAUNCHCTL" kickstart -k "$HEALTH_SERVICE"
"$LAUNCHCTL" print "$HEALTH_SERVICE" >/dev/null
render_tmp_and_install "$WATCH_PLIST" render_watch
"$LAUNCHCTL" bootstrap "gui/${UID_VALUE}" "$WATCH_PLIST"
"$LAUNCHCTL" kickstart -k "$WATCH_SERVICE"
"$LAUNCHCTL" print "$WATCH_SERVICE" >/dev/null

echo "installed ${WATCH_SERVICE} and ${HEALTH_SERVICE}"
