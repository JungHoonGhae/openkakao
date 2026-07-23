# launchd

Use these files as a starting point for long-running unattended OpenKakao jobs on macOS.

Recommended shape:

1. use `scripts/install-bujamentor-launchd.sh` for the supervised two-agent setup
2. keep `openkakao-watch-wrapper.sh` and `com.openkakao.watch.plist` only for the legacy single-agent watch path
3. read `docs/bujamentor-launchd-supervision.md` before promoting production mode after a binary change

Guardrails:

- keep `watch` local-first unless you truly need a remote webhook
- keep `--allow-watch-side-effects` explicit
- prefer `--hook-cmd` over `--webhook-url`
- treat GUI/TCC preflight as the only valid proof after binary changes

Operational checks:

```bash
sh scripts/status-bujamentor-launchd.sh
launchctl print gui/$(id -u)/com.openkakao.bujamentor.health
launchctl print gui/$(id -u)/com.openkakao.bujamentor.watch
```

Legacy single-agent unload:

```bash
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.openkakao.watch.plist
```
