# agents-notifier

`agents-notifier` is an open-source, local-first notification router for AI coding agents.

It solves a simple problem: coding agents often need your attention while you are away from the app. This project forwards those local agent notifications to the notification channels you already use, without taking over your workflow.

## Why

AI coding agents are useful, but they still depend on the developer at key moments. A task may finish, fail, or ask for attention while the user is in another app or away from the desk.

The goal of `agents-notifier` is to make those moments visible.

It does not try to become an IDE, a remote desktop, or an agent operator. It stays small: listen locally, normalize the signal, route the notification.

## Long-Term Plan

The project starts with a narrow local path and grows horizontally.

Planned direction:

- Support more local coding agents.
- Support more notification providers.
- Keep the core event model agent-agnostic.
- Keep integrations as adapters around a small routing core.
- Stay local-first by default.

The first versions are intentionally small. The architecture is designed so new sources and providers can be added without changing the core.

## Architecture

The service owns the provider delivery path:

```text
Source Adapter -> Signal -> Router -> Provider Adapter
```

A source adapter listens to one local agent or tool and creates a generic `Signal`.

The router decides where that signal should go.

A provider adapter sends the signal to a notification channel.

Sources do not know providers. Providers do not know sources. Short-lived hook commands submit events to the local service; they do not send provider notifications directly. The core stays independent of any specific agent or notification platform.

## Phase 1 Usage

Set up Agents Notifier:

```bash
agents-notifier setup
```

`setup` asks which agent to watch, asks where notifications should go, writes `~/.config/agents-notifier/config.toml`, starts the background service, and sends a test notification through the running service.

The guided setup supports these agents:

- Codex Desktop, for completed jobs written by the Codex Desktop app.
- Codex CLI, for hook events submitted to the local service with `agents-notifier emit`.

It supports these notification providers:

- `ntfy`, for phone notifications through a topic subscription.
- Feishu/Lark Custom Bot, for posting notifications into one group chat through a custom bot webhook.

On macOS, the service is managed by a LaunchAgent at `~/Library/LaunchAgents/com.agents-notifier.service.plist`. The LaunchAgent runs `agents-notifier watch --config <path>` as the long-running worker, so the service can keep running after the terminal closes and can start again when you log in.

To start the service later with the existing config, run:

```bash
agents-notifier start
```

If the service is already running, `start` is safe to run again. It reuses the existing config, prints the current service and notification target details, and can send another test notification. Your phone only needs a new ntfy subscription if you change the ntfy topic in the config.

If there is no config yet and `start` is run in an interactive terminal, it enters the same setup flow. In non-interactive shells, missing config is an error and the command tells you to run `agents-notifier setup`.

To switch agents or providers later, run `agents-notifier setup` again. It rewrites the local config, restarts the LaunchAgent-managed service, and sends a test notification.

ntfy notifications are sent with high priority so mobile clients are more likely to show a banner, play a sound, and vibrate.

If a previous pre-LaunchAgent background process is still present, `start` safely stops or cleans up that legacy runtime before starting the LaunchAgent-managed service.

For Codex Desktop, Phase 1 watches local Codex session files under `~/.codex/sessions` and forwards new `task_complete` events. This catches completed jobs even when the Codex window is focused and macOS does not create a system notification. The source reads only the completion event and lightweight session metadata needed for the message: project name, session title, final reply preview, duration, branch, and completion time. It does not log prompts, full replies, tool output, code, or full session content.

A config can look like this:

```toml
schema_version = 1

[[sources]]
id = "codex_desktop"
type = "codex_desktop"

[[sources]]
id = "agents_notifier"
type = "agents_notifier"

[[providers]]
id = "phone"
type = "ntfy"
server = "https://ntfy.sh"
topic = "my-codex-alerts"

[[providers]]
id = "debug"
type = "webhook"
url_env = "AGENTS_NOTIFIER_WEBHOOK_URL"

[[providers]]
id = "work_chat"
type = "feishu_lark"
url_env = "AGENTS_NOTIFIER_FEISHU_LARK_WEBHOOK_URL"
secret_env = "AGENTS_NOTIFIER_FEISHU_LARK_SECRET"

[[routes]]
sources = ["codex_desktop", "agents_notifier"]
providers = ["phone", "debug", "work_chat"]
```

For Codex CLI instead of Codex Desktop, use a `codex_cli` source and route it to the providers you want. `agents_notifier` is the service's own test source; setup includes it so test notifications use the same running service and provider route as real agent notifications.

Run the service in the foreground for debugging:

```bash
agents-notifier watch
```

Show service status:

```bash
agents-notifier status
```

Stop the service:

```bash
agents-notifier stop
```

Submit one event from a CLI runtime hook:

```bash
agents-notifier emit \
  --source codex_cli \
  --title "Codex" \
  --body "Codex finished a job."
```

`emit` submits the event to the running local service. It does not read provider config and does not send to `ntfy`, Feishu/Lark, or `webhook` by itself.

Use `--config <path>` with `start` or `watch` to run with a different config file. `start` installs or updates the LaunchAgent to use that config path; `watch` runs the worker directly in the foreground and does not install service files.

Feishu/Lark Custom Bot uses the official custom bot webhook format and sends a plain text message. Message time is shown in the Mac's local time with a numeric UTC offset. If Signature Verification is enabled for the bot, set `secret` or `secret_env`.

Codex Desktop completion messages are formatted like this:

```text
Codex Desktop finished a job.

Project: agents-notifier
Session: agents-notifier sync report
Duration: 1m 32s
Branch: main
Time: 2026-05-10 01:35:42 +08:00

Preview: Updated the Codex Desktop default notification text...
```

Webhook providers receive the full `Signal` JSON. Only use webhook URLs you trust.
