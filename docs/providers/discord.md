# Discord

中文文档：[discord.zh-CN.md](discord.zh-CN.md)

Use Discord when you want Agents Router updates in one Discord channel.

Agents Router uses Discord Incoming Webhooks. One webhook posts to one channel.

## Official Links

- [Discord Webhook Resource](https://docs.discord.com/developers/resources/webhook)
- [Discord Webhooks Overview](https://docs.discord.com/developers/platform/webhooks)
- [Discord Rate Limits](https://docs.discord.com/developers/topics/rate-limits)

## What You Need

- A Discord server and channel.
- Permission to manage webhooks in that channel.
- A Discord channel webhook URL.
- Agents Router installed.

## 1. Create a Discord Channel Webhook

Open the channel settings in Discord.

Create a webhook for the channel that should receive notifications.

Copy the webhook URL. It looks like this:

```text
https://discord.com/api/webhooks/123456789012345678/your-webhook-token
```

Treat this URL like a secret.

## 2. Connect Agents Router

Run:

```bash
agents-router setup
```

Choose:

```text
Discord
```

Paste the Discord webhook URL.

Agents Router stores the provider, starts the local service, and sends a test message through the same service route used by real agent events.

## Answer Detail

Agents Router fixes answer detail to `Preview` for Discord.

Discord webhook `content` is limited to 2000 characters. Full answers can be long, so Agents Router keeps Discord notifications short for reliable delivery.

## Prompt Detail

Agents Router disables prompt detail for Discord.

Discord webhook `content` is limited to 2000 characters. Prompts can be long and private, so Agents Router keeps prompts out of Discord notifications.

## Manual Config

Discord is configured in:

```text
~/.config/agents-router/config.toml
```

Simple config:

```toml
[[providers]]
id = "discord"
type = "discord"
url = "https://discord.com/api/webhooks/123456789012345678/your-webhook-token"

[[routes]]
sources = ["codex_desktop"]
providers = ["discord"]

[[routes]]
sources = ["agents_router"]
providers = ["discord"]
```

Advanced: `url_env` is supported, but only use it when the environment variable is visible to the running local service. For normal setup, `url` is simpler and more predictable.

The running service automatically reloads valid config changes. If it is not running, start it:

```bash
agents-router start
```

## Limits

Discord limits webhook `content` to 2000 characters.

Agents Router sends with `wait=true` so Discord confirms the created message. It also disables automatic mentions, so a notification body containing `@everyone` does not ping a server.

If a formatted Discord notification is too long, Agents Router fails the Discord delivery before sending. It does not silently cut your message.

Agents Router always uses `Preview` answer detail for Discord.

## If It Does Not Show Up

Check these first:

- The webhook URL is exact.
- The webhook still exists in the Discord channel.
- The channel still allows webhook posts.
- If you use `url_env`, the environment variable is visible to the running service.
- The local service is running:

```bash
agents-router status
```
