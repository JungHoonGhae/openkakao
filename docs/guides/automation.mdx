---
title: Automation
description: Build automations with OpenKakao and Unix tools.
---

## JSON Output

All commands support `--json` for machine-readable output:

```bash
openkakao-rs friends --json | jq '.[] | .nickname'
openkakao-rs loco-chats --json | jq '.[] | select(.unread > 0)'
openkakao-rs loco-read <chat_id> --all --json > messages.json
```

## Examples

### Daily Unread Summary

Send yourself a summary of unread chats every morning:

```bash
#!/bin/bash
MEMO_CHAT="<your_memo_chat_id>"
SUMMARY=$(openkakao-rs unread --json | jq -r '.[] | "\(.title): \(.unread_count)"')
openkakao-rs --no-prefix send "$MEMO_CHAT" "$SUMMARY" -y
```

### LLM Chat Summarizer

Summarize a chat room's recent messages using an LLM:

```bash
openkakao-rs loco-read <chat_id> -n 50 --json | \
  jq -r '.[] | "\(.author): \(.message)"' | \
  llm "Summarize this conversation in 3 bullet points"
```

### Keyword Alert

Watch for specific keywords and trigger a notification:

```bash
openkakao-rs watch 2>&1 | while read -r line; do
  if echo "$line" | grep -qi "urgent\|emergency"; then
    osascript -e "display notification \"$line\" with title \"KakaoTalk Alert\""
  fi
done
```

### Export Chat to SQLite

```bash
openkakao-rs loco-read <chat_id> --all --json | \
  jq -c '.[]' | \
  sqlite3 chats.db ".import /dev/stdin messages"
```

### Cron Job Setup

```bash
# Edit crontab
crontab -e

# Run daily at 9am
0 9 * * * /usr/local/bin/openkakao-rs unread --json | jq -r '.[] | "\(.title): \(.unread_count)"' | /usr/local/bin/openkakao-rs --no-prefix send <memo_id> "$(cat)" -y
```

## Claude Code Integration

With the OpenKakao agent skill installed, Claude Code can directly interact with your KakaoTalk:

```bash
# Install the skill
npx skills add JungHoonGhae/skills@openkakao-cli

# Then in Claude Code:
# "Read my recent messages from chat X"
# "Send a message to my memo chat"
# "Summarize today's unread chats"
```
