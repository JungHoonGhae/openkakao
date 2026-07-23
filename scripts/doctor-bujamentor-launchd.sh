#!/bin/sh
set -eu

STATE_ROOT="${HOME}/Library/Application Support/openkakao/bujamentor"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --state-root)
      [ "$#" -ge 2 ] || {
        echo 'missing value for --state-root' >&2
        exit 1
      }
      STATE_ROOT="$2"
      shift 2
      ;;
    -h|--help)
      printf '%s\n' 'usage: doctor-bujamentor-launchd.sh [--state-root ABS]'
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
WATCH_PLIST="${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist"
HEALTH_PLIST="${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.health.plist"

[ -d "$LAUNCH_AGENTS_DIR" ] || {
  echo "missing launch agents directory: $LAUNCH_AGENTS_DIR" >&2
  exit 1
}

for path in "$WATCH_PLIST" "$HEALTH_PLIST"; do
  if [ -e "$path" ] && [ ! -f "$path" ]; then
    echo "managed plist is not a regular file: $path" >&2
    exit 1
  fi
done

if [ -e "$STATE_ROOT" ] && [ ! -d "$STATE_ROOT" ]; then
  echo "state root is not a directory: $STATE_ROOT" >&2
  exit 1
fi

echo "launch_agents_dir=$LAUNCH_AGENTS_DIR"
echo "state_root=$STATE_ROOT"
echo "watch_plist_present=$( [ -f "$WATCH_PLIST" ] && echo yes || echo no )"
echo "health_plist_present=$( [ -f "$HEALTH_PLIST" ] && echo yes || echo no )"
