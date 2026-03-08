#!/bin/sh
set -eu

PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
export PATH

LOG_DIR="${HOME}/Library/Logs/openkakao"
mkdir -p "${LOG_DIR}"

exec /opt/homebrew/bin/openkakao-rs \
  --unattended \
  --allow-watch-side-effects \
  watch \
  --max-reconnect 20 \
  "$@"
