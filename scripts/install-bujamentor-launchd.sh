#!/bin/sh
set -eu

LAUNCHCTL="${OPENKAKAO_BUJAMENTOR_LAUNCHCTL:-launchctl}"
PLUTIL="${OPENKAKAO_BUJAMENTOR_PLUTIL:-plutil}"
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

validate_private_executable() {
  path="$1"
  label="$2"
  require_private_mode="$3"
  validate_absolute_path_text "$path" "$label"
  python3 - "$path" "$label" "$require_private_mode" <<'PY'
import os
import stat
import sys

path, label, require_private_mode = sys.argv[1:4]
try:
    st = os.lstat(path)
except FileNotFoundError:
    raise SystemExit(f"{label} does not exist: {path}")
if stat.S_ISLNK(st.st_mode):
    raise SystemExit(f"{label} must not be a symlink: {path}")
if not stat.S_ISREG(st.st_mode):
    raise SystemExit(f"{label} must be a regular file: {path}")
if st.st_uid != os.geteuid():
    raise SystemExit(f"{label} must be owned by the effective uid: {path}")
if st.st_mode & stat.S_IXUSR == 0:
    raise SystemExit(f"{label} must be owner-executable: {path}")
if st.st_mode & 0o022:
    raise SystemExit(f"{label} must not be group- or world-writable: {path}")
if require_private_mode == "yes" and st.st_mode & 0o077:
    raise SystemExit(f"{label} must have mode bits masked by 0077: {path}")
PY
}

validate_state_root_text() {
  validate_absolute_path_text "$1" "$2"
}

ensure_private_directory() {
  target="$1"
  mode="$2"
  home="$3"
  python3 - "$target" "$mode" "$home" <<'PY'
import os
import pathlib
import stat
import sys

TARGET, MODE, HOME = sys.argv[1:4]
EUID = os.geteuid()

def fail(message: str) -> None:
    raise SystemExit(message)

def lstat_dir(path: str, label: str) -> os.stat_result:
    try:
        st = os.lstat(path)
    except FileNotFoundError:
        fail(f"{label} is missing: {path}")
    if stat.S_ISLNK(st.st_mode):
        fail(f"{label} must not be a symlink: {path}")
    if not stat.S_ISDIR(st.st_mode):
        fail(f"{label} must be a directory: {path}")
    if st.st_uid != EUID:
        fail(f"{label} must be owned by the effective uid: {path}")
    if st.st_mode & 0o022:
        fail(f"{label} must not be group- or world-writable: {path}")
    return st

def parts_after(anchor: str, target: str) -> list[str]:
    anchor_parts = pathlib.PurePosixPath(anchor).parts
    target_parts = pathlib.PurePosixPath(target).parts
    if target_parts[: len(anchor_parts)] != anchor_parts:
        fail(f"target escapes anchor: {target}")
    return list(target_parts[len(anchor_parts) :])

if MODE == "plist":
    anchor = HOME
    lstat_dir(anchor, "plist anchor")
    suffix = parts_after(anchor, TARGET)
    if suffix != ["Library", "LaunchAgents"]:
        fail(f"plist target must stay within $HOME/Library/LaunchAgents: {TARGET}")
else:
    target_parts = pathlib.PurePosixPath(TARGET).parts
    current = target_parts[0]
    deepest = current
    for part in target_parts[1:]:
        candidate = os.path.join(current, part) if current != "/" else f"/{part}"
        if os.path.lexists(candidate):
            deepest = candidate
            current = candidate
        else:
            break
    anchor = deepest
    lstat_dir(anchor, "state anchor")
    suffix = parts_after(anchor, TARGET)

current = anchor
for part in suffix:
    current = os.path.join(current, part) if current != "/" else f"/{part}"
    if os.path.lexists(current):
        lstat_dir(current, "managed directory")
        continue
    os.mkdir(current, 0o700)
    lstat_dir(current, "managed directory")

current = anchor
for part in suffix:
    current = os.path.join(current, part) if current != "/" else f"/{part}"
    lstat_dir(current, "managed directory")
PY
}

