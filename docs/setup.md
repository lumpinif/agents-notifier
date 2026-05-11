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

## Language

Setup asks for the CLI language first:

```text
1. English
2. 简体中文
```

English is the default. The chosen language is saved in config:

```toml
[cli]
language = "en"
```

Use `language = "zh-CN"` for Simplified Chinese. You can also set
`AGENTS_NOTIFIER_LANGUAGE=zh-CN` before running setup to make Chinese the default selection.
Setup prompts and setup confirmation output use the selected language.

## Agent

Choose the agent Agents Notifier should watch:

```text
1. Codex Desktop
2. Codex CLI
3. Claude Code
4. Cursor CLI
5. OpenCode CLI
6. OpenClaw
7. Hermes Agent CLI
8. GitHub Copilot CLI
9. Gemini CLI
10. Aider
```

Codex Desktop is offered on macOS and Windows. On Linux, setup starts at Codex CLI and offers the hook-based CLI sources.

## Provider

Choose where notifications should go:

```text
1. ntfy
2. Slack
3. Discord
4. Pushover
5. Feishu/Lark custom bot
6. Webhook
7. Telegram
8. WhatsApp
9. WeChat
10. Microsoft Teams
11. Email SMTP
```

Provider guides:

- [Feishu/Lark Custom Bot](providers/feishu-lark-custom-bot.md)
- [ntfy](providers/ntfy.md)
- [Pushover](providers/pushover.md)
- [Slack](providers/slack.md)
- [Discord](providers/discord.md)
- [Telegram](providers/telegram.md)
- [WhatsApp](providers/whatsapp.md)
- [WeChat](providers/weixin.md)
- [Microsoft Teams](providers/microsoft-teams.md)
- [Email SMTP](providers/email-smtp.md)
- [Webhook](providers/webhook.md)

## Answer Detail

Choose how much answer text notifications include:

```text
1. Preview (Recommended)
2. Full Answer
```

Press Enter to keep `Preview`.
Full Answer includes the visible assistant answer and omits Codex App control directives.

Answer detail is only configurable for providers without a small message size limit or delivery guard.

Agents Notifier fixes answer detail to `Preview` for:

- ntfy, because ntfy has a configurable message body size limit that defaults to 4K.
- Pushover, because Pushover messages are limited to 1024 characters.
- Slack, because Slack has documented message length and truncation limits.
- Discord, because Discord webhook content is limited to 2000 characters.
- Telegram, because Telegram Bot API text messages are limited to 4096 characters.
- WhatsApp, because Agents Notifier uses a 4096-character guard for WhatsApp text bodies.
- WeChat, because Agents Notifier uses a 3800-character guard for WeChat iLink text messages.
- Microsoft Teams, because Teams webhook messages have a documented 28 KB size limit.

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

Prompt detail is only configurable for providers without a small message size limit or delivery guard.

Agents Notifier disables prompt detail for:

- ntfy, because ntfy has a configurable message body size limit that defaults to 4K.
- Pushover, because Pushover messages are limited to 1024 characters.
- Slack, because Slack has documented message length and truncation limits.
- Discord, because Discord webhook content is limited to 2000 characters.
- Telegram, because Telegram Bot API text messages are limited to 4096 characters.
- WhatsApp, because Agents Notifier uses a 4096-character guard for WhatsApp text bodies.
- WeChat, because Agents Notifier uses a 3800-character guard for WeChat iLink text messages.
- Microsoft Teams, because Teams webhook messages have a documented 28 KB size limit.

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
