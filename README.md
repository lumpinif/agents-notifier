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

Start the local service:

```bash
agents-notifier start
```

On first run, `start` guides you through a minimal ntfy setup, writes `~/.config/agents-notifier/config.toml`, starts the background service, asks you to subscribe on your phone, and sends a test notification through the running service.

If the service is already running, `start` is safe to run again. It reuses the existing config and topic, prints the current phone subscription details, and can send another test notification. Your phone only needs a new subscription if you change the ntfy topic in the config.

ntfy notifications are sent with high priority so mobile clients are more likely to show a banner, play a sound, and vibrate.

If a previous service process exited unexpectedly, `start` cleans up stale local service files and starts a fresh background service.

The generated config looks like this:

```toml
schema_version = 1

[[sources]]
id = "codex_desktop"
type = "codex_desktop"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "phone"
type = "ntfy"
server = "https://ntfy.sh"
topic = "my-codex-alerts"

[[providers]]
id = "debug"
type = "webhook"
url_env = "AGENTS_NOTIFIER_WEBHOOK_URL"

[[routes]]
sources = ["codex_desktop", "codex_cli"]
providers = ["phone", "debug"]
```

Run the service in the foreground for debugging:

```bash
agents-notifier watch
```

Stop the background service:

```bash
agents-notifier stop
```

Submit one event from a CLI runtime hook:

```bash
agents-notifier emit \
  --source codex_cli \
  --title "Codex" \
  --body "Codex sent a notification."
```

`emit` submits the event to the running local service. It does not read provider config and does not send to `ntfy` or `webhook` by itself.

Use `--config <path>` with `start` or `watch` to run with a different config file.

Webhook providers receive the full `Signal` JSON. Only use webhook URLs you trust.
