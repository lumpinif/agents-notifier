# agents-notifier

Set it up in 2-3 minutes and get local coding agent updates on your phone, Feishu/Lark, or webhook.

---

中文文档：[docs/README.zh-CN.md](docs/README.zh-CN.md)

[Quick start](#-install---step-1)

> _"Imagine [Codex Desktop App](https://openai.com/codex/) keeps working while you make coffee or do laundry._
>
> _The moment the job finishes, you get a notification and know it is time to come back."_

⚡ Local-only notifications for AI coding agents.

Built for local agents like [Codex Desktop](https://openai.com/codex/), [Codex CLI](https://github.com/openai/codex), and [Claude Code](https://claude.com/product/claude-code).

Built in Rust 🦀. Fast, small, and quiet in the background.

```text
Agent on your Mac -> Agents Notifier -> Your provider
```

No cloud account. No hosted backend. No extra dashboard.

## ✅ Support

Agents:

- [Codex Desktop](https://openai.com/codex/) on macOS
- [Codex CLI](https://github.com/openai/codex) through hooks
- [Claude Code](https://claude.com/product/claude-code) through hooks
- More local agents soon

Providers (Where do you want to get the notification?):

- ntfy
- Feishu/Lark Custom Bot
- Webhook
- More providers soon

## 🔒 Privacy

Agents Notifier runs locally.

Your data does not go to an Agents Notifier cloud.

Notifications go directly from your Mac to your provider.

For Codex Desktop, it reads only completion data needed for the notification:

- project
- project path
- session
- Codex thread link
- duration
- branch
- time
- final answer preview by default, or full answer when enabled
- prompt only when explicitly enabled
- Mac computer name

In Feishu/Lark, notifications are sent as Codex-colored interactive cards with a clickable Open in Codex button.
The button opens a local browser URL first, then hands off to Codex Desktop.

## ⚙️ Install - Step 1

Pick one install method. That is enough.

Recommended:

Copy this into your Terminal:

```bash
curl -fsSL https://raw.githubusercontent.com/lumpinif/agents-notifier/main/install.sh | sh
agents-notifier setup
```

From source:

```bash
git clone https://github.com/lumpinif/agents-notifier.git
cd agents-notifier
cargo install --path .
agents-notifier setup
```

## 🚀 Setup - Step 2

```bash
agents-notifier setup
```

It asks four questions:

1. Which agent should it watch?
2. Answer detail?
3. Include your prompt?
4. Where should notifications go?

Then it writes config, starts the service, and sends a test notification.

For every setup option, see [Setup](docs/setup.md).

## 🎉 That's it

On macOS, the service runs as a LaunchAgent.
To stop using the service, run `agents-notifier stop`.

Provider setup guides:

- [Feishu/Lark Custom Bot](docs/providers/feishu-lark-custom-bot.md)
- [ntfy](docs/providers/ntfy.md)
- [Webhook](docs/providers/webhook.md)

Agent setup guides:

- [Codex CLI](docs/agents/codex-cli.md)
- [Claude Code](docs/agents/claude-code.md)

## 🧹 Uninstall

Remove Agents Notifier cleanly:

```bash
agents-notifier uninstall
```

## 🧭 Commands

```bash
agents-notifier setup    # set up or change agent/provider
agents-notifier start    # start existing service
agents-notifier status   # check service status
agents-notifier stop     # stop service
agents-notifier uninstall # remove service, config, logs, state, and binary
agents-notifier watch    # foreground debug worker
```

CLI agent hooks can submit events with:

```bash
agents-notifier emit \
  --source codex_cli \
  --title "Codex" \
  --body "Ready for review."
```

```bash
agents-notifier emit \
  --source claude_code \
  --title "Claude Code" \
  --body "Claude Code finished a task."
```

`emit` only talks to the local service. Providers are sent by the service.

## ✨ Example

```text
Codex Desktop

Project: agents-notifier
Session: README polish
Open in Codex: codex://threads/019e1049-2d6d-7de2-bcdf-f47346930b71
Duration: 1m 32s
Branch: main
Time: 2026-05-10 01:35:42 +08:00

Preview: Updated the README with a clearer setup flow...
```

## 📝 Config

```text
~/.config/agents-notifier/config.toml
```

Most users should use `agents-notifier setup`.

## 🧩 Core

```text
Source -> Signal -> Router -> Provider
```

Simple core. More agents and providers over time.

Contributions welcome. 💛
