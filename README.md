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

