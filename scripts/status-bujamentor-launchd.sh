#!/bin/sh
set -eu

LAUNCHCTL="${OPENKAKAO_BUJAMENTOR_LAUNCHCTL:-launchctl}"
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
      printf '%s\n' 'usage: status-bujamentor-launchd.sh [--state-root ABS]'
      exit 0
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

printf 'state_root=%s\n' "$STATE_ROOT"
printf 'watch_plist=%s\n' "$WATCH_PLIST"
printf 'health_plist=%s\n' "$HEALTH_PLIST"

"$LAUNCHCTL" print "$HEALTH_SERVICE" || true
"$LAUNCHCTL" print "$WATCH_SERVICE" || true
