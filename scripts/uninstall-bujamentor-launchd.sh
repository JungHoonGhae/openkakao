#!/bin/sh
set -eu

LAUNCHCTL="${OPENKAKAO_BUJAMENTOR_LAUNCHCTL:-launchctl}"
PURGE_STATE=0
STATE_ROOT="${HOME}/Library/Application Support/openkakao/bujamentor"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --purge-state)
      PURGE_STATE=1
      shift
      ;;
    --state-root)
      STATE_ROOT="$2"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

UID_VALUE="$(id -u)"
WATCH_SERVICE="gui/${UID_VALUE}/com.openkakao.bujamentor.watch"
HEALTH_SERVICE="gui/${UID_VALUE}/com.openkakao.bujamentor.health"
WATCH_PLIST="${HOME}/Library/LaunchAgents/com.openkakao.bujamentor.watch.plist"
HEALTH_PLIST="${HOME}/Library/LaunchAgents/com.openkakao.bujamentor.health.plist"

"$LAUNCHCTL" bootout "$WATCH_SERVICE" >/dev/null 2>&1 || true
"$LAUNCHCTL" bootout "$HEALTH_SERVICE" >/dev/null 2>&1 || true
rm -f "$WATCH_PLIST" "$HEALTH_PLIST"

if [ "$PURGE_STATE" -eq 1 ]; then
  rm -f \
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
  rmdir "$STATE_ROOT" 2>/dev/null || true
fi

echo "removed bujamentor launchd artifacts"
