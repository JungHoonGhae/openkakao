#!/bin/sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
FAKE_BIN_DIR="${TMP}/bin"
FAKE_STATE_DIR="${TMP}/launchctl-state"
LAUNCHCTL_LOG="${TMP}/launchctl.log"
PLUTIL_LOG="${TMP}/plutil.log"
BIN_PATH="${TMP}/openkakao-cli"
HEALTH_BIN_PATH="${TMP}/openkakao-bujamentor-health"
HOOK_PATH="${TMP}/hook.sh"
SYMLINK_HOOK="${TMP}/hook-link.sh"
UID_VALUE="$(id -u)"
WATCH_LABEL="com.openkakao.bujamentor.watch"
HEALTH_LABEL="com.openkakao.bujamentor.health"
WATCH_SERVICE="gui/${UID_VALUE}/${WATCH_LABEL}"
HEALTH_SERVICE="gui/${UID_VALUE}/${HEALTH_LABEL}"

cleanup() {
  rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

fail() {
  printf '%s\n' "$*" >&2
  exit 1
}

json_args() {
  python3 - "$@" <<'PY'
import json
import sys
print(json.dumps(sys.argv[1:], separators=(",", ":")))
PY
}

assert_file_contains() {
  file="$1"
  needle="$2"
  grep -F -- "$needle" "$file" >/dev/null || fail "expected $file to contain: $needle"
}

assert_file_not_contains() {
  file="$1"
  needle="$2"
  if grep -F -- "$needle" "$file" >/dev/null; then
    fail "did not expect $file to contain: $needle"
  fi
}

assert_log_order() {
  first="$1"
  second="$2"
  python3 - "$LAUNCHCTL_LOG" "$first" "$second" <<'PY'
import sys
log_path, first, second = sys.argv[1:4]
with open(log_path, 'r', encoding='utf-8') as fh:
    lines = fh.read().splitlines()
try:
    first_index = next(i for i, line in enumerate(lines) if first in line)
except StopIteration:
    raise SystemExit(f"missing log entry: {first}")
try:
    second_index = next(i for i, line in enumerate(lines) if second in line)
except StopIteration:
    raise SystemExit(f"missing log entry: {second}")
if first_index >= second_index:
    raise SystemExit(f"log order mismatch: {first} !< {second}")
PY
}

assert_log_last_after() {
  first="$1"
  second="$2"
  python3 - "$LAUNCHCTL_LOG" "$first" "$second" <<'PY'
import sys
log_path, first, second = sys.argv[1:4]
with open(log_path, 'r', encoding='utf-8') as fh:
    lines = fh.read().splitlines()
try:
    first_index = next(i for i, line in enumerate(lines) if first in line)
except StopIteration:
    raise SystemExit(f"missing log entry: {first}")
matching = [i for i, line in enumerate(lines) if second in line]
if not matching:
    raise SystemExit(f"missing log entry: {second}")
if first_index >= matching[-1]:
    raise SystemExit(f"log order mismatch: {first} !< last({second})")
PY
}

assert_plist_argv() {
  plist_path="$1"
  expected_json="$2"
  actual_json=$(python3 - "$plist_path" <<'PY'
import json
import plistlib
import sys
with open(sys.argv[1], 'rb') as fh:
    plist = plistlib.load(fh)
print(json.dumps(plist.get('ProgramArguments', []), separators=(",", ":")))
PY
)
  [ "$actual_json" = "$expected_json" ] || fail "unexpected ProgramArguments for $plist_path"
}

assert_loaded_argv() {
  service="$1"
  expected_json="$2"
  actual_json=$(python3 - "$FAKE_STATE_DIR" "$service" <<'PY'
import json
import pathlib
import sys
state_dir = pathlib.Path(sys.argv[1])
service = sys.argv[2]
label = service.split('/')[-1]
path = state_dir / f"{label}.json"
if not path.exists():
    raise SystemExit(f"service not loaded: {service}")
print(json.dumps(json.loads(path.read_text())['argv'], separators=(",", ":")))
PY
)
  [ "$actual_json" = "$expected_json" ] || fail "unexpected loaded argv for $service"
}

assert_service_loaded() {
  service="$1"
  label=${service##*/}
  [ -f "$FAKE_STATE_DIR/${label}.json" ] || fail "expected loaded service: $service"
}

assert_service_absent() {
  service="$1"
  label=${service##*/}
  [ ! -e "$FAKE_STATE_DIR/${label}.json" ] || fail "expected absent service: $service"
}

reset_case() {
  case_name="$1"
  HOME_DIR="${TMP}/${case_name}-home"
  STATE_ROOT="${HOME_DIR}/Library/Application Support/openkakao/bujamentor"
  LAUNCH_AGENTS_DIR="${HOME_DIR}/Library/LaunchAgents"
  rm -rf "$HOME_DIR" "$FAKE_STATE_DIR"
  mkdir -p "$HOME_DIR" "$FAKE_STATE_DIR"
  : > "$LAUNCHCTL_LOG"
  : > "$PLUTIL_LOG"
  OPENKAKAO_BUJAMENTOR_FAIL_BOOTSTRAP=
  OPENKAKAO_BUJAMENTOR_FAIL_KICKSTART=
  OPENKAKAO_BUJAMENTOR_FAIL_PRINT=
  OPENKAKAO_BUJAMENTOR_FAIL_BOOTOUT=
}

run_install() {
  mode="$1"
  shift
  HOME="$HOME_DIR" \
  FAKE_LAUNCHCTL_LOG="$LAUNCHCTL_LOG" \
  FAKE_LAUNCHCTL_STATE_DIR="$FAKE_STATE_DIR" \
  OPENKAKAO_BUJAMENTOR_LAUNCHCTL="${FAKE_BIN_DIR}/launchctl" \
  OPENKAKAO_BUJAMENTOR_PLUTIL="${FAKE_BIN_DIR}/plutil" \
  OPENKAKAO_BUJAMENTOR_FAIL_BOOTSTRAP="${OPENKAKAO_BUJAMENTOR_FAIL_BOOTSTRAP-}" \
  OPENKAKAO_BUJAMENTOR_FAIL_KICKSTART="${OPENKAKAO_BUJAMENTOR_FAIL_KICKSTART-}" \
  OPENKAKAO_BUJAMENTOR_FAIL_PRINT="${OPENKAKAO_BUJAMENTOR_FAIL_PRINT-}" \
  OPENKAKAO_BUJAMENTOR_FAIL_BOOTOUT="${OPENKAKAO_BUJAMENTOR_FAIL_BOOTOUT-}" \
  sh "${ROOT}/scripts/install-bujamentor-launchd.sh" \
    --mode "$mode" \
    --bin "$BIN_PATH" \
    --health-bin "$HEALTH_BIN_PATH" \
    "$@" \
    --state-root "$STATE_ROOT"
}

run_install_production() {
  run_install production --hook-path "$HOOK_PATH"
}

run_install_preflight() {
  run_install preflight
}

run_uninstall() {
  HOME="$HOME_DIR" \
  FAKE_LAUNCHCTL_LOG="$LAUNCHCTL_LOG" \
  FAKE_LAUNCHCTL_STATE_DIR="$FAKE_STATE_DIR" \
  OPENKAKAO_BUJAMENTOR_LAUNCHCTL="${FAKE_BIN_DIR}/launchctl" \
  sh "${ROOT}/scripts/uninstall-bujamentor-launchd.sh" --state-root "$STATE_ROOT" "$@"
}

mkdir -p "$FAKE_BIN_DIR"
printf '#!/bin/sh\nexit 0\n' > "$BIN_PATH"
printf '#!/bin/sh\nexit 0\n' > "$HEALTH_BIN_PATH"
printf '#!/bin/sh\nexit 0\n' > "$HOOK_PATH"
chmod 700 "$BIN_PATH" "$HEALTH_BIN_PATH" "$HOOK_PATH"
ln -s "$HOOK_PATH" "$SYMLINK_HOOK"

cat > "${FAKE_BIN_DIR}/launchctl" <<'EOF'
#!/usr/bin/env python3
import json
import os
import pathlib
import plistlib
import sys

log_path = pathlib.Path(os.environ["FAKE_LAUNCHCTL_LOG"])
state_dir = pathlib.Path(os.environ["FAKE_LAUNCHCTL_STATE_DIR"])
state_dir.mkdir(parents=True, exist_ok=True)
args = sys.argv[1:]
with log_path.open("a", encoding="utf-8") as fh:
    fh.write(" ".join(args) + "\n")

def fail_set(name: str) -> set[str]:
    raw = os.environ.get(name, "")
    return {value for value in raw.split(":") if value}

def label_from_service(service: str) -> str:
    return service.split("/")[-1]

def service_path(label: str) -> pathlib.Path:
    return state_dir / f"{label}.json"

if not args:
    raise SystemExit("missing launchctl action")
action = args[0]
if action == "bootstrap":
    domain, plist_path = args[1], pathlib.Path(args[2])
    with plist_path.open("rb") as fh:
        plist = plistlib.load(fh)
    label = plist["Label"]
    service = f"{domain}/{label}"
    if service in fail_set("OPENKAKAO_BUJAMENTOR_FAIL_BOOTSTRAP"):
        print(f"bootstrap failed for {service}", file=sys.stderr)
        raise SystemExit(71)
    service_path(label).write_text(json.dumps({"service": service, "argv": plist.get("ProgramArguments", [])}))
elif action == "kickstart":
    service = args[2] if len(args) >= 3 and args[1] == "-k" else args[1]
    if service in fail_set("OPENKAKAO_BUJAMENTOR_FAIL_KICKSTART"):
        print(f"kickstart failed for {service}", file=sys.stderr)
        raise SystemExit(72)
    path = service_path(label_from_service(service))
    if not path.exists():
        print(f"service not loaded: {service}", file=sys.stderr)
        raise SystemExit(36)
elif action == "print":
    service = args[1]
    if service in fail_set("OPENKAKAO_BUJAMENTOR_FAIL_PRINT"):
        print(f"print failed for {service}", file=sys.stderr)
        raise SystemExit(73)
    path = service_path(label_from_service(service))
    if not path.exists():
        print(f"service not loaded: {service}", file=sys.stderr)
        raise SystemExit(36)
    payload = json.loads(path.read_text())
    print(f"service={payload['service']}")
    for index, value in enumerate(payload["argv"]):
        print(f"argv[{index}]={value}")
elif action == "bootout":
    service = args[1]
    if service in fail_set("OPENKAKAO_BUJAMENTOR_FAIL_BOOTOUT"):
        print(f"bootout failed for {service}", file=sys.stderr)
        raise SystemExit(74)
    path = service_path(label_from_service(service))
    if not path.exists():
        print(f"service not loaded: {service}", file=sys.stderr)
        raise SystemExit(36)
    path.unlink()
else:
    raise SystemExit(f"unsupported launchctl action: {action}")
EOF
chmod 700 "${FAKE_BIN_DIR}/launchctl"

cat > "${FAKE_BIN_DIR}/plutil" <<EOF
#!/usr/bin/env python3
import pathlib
import plistlib
import sys
pathlib.Path("$PLUTIL_LOG").open('a', encoding='utf-8').write(' '.join(sys.argv[1:]) + '\n')
if len(sys.argv) != 3 or sys.argv[1] != '-lint':
    raise SystemExit(1)
with pathlib.Path(sys.argv[2]).open('rb') as fh:
    plistlib.load(fh)
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

reset_case preflight
run_install_preflight
[ -d "$STATE_ROOT" ] || fail 'state root missing after preflight install'
[ -d "$LAUNCH_AGENTS_DIR" ] || fail 'LaunchAgents missing after preflight install'
[ -f "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" ] || fail 'watch plist missing after preflight install'
[ -f "${LAUNCH_AGENTS_DIR}/${HEALTH_LABEL}.plist" ] || fail 'health plist missing after preflight install'
[ ! -e "${STATE_ROOT}/watch-status.json" ] || fail 'watch status should not be precreated'
[ ! -e "${STATE_ROOT}/health-alerts.json" ] || fail 'health alerts should not be precreated'
[ ! -e "${STATE_ROOT}/watch.log" ] || fail 'watch log should not be precreated'
[ ! -e "${STATE_ROOT}/health.log" ] || fail 'health log should not be precreated'
preflight_watch_args=$(json_args \
  "$BIN_PATH" \
  "ax-watch" \
  "--service-mode" \
  "--interval" \
  "5" \
  "--status-path" \
  "$STATE_ROOT/watch-status.json" \
  "--log-path" \
  "$STATE_ROOT/watch.log")
health_args=$(json_args \
  "$HEALTH_BIN_PATH" \
  "--status-path" \
  "$STATE_ROOT/watch-status.json" \
  "--alerts-path" \
  "$STATE_ROOT/health-alerts.json" \
  "--log-path" \
  "$STATE_ROOT/health.log" \
  "--interval-secs" \
  "15")
assert_plist_argv "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" "$preflight_watch_args"
assert_plist_argv "${LAUNCH_AGENTS_DIR}/${HEALTH_LABEL}.plist" "$health_args"
assert_loaded_argv "$WATCH_SERVICE" "$preflight_watch_args"
assert_loaded_argv "$HEALTH_SERVICE" "$health_args"
assert_file_not_contains "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" '--hook-path'
assert_file_not_contains "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" '--hook-cmd'
assert_file_not_contains "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" '--unattended'
assert_log_order "bootstrap gui/${UID_VALUE} ${LAUNCH_AGENTS_DIR}/${HEALTH_LABEL}.plist" "bootstrap gui/${UID_VALUE} ${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist"
assert_file_contains "$PLUTIL_LOG" '-lint'

run_install_production
production_watch_args=$(json_args \
  "$BIN_PATH" \
  "--unattended" \
  "--allow-watch-side-effects" \
  "ax-watch" \
  "--service-mode" \
  "--interval" \
  "5" \
  "--status-path" \
  "$STATE_ROOT/watch-status.json" \
  "--log-path" \
  "$STATE_ROOT/watch.log" \
  "--hook-path" \
  "$HOOK_PATH")
assert_plist_argv "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" "$production_watch_args"
assert_loaded_argv "$WATCH_SERVICE" "$production_watch_args"
assert_file_contains "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" '--service-mode'
assert_file_contains "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" '--hook-path'
assert_file_not_contains "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" '--hook-cmd'
assert_log_order "bootout ${WATCH_SERVICE}" "bootstrap gui/${UID_VALUE} ${LAUNCH_AGENTS_DIR}/${HEALTH_LABEL}.plist"

reset_case health-failure
if OPENKAKAO_BUJAMENTOR_FAIL_PRINT="$HEALTH_SERVICE" run_install_preflight; then
  fail 'expected health print failure'
fi
assert_service_absent "$HEALTH_SERVICE"
assert_service_absent "$WATCH_SERVICE"
assert_file_not_contains "$LAUNCHCTL_LOG" "bootstrap gui/${UID_VALUE} ${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist"
assert_log_last_after "print ${HEALTH_SERVICE}" "bootout ${HEALTH_SERVICE}"

reset_case watch-failure
if OPENKAKAO_BUJAMENTOR_FAIL_PRINT="$WATCH_SERVICE" run_install_production; then
  fail 'expected watch print failure'
fi
assert_service_loaded "$HEALTH_SERVICE"
assert_service_absent "$WATCH_SERVICE"
assert_log_last_after "print ${WATCH_SERVICE}" "bootout ${WATCH_SERVICE}"

reset_case bad-home
mkdir -p "${HOME_DIR}/home-target"
ln -s "${HOME_DIR}/home-target" "${HOME_DIR}/Library"
if run_install_preflight; then
  fail 'expected trusted anchor validation failure'
fi

reset_case symlink-hook
if HOME="$HOME_DIR" \
  FAKE_LAUNCHCTL_LOG="$LAUNCHCTL_LOG" \
  FAKE_LAUNCHCTL_STATE_DIR="$FAKE_STATE_DIR" \
  OPENKAKAO_BUJAMENTOR_LAUNCHCTL="${FAKE_BIN_DIR}/launchctl" \
  OPENKAKAO_BUJAMENTOR_PLUTIL="${FAKE_BIN_DIR}/plutil" \
  sh "${ROOT}/scripts/install-bujamentor-launchd.sh" \
    --mode production \
    --bin "$BIN_PATH" \
    --health-bin "$HEALTH_BIN_PATH" \
    --hook-path "$SYMLINK_HOOK" \
    --state-root "$STATE_ROOT"; then
  fail 'expected symlink hook path rejection'
fi

reset_case purge-safety
run_install_production
printf 'status\n' > "$STATE_ROOT/watch-status.json"
printf 'alerts\n' > "$STATE_ROOT/health-alerts.json"
printf 'watch\n' > "$STATE_ROOT/watch.log"
printf 'health\n' > "$STATE_ROOT/health.log"
printf 'keep\n' > "$STATE_ROOT/keep.txt"
run_uninstall --purge-state
[ ! -e "${LAUNCH_AGENTS_DIR}/${WATCH_LABEL}.plist" ] || fail 'watch plist should be removed'
[ ! -e "${LAUNCH_AGENTS_DIR}/${HEALTH_LABEL}.plist" ] || fail 'health plist should be removed'
[ ! -e "$STATE_ROOT/watch-status.json" ] || fail 'watch status should be purged'
[ ! -e "$STATE_ROOT/health-alerts.json" ] || fail 'health alerts should be purged'
[ ! -e "$STATE_ROOT/watch.log" ] || fail 'watch log should be purged'
[ ! -e "$STATE_ROOT/health.log" ] || fail 'health log should be purged'
[ -e "$STATE_ROOT/keep.txt" ] || fail 'unmanaged files must survive purge'
[ -d "$STATE_ROOT" ] || fail 'state root should remain when unmanaged files exist'
assert_service_absent "$HEALTH_SERVICE"
assert_service_absent "$WATCH_SERVICE"

printf 'launchd artifact harness passed\n'
