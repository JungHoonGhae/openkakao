#!/bin/sh
set -eu

LAUNCHCTL="${OPENKAKAO_BUJAMENTOR_LAUNCHCTL:-launchctl}"
PURGE_STATE=0
STATE_ROOT="${HOME}/Library/Application Support/openkakao/bujamentor"

die() {
  printf '%s\n' "$*" >&2
  exit 1
}

validate_absolute_path_text() {
  path="$1"
  label="$2"
  python3 - "$path" "$label" <<'PY'
import pathlib
import sys

path = sys.argv[1]
label = sys.argv[2]
if not path:
    raise SystemExit(f"{label} is required")
if "\n" in path or "\x00" in path:
    raise SystemExit(f"{label} must not contain control separators")
pure = pathlib.PurePosixPath(path)
if not pure.is_absolute():
    raise SystemExit(f"{label} must be an absolute path")
if path == "/":
    raise SystemExit(f"{label} must not be /")
for part in pure.parts:
    if part in {".", ".."}:
        raise SystemExit(f"{label} must not contain . or .. components")
PY
}

bootout_benign() {
  service="$1"
  set +e
  output=$($LAUNCHCTL bootout "$service" 2>&1)
  status=$?
  set -e
  if [ "$status" -eq 0 ]; then
    return 0
  fi
  case "$output" in
    *"not loaded"*|*"No such process"*|*"Could not find service"*|*"service not found"*)
      return 0
      ;;
  esac
  printf '%s\n' "$output" >&2
  return "$status"
}

remove_private_regular_if_present() {
  path="$1"
  label="$2"
  python3 - "$path" "$label" <<'PY'
import os
import stat
import sys

path = sys.argv[1]
label = sys.argv[2]
try:
    st = os.lstat(path)
except FileNotFoundError:
    raise SystemExit(0)
if stat.S_ISLNK(st.st_mode):
    raise SystemExit(f"{label} must not be a symlink: {path}")
if not stat.S_ISREG(st.st_mode):
    raise SystemExit(f"{label} must be a regular file: {path}")
if st.st_uid != os.geteuid():
    raise SystemExit(f"{label} must be owned by the effective uid: {path}")
os.unlink(path)
PY
}

prune_state_root_if_empty() {
  path="$1"
  python3 - "$path" <<'PY'
import os
import stat
import sys

path = sys.argv[1]
try:
    st = os.lstat(path)
except FileNotFoundError:
    raise SystemExit(0)
if stat.S_ISLNK(st.st_mode):
    raise SystemExit(f"state root must not be a symlink: {path}")
if not stat.S_ISDIR(st.st_mode):
    raise SystemExit(f"state root must be a directory: {path}")
if st.st_uid != os.geteuid():
    raise SystemExit(f"state root must be owned by the effective uid: {path}")
try:
    os.rmdir(path)
except OSError:
    pass
PY
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --purge-state)
      PURGE_STATE=1
      shift
      ;;
    --state-root)
      [ "$#" -ge 2 ] || die "missing value for --state-root"
      STATE_ROOT="$2"
      shift 2
      ;;
    -h|--help)
      printf '%s\n' 'usage: uninstall-bujamentor-launchd.sh [--purge-state] [--state-root ABS]'
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

validate_absolute_path_text "$STATE_ROOT" "--state-root"

UID_VALUE="$(id -u)"
WATCH_LABEL="com.openkakao.bujamentor.watch"
HEALTH_LABEL="com.openkakao.bujamentor.health"
WATCH_SERVICE="gui/${UID_VALUE}/${WATCH_LABEL}"
HEALTH_SERVICE="gui/${UID_VALUE}/${HEALTH_LABEL}"
WATCH_PLIST="${HOME}/Library/LaunchAgents/${WATCH_LABEL}.plist"
HEALTH_PLIST="${HOME}/Library/LaunchAgents/${HEALTH_LABEL}.plist"

bootout_benign "$WATCH_SERVICE" || die "failed to bootout ${WATCH_SERVICE}"
bootout_benign "$HEALTH_SERVICE" || die "failed to bootout ${HEALTH_SERVICE}"
remove_private_regular_if_present "$WATCH_PLIST" "watch plist"
remove_private_regular_if_present "$HEALTH_PLIST" "health plist"

if [ "$PURGE_STATE" -eq 1 ]; then
  for managed_path in \
    "$STATE_ROOT/watch-status.json" \
    "$STATE_ROOT/health-alerts.json" \
    "$STATE_ROOT/watch.log" \
    "$STATE_ROOT/watch.log.1" \
    "$STATE_ROOT/watch.log.2" \
    "$STATE_ROOT/watch.log.3" \
    "$STATE_ROOT/health.log" \
    "$STATE_ROOT/health.log.1" \
    "$STATE_ROOT/health.log.2" \
    "$STATE_ROOT/health.log.3"
  do
    remove_private_regular_if_present "$managed_path" "managed state file"
  done
  prune_state_root_if_empty "$STATE_ROOT"
fi

printf 'removed bujamentor launchd artifacts\n'
