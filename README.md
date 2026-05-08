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

The core pipeline is:

```text
Source Adapter -> Signal -> Router -> Provider Adapter
```

A source adapter listens to one local agent or tool and creates a generic `Signal`.

The router decides where that signal should go.

A provider adapter sends the signal to a notification channel.

Sources do not know providers. Providers do not know sources. The core stays independent of any specific agent or notification platform.

## Phase 1 Usage

Create a config file:

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

Run the Codex Desktop watcher:

```bash
agents-notifier watch --config ~/.config/agents-notifier/config.toml
```

Run it in the background:

```bash
agents-notifier watch --background --config ~/.config/agents-notifier/config.toml
```

Stop the background watcher:

```bash
agents-notifier stop
```

Send one notification from a CLI hook:

```bash
agents-notifier emit \
  --config ~/.config/agents-notifier/config.toml \
  --source codex_cli \
  --title "Codex" \
  --body "Codex sent a notification."
```

Webhook providers receive the full `Signal` JSON. Only use webhook URLs you trust.
