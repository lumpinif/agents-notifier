# agents-notifier

> _"Imagine your coding agent is working while you do laundry or handle chores._
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
- [Claude Code](https://claude.com/product/claude-code) soon
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
- session
- duration
- branch
- time
- short preview

## ⚙️ Install

```bash
cargo install --path .
```

## 🚀 Setup

```bash
agents-notifier setup
```

It asks two questions:

1. Which agent should it watch?
2. Where should notifications go?

Then it writes config, starts the service, and sends a test notification.

On macOS, the service runs as a LaunchAgent.

## 🧭 Commands

```bash
agents-notifier setup    # set up or change agent/provider
agents-notifier start    # start existing service
agents-notifier status   # check service status
agents-notifier stop     # stop service
agents-notifier watch    # foreground debug worker
```

Codex CLI hooks can submit events with:

```bash
agents-notifier emit \
  --source codex_cli \
  --title "Codex" \
  --body "Codex finished a job."
```

`emit` only talks to the local service. Providers are sent by the service.

## ✨ Example

```text
Codex Desktop finished a job.

Project: agents-notifier
Session: README polish
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
