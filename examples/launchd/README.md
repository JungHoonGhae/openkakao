# launchd

Use these files as a starting point for long-running unattended OpenKakao jobs on macOS. The recommended receiver is the login-free, durable `notif-watch` LaunchAgent; the older `watch` examples require a LOCO server session and remain only for legacy setups.

Recommended shape:

1. Install `openkakao-cli` at `/opt/homebrew/bin/openkakao-cli` (or edit the wrapper).
2. Grant Full Disk Access to the executable used by the LaunchAgent; terminal-only permission does not cover a separately launched job.
3. Copy `openkakao-notif-watch-wrapper.sh` to `~/.config/openkakao/` and make it executable.
4. Replace every `YOUR_USER` in `com.openkakao.notif-watch.plist`, then copy it into `~/Library/LaunchAgents/`.
5. Create the log directory before bootstrap and load the agent:

```bash
mkdir -p ~/Library/Logs/openkakao
chmod 700 ~/.config/openkakao/openkakao-notif-watch-wrapper.sh
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.openkakao.notif-watch.plist
```

Guardrails:

- keep `--durable` enabled so captured events survive process restarts
- keep `--allow-watch-side-effects` explicit
- prefer `--hook-cmd` over `--webhook-url`
- keep logs in `~/Library/Logs/openkakao/`
- use `chat_id` + `log_id` as the hook/webhook idempotency key; delivery is at least once
- expect capped exponential retries; eight failed deliveries quarantine one event so later events continue
- delivered/quarantined tombstones are retained for 24 hours after Notification Center stops retaining the source notification, then pruned
- inspect `openkakao-cli doctor --json` before assuming the service is healthy

Operational checks:

```bash
launchctl print gui/$(id -u)/com.openkakao.notif-watch
tail -f ~/Library/Logs/openkakao/notif-watch.stderr.log
openkakao-cli doctor --json
```

Unload:

```bash
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.openkakao.notif-watch.plist
```

To add a hook or webhook, edit the wrapper and add `--unattended --allow-watch-side-effects` before `notif-watch`, then add the desired sink flags after it. This permission enables only the configured local/HTTP side effect; `notif-watch` itself never performs a Kakao write or server login.
