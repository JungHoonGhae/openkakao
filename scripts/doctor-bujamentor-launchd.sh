#!/bin/sh
set -eu

STATE_ROOT="${HOME}/Library/Application Support/openkakao/bujamentor"
LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
WATCH_PLIST="${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.watch.plist"
HEALTH_PLIST="${LAUNCH_AGENTS_DIR}/com.openkakao.bujamentor.health.plist"

[ -d "$LAUNCH_AGENTS_DIR" ] || {
  echo "missing launch agents directory: $LAUNCH_AGENTS_DIR" >&2
  exit 1
}

if [ -e "$WATCH_PLIST" ] && [ ! -f "$WATCH_PLIST" ]; then
  echo "watch plist is not a regular file" >&2
  exit 1
fi
if [ -e "$HEALTH_PLIST" ] && [ ! -f "$HEALTH_PLIST" ]; then
  echo "health plist is not a regular file" >&2
  exit 1
fi
if [ -e "$STATE_ROOT" ] && [ ! -d "$STATE_ROOT" ]; then
  echo "state root is not a directory" >&2
  exit 1
fi

echo "launch_agents_dir=$LAUNCH_AGENTS_DIR"
echo "state_root=$STATE_ROOT"
echo "watch_plist_present=$( [ -f "$WATCH_PLIST" ] && echo yes || echo no )"
echo "health_plist_present=$( [ -f "$HEALTH_PLIST" ] && echo yes || echo no )"
