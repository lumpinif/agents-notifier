# Setup

中文文档：[setup.zh-CN.md](setup.zh-CN.md)

Use setup to create or replace the local config, start the service, and send a test notification.

```bash
agents-notifier setup
```

Without an existing config, setup shows recommended defaults. If a config already exists, setup
prints `Current` for existing answers, and pressing Enter keeps the current value. Webhook URLs are
shown by host only. Signing secrets and private provider keys are shown only as configured.

For a Feishu/Lark signing secret, type `none` to clear the existing secret.

## Agent

Choose the agent Agents Notifier should watch:

```text
1. Codex Desktop
2. Codex CLI
3. Claude Code
```

Codex Desktop is currently offered only on macOS. On Linux and Windows, setup offers Codex CLI and Claude Code hook sources.

## Provider

Choose where notifications should go:

```text
1. ntfy
2. Slack
3. Discord
4. Pushover
5. Feishu/Lark custom bot
6. Webhook
```

Provider guides:

- [Feishu/Lark Custom Bot](providers/feishu-lark-custom-bot.md)
- [ntfy](providers/ntfy.md)
- [Pushover](providers/pushover.md)
- [Slack](providers/slack.md)
- [Discord](providers/discord.md)
- [Webhook](providers/webhook.md)

## Answer Detail

Choose how much answer text notifications include:

```text
1. Preview (Recommended)
2. Full Answer
```

Press Enter to keep `Preview`.
Full Answer includes the visible assistant answer and omits Codex App control directives.

Answer detail is only configurable for providers without a small documented message size limit.

Agents Notifier fixes answer detail to `Preview` for:

- ntfy, because ntfy has a configurable message body size limit that defaults to 4K.
- Pushover, because Pushover messages are limited to 1024 characters.
- Slack, because Slack has documented message length and truncation limits.
- Discord, because Discord webhook content is limited to 2000 characters.

## Prompt Detail

Choose whether notifications include your original prompt:

```text
1. No (Recommended)
2. Yes
```

Press Enter to keep `No`. Prompt detail is off by default because prompts can contain private
requirements, code, logs, paths, or secrets.
For Codex Desktop, the prompt comes from Codex's local `user_message` record. Codex may include
IDE context such as the active file and open tabs in that record.
If a source does not provide a prompt, no Prompt section is shown.

Manual config:

```toml
[notification]
answer_detail = "preview"
prompt_detail = "off"
```

Prompt detail is only configurable for providers without a small documented message size limit.

Agents Notifier disables prompt detail for:

- ntfy, because ntfy has a configurable message body size limit that defaults to 4K.
- Pushover, because Pushover messages are limited to 1024 characters.
- Slack, because Slack has documented message length and truncation limits.
- Discord, because Discord webhook content is limited to 2000 characters.

To include prompts:

```toml
[notification]
prompt_detail = "on"
```

To send full answers:

```toml
[notification]
answer_detail = "full"
```

To send full answers and include prompts:

```toml
[notification]
answer_detail = "full"
prompt_detail = "on"
```

After manual edits, restart the service:

```bash
agents-notifier stop
agents-notifier start
```

## Result

Setup writes:

```text
~/.config/agents-notifier/config.toml
```

Then it starts the local service and sends a test notification through the same route.
On macOS this is a LaunchAgent. On Linux this is a systemd user service. On Windows this is a Task Scheduler task.
