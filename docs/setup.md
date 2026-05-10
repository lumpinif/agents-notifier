# Setup

中文文档：[setup.zh-CN.md](setup.zh-CN.md)

Use setup to create or replace the local config, start the service, and send a test notification.

```bash
agents-notifier setup
```

If a config already exists, setup uses its current answers as defaults. Press Enter on any prompt
to keep the shown default. Webhook URLs are shown by host only, and signing secrets are shown only
as configured.

For a Feishu/Lark signing secret, type `none` to clear the existing secret.

## Agent

Choose the agent Agents Notifier should watch:

```text
1. Codex Desktop
2. Codex CLI
3. Claude Code
```

## Answer Detail

Choose how much answer text notifications include:

```text
1. Preview (Recommended)
2. Full Answer
```

Press Enter to keep `Preview`.

Manual config:

```toml
[notification]
answer_detail = "preview"
```

To send full answers:

```toml
[notification]
answer_detail = "full"
```

After manual edits, restart the service:

```bash
agents-notifier stop
agents-notifier start
```

## Provider

Choose where notifications should go:

```text
1. ntfy
2. Feishu/Lark custom bot
3. Webhook
```

Provider guides:

- [Feishu/Lark Custom Bot](providers/feishu-lark-custom-bot.md)
- [ntfy](providers/ntfy.md)
- [Webhook](providers/webhook.md)

## Result

Setup writes:

```text
~/.config/agents-notifier/config.toml
```

Then it starts the macOS LaunchAgent service and sends a test notification through the same route.
