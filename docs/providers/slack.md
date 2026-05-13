# Slack

中文文档：[slack.zh-CN.md](slack.zh-CN.md)

Use Slack when you want Agents Notifier updates in one Slack channel.

Agents Notifier uses Slack Incoming Webhooks. One webhook posts to one channel selected when the webhook is created.

Messages are sent as plain text.

## Official Links

- [Slack Incoming Webhooks](https://docs.slack.dev/messaging/sending-messages-using-incoming-webhooks/)
- [Slack Rate Limits](https://docs.slack.dev/apis/web-api/rate-limits/)
- [Slack Message Truncation](https://docs.slack.dev/changelog/2018-truncating-really-long-messages/)

## What You Need

- A Slack workspace.
- Permission to create or install a Slack app with Incoming Webhooks.
- One target channel.
- Agents Notifier installed.

## 1. Create a Slack Incoming Webhook

Open your Slack app settings.

Enable Incoming Webhooks, then create a webhook for the channel that should receive notifications.

Copy the webhook URL. It looks like this:

```text
https://hooks.slack.com/services/...
```

Treat this URL like a secret.

## 2. Connect Agents Notifier

Run:

```bash
agents-notifier setup
```

Choose:

```text
Slack
```

Paste the Slack webhook URL.

Agents Notifier stores the provider, starts the local service, and sends a test message through the same service route used by real agent events.

## Answer Detail

Agents Notifier fixes answer detail to `Preview` for Slack.

Slack has documented message length and truncation limits. Full answers can be long, so Agents Notifier keeps Slack notifications short for reliable delivery.

## Prompt Detail

Agents Notifier disables prompt detail for Slack.

Prompts can be long and private. Slack messages have documented length limits, so Agents Notifier keeps prompts out of Slack notifications.

## Manual Config

Slack is configured in:

```text
~/.config/agents-notifier/config.toml
```

Simple config:

```toml
[[providers]]
id = "slack"
type = "slack"
url = "<your Slack incoming webhook URL>"

[[routes]]
sources = ["codex_desktop"]
providers = ["slack"]

[[routes]]
sources = ["agents_notifier"]
providers = ["slack"]
```

Advanced: `url_env` is supported, but only use it when the environment variable is visible to the running local service. For normal setup, `url` is simpler and more predictable.

The running service automatically reloads valid config changes. If it is not running, start it:

```bash
agents-notifier start
```

## Limits

Slack recommends keeping posted message text to 4000 characters and documents truncation for very long posted messages. Agents Notifier uses the 4000-character recommendation as its delivery guard.

If a formatted Slack notification is too long, Agents Notifier fails the Slack delivery before sending. It does not silently cut your message.

Agents Notifier always uses `Preview` answer detail for Slack.

## If It Does Not Show Up

Check these first:

- The webhook URL is exact.
- The Slack app still has Incoming Webhooks enabled.
- The webhook is still connected to the target channel.
- If you use `url_env`, the environment variable is visible to the running service.
- The local service is running:

```bash
agents-notifier status
```
