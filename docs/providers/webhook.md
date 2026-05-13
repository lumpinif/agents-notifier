# Webhook

中文文档：[webhook.zh-CN.md](webhook.zh-CN.md)

Use webhook when you want Agents Router to POST every `Signal` to your own endpoint.

This is the clean integration path for internal tools, automations, dashboards, and custom notification bridges.

## Payload

Agents Router sends the full `Signal` as JSON:

```json
{
  "schema_version": 2,
  "id": "signal-1",
  "source": {
    "id": "codex_cli",
    "source_type": "codex_cli"
  },
  "event": {
    "kind": "custom"
  },
  "display": {
    "title": "Codex",
    "summary": "Ready for review."
  },
  "links": [],
  "timestamp": "2026-05-08T12:00:00Z",
  "metadata": {}
}
```

Your endpoint must return a `2xx` status.

Anything else is treated as a failed delivery.

## What You Need

- An HTTPS endpoint that accepts `POST`.
- A stable URL.
- Agents Router installed.

Local test URLs such as `http://127.0.0.1:8080/hook` are accepted during setup. Remote HTTP URLs are rejected.

## 1. Prepare the Endpoint

Your endpoint should accept:

```text
POST /your/path
Content-Type: application/json
```

Start simple. Log the request body first. Add processing after you see the first signal.

## 2. Connect Agents Router

Run:

```bash
agents-router setup
```

Choose:

```text
Webhook
```

Paste the HTTPS endpoint URL.

Agents Router stores the provider, starts the local service, and sends a test JSON payload through the same service route used by real agent events.

## Manual Config

Webhook is configured in:

```text
~/.config/agents-router/config.toml
```

Use `url` for the local service:

```toml
[[providers]]
id = "debug_webhook"
type = "webhook"
url = "https://example.com/agents-router"

[[routes]]
sources = ["codex_desktop", "codex_cli"]
providers = ["debug_webhook"]
```

The running service automatically reloads valid config changes. If it is not running, start it:

```bash
agents-router start
```

Advanced: `url_env` is supported, but only use it when the environment variable is visible to the running service. For normal setup, `url` is simpler and more predictable.

## 3. Confirm

Trigger a test notification or wait for a coding agent event.

Your endpoint should receive one JSON payload.

## If It Does Not Show Up

Check these first:

- The endpoint is reachable from your computer.
- The endpoint returns `2xx`.
- If you use `url_env`, the environment variable is visible to the running service.
- The route includes the source you expect.
- The local service is running:

```bash
agents-router status
```

## Security

Treat the webhook URL like a secret.

Use HTTPS.

Do not log tokens, secrets, or full webhook URLs.