fsync_directory() {
  dir_path="$1"
  python3 - "$dir_path" <<'PY'
import os
import sys

fd = os.open(sys.argv[1], os.O_RDONLY)
try:
    os.fsync(fd)
finally:
    os.close(fd)
PY
}

watch_program_arguments() {
  printf '%s\n' "$BIN"
  if [ "$MODE" = "production" ]; then
    printf '%s\n' "--unattended" "--allow-watch-side-effects"
  fi
  printf '%s\n' \
    "ax-watch" \
    "--service-mode" \
    "--interval" \
    "5" \
    "--status-path" \
    "${STATE_ROOT}/watch-status.json" \
    "--log-path" \
    "${STATE_ROOT}/watch.log"
  if [ "$MODE" = "production" ]; then
    printf '%s\n' "--hook-path" "$HOOK_PATH"
  fi
}

health_program_arguments() {
  printf '%s\n' \
    "$HEALTH_BIN" \
    "--status-path" \
    "${STATE_ROOT}/watch-status.json" \
    "--alerts-path" \
    "${STATE_ROOT}/health-alerts.json" \
    "--log-path" \
    "${STATE_ROOT}/health.log" \
    "--interval-secs" \
    "15"
}

write_plist_atomic() {
  target="$1"
  label="$2"
  generator="$3"
  tmp="${target}.tmp.$$"
  args_lines=$($generator)
  umask 077
  PROGRAM_ARGUMENTS="$args_lines" python3 - "$tmp" "$label" <<'PY'
import os
import plistlib
import sys

path, label = sys.argv[1:3]
argv = os.environ.get("PROGRAM_ARGUMENTS", "").splitlines()
plist = {
    "Label": label,
    "RunAtLoad": True,
    "KeepAlive": {"SuccessfulExit": False},
    "ThrottleInterval": 10,
    "ProgramArguments": argv,
}
with open(path, "wb") as fh:
    plistlib.dump(plist, fh, sort_keys=False)
    fh.flush()
    os.fsync(fh.fileno())
os.chmod(path, 0o600)
PY
  "$PLUTIL" -lint "$tmp" >/dev/null
  mv "$tmp" "$target"
  fsync_directory "$(dirname "$target")"
}

