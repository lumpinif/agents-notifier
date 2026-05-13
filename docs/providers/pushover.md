# Pushover

中文文档：[pushover.zh-CN.md](pushover.zh-CN.md)

Use Pushover when you want Agents Router updates on the Pushover mobile or desktop apps.

## Official Links

- [Pushover Message API](https://pushover.net/api)
- [Pushover Apps and Devices](https://pushover.net/clients)
- [Pushover Dashboard](https://pushover.net/)
- [Pushover API Knowledge Base](https://support.pushover.net/s1-pushover/knowledgebase/default/c2-api-integration)

## What You Need

- A Pushover account.
- The Pushover app signed in on at least one device.
- A Pushover application API token.
- Your Pushover user key, or a group key.
- Agents Router installed.

## 1. Create a Pushover Application

Open the Pushover dashboard and create an application.

Copy the application API token. It is a private 30-character value.

## 2. Copy Your User Key

Copy your user key from the Pushover dashboard.

You can also use a Pushover group key. Agents Router treats user keys and group keys the same way because the Pushover API does.

Keep both the application token and user key private.

## 3. Connect Agents Router

Run:

```bash
agents-router setup
```

Choose:

```text
Pushover
```

Paste:

- Pushover application API token
- Pushover user or group key

Optional fields:

- Device name, or press Enter to send to all devices.
- Sound name, or press Enter to use your Pushover account default.

## Answer Detail

Agents Router fixes answer detail to `Preview` for Pushover.

Pushover messages are limited to 1024 characters. Full answers can be long, so Agents Router keeps Pushover notifications short for reliable delivery.

## Prompt Detail

Agents Router disables prompt detail for Pushover.

Pushover messages are limited to 1024 characters. Prompts can be long, so Agents Router keeps prompts out of Pushover notifications to avoid unreliable delivery.

## Manual Config

Pushover is configured in:

```text
~/.config/agents-router/config.toml
```

Simple config:

```toml
[[providers]]
id = "pushover"
type = "pushover"
app_token = "your-application-api-token"
user_key = "your-user-or-group-key"

[[routes]]
sources = ["codex_desktop"]
providers = ["pushover"]

[[routes]]
sources = ["agents_router"]
providers = ["pushover"]
```

Optional device and sound:

```toml
[[providers]]
id = "pushover"
type = "pushover"
app_token = "your-application-api-token"
user_key = "your-user-or-group-key"
device = "iphone"
sound = "pushover"
```

Advanced: `app_token_env` and `user_key_env` are supported, but only use them when those environment variables are visible to the running local service.

The running service automatically reloads valid config changes. If it is not running, start it:

```bash
agents-router start
```

## Limits

Pushover limits message titles to 250 characters and message bodies to 1024 characters.

Agents Router fails the Pushover delivery before sending when a notification is too large. It does not silently cut your message.

Agents Router always uses `Preview` answer detail for Pushover.

## If It Does Not Show Up

Check these first:

- The application API token is exact.
- The user or group key is exact.
- At least one Pushover device is active.
- If you set `device`, the device name is exact.
- If you use env vars, they are visible to the running service.
- The local service is running:

```bash
agents-router status
```
