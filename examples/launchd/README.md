# launchd

Use these files as a starting point for long-running unattended OpenKakao jobs on macOS.

Recommended shape:

1. copy `openkakao-watch-wrapper.sh` to a stable local path
2. edit the command flags for your environment
3. copy `com.openkakao.watch.plist` into `~/Library/LaunchAgents/`
4. load it with `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.openkakao.watch.plist`

Guardrails:

- keep `watch` local-first unless you truly need a remote webhook
- keep `--allow-watch-side-effects` explicit
- prefer `--hook-cmd` over `--webhook-url`
- keep logs in `~/Library/Logs/openkakao/`
- inspect `openkakao-rs auth-status` and `openkakao-rs doctor --loco` before assuming the service is healthy

Operational checks:

```bash
launchctl print gui/$(id -u)/com.openkakao.watch
tail -f ~/Library/Logs/openkakao/watch.stderr.log
openkakao-rs auth-status
openkakao-rs doctor --loco
```

Unload:

```bash
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.openkakao.watch.plist
```