verify_plist_arguments() {
  target="$1"
  generator="$2"
  label="$3"
  args_lines=$($generator)
  expected_json=$(PROGRAM_ARGUMENTS="$args_lines" python3 - <<'PY'
import json
import os
print(json.dumps(os.environ.get("PROGRAM_ARGUMENTS", "").splitlines(), separators=(",", ":")))
PY
)
  actual_json=$(python3 - "$target" <<'PY'
import json
import plistlib
import sys
with open(sys.argv[1], 'rb') as fh:
    plist = plistlib.load(fh)
print(json.dumps(plist.get('ProgramArguments', []), separators=(",", ":")))
PY
)
  [ "$expected_json" = "$actual_json" ] || die "$label ProgramArguments mismatch"
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

cleanup_loaded_service() {
  service="$1"
  set +e
  output=$($LAUNCHCTL bootout "$service" 2>&1)
  status=$?
  set -e
  if [ "$status" -eq 0 ]; then
    return 0
  fi
  printf '%s\n' "$output" >&2
  return "$status"
}

install_health_or_fail() {
  health_loaded=0
  write_plist_atomic "$HEALTH_PLIST" "$HEALTH_LABEL" health_program_arguments
  verify_plist_arguments "$HEALTH_PLIST" health_program_arguments "health plist"
  if ! $LAUNCHCTL bootstrap "gui/${UID_VALUE}" "$HEALTH_PLIST"; then
    die "failed to bootstrap ${HEALTH_SERVICE}"
  fi
  health_loaded=1
  if ! $LAUNCHCTL kickstart -k "$HEALTH_SERVICE"; then
    if cleanup_loaded_service "$HEALTH_SERVICE"; then
      die "failed to kickstart ${HEALTH_SERVICE}"
    fi
    die "failed to kickstart ${HEALTH_SERVICE}; cleanup failed"
  fi
  if ! $LAUNCHCTL print "$HEALTH_SERVICE" >/dev/null; then
    if cleanup_loaded_service "$HEALTH_SERVICE"; then
      die "failed to verify ${HEALTH_SERVICE}"
    fi
    die "failed to verify ${HEALTH_SERVICE}; cleanup failed"
  fi
}

install_watch_or_fail() {
  watch_loaded=0
  write_plist_atomic "$WATCH_PLIST" "$WATCH_LABEL" watch_program_arguments
  verify_plist_arguments "$WATCH_PLIST" watch_program_arguments "watch plist"
  if ! $LAUNCHCTL bootstrap "gui/${UID_VALUE}" "$WATCH_PLIST"; then
    die "failed to bootstrap ${WATCH_SERVICE}"
  fi
  watch_loaded=1
  if ! $LAUNCHCTL kickstart -k "$WATCH_SERVICE"; then
    if cleanup_loaded_service "$WATCH_SERVICE"; then
      die "failed to kickstart ${WATCH_SERVICE}"
    fi
    die "failed to kickstart ${WATCH_SERVICE}; cleanup failed"
  fi
  if ! $LAUNCHCTL print "$WATCH_SERVICE" >/dev/null; then
    if cleanup_loaded_service "$WATCH_SERVICE"; then
      die "failed to verify ${WATCH_SERVICE}"
    fi
    die "failed to verify ${WATCH_SERVICE}; cleanup failed"
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --mode)
      [ "$#" -ge 2 ] || die "missing value for --mode"
      MODE="$2"
      shift 2
      ;;
    --bin)
      [ "$#" -ge 2 ] || die "missing value for --bin"
      BIN="$2"
      shift 2
      ;;
    --health-bin)
      [ "$#" -ge 2 ] || die "missing value for --health-bin"
      HEALTH_BIN="$2"
      shift 2
      ;;
    --hook-path)
      [ "$#" -ge 2 ] || die "missing value for --hook-path"
      HOOK_PATH="$2"
      shift 2
      ;;
    --state-root)
      [ "$#" -ge 2 ] || die "missing value for --state-root"
      STATE_ROOT="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      die "unknown argument: $1"
      ;;
  esac
done

case "$MODE" in
  preflight|production) ;;
  *) die "--mode must be preflight or production" ;;
esac

validate_private_executable "$BIN" "--bin" no
validate_private_executable "$HEALTH_BIN" "--health-bin" no
validate_state_root_text "$STATE_ROOT" "--state-root"
if [ "$MODE" = "production" ]; then
  validate_private_executable "$HOOK_PATH" "--hook-path" yes
elif [ -n "$HOOK_PATH" ]; then
  die "--hook-path is only valid with --mode production"
fi

LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
WATCH_LABEL="com.openkakao.bujamentor.watch"
HEALTH_LABEL="com.openkakao.bujamentor.health"
WATCH_PLIST="${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist"
HEALTH_PLIST="${LAUNCH_AGENTS_DIR}/${HEALTH_LABEL}.plist"
UID_VALUE="$(id -u)"
WATCH_SERVICE="gui/${UID_VALUE}/${WATCH_LABEL}"
HEALTH_SERVICE="gui/${UID_VALUE}/${HEALTH_LABEL}"

ensure_private_directory "$LAUNCH_AGENTS_DIR" plist "$HOME"
ensure_private_directory "$STATE_ROOT" state "$HOME"

bootout_benign "$WATCH_SERVICE" || die "failed to bootout ${WATCH_SERVICE}"
bootout_benign "$HEALTH_SERVICE" || die "failed to bootout ${HEALTH_SERVICE}"
install_health_or_fail
install_watch_or_fail

printf 'installed %s and %s\n' "$WATCH_SERVICE" "$HEALTH_SERVICE"
