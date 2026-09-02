#!/bin/sh
set -eu

PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
export PATH

exec /opt/homebrew/bin/openkakao-cli \
  notif-watch \
  --durable \
  "$@"
