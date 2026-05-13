# Telegram

中文文档：[telegram.zh-CN.md](telegram.zh-CN.md)

Use Telegram when you want Agents Router updates in one Telegram chat, group, or channel.

Agents Router uses the Telegram Bot API `sendMessage` method. It sends plain text only.

## Official Links

- [Telegram Bot API](https://core.telegram.org/bots/api)
- [sendMessage](https://core.telegram.org/bots/api#sendmessage)
- [BotFather](https://core.telegram.org/bots/features#botfather)

## What You Need

- A Telegram account.
- A Telegram bot token from BotFather.
- One target chat id, group id, channel id, or public channel username.
- Agents Router installed.

## 1. Create a Telegram Bot

Open BotFather in Telegram and create a bot:

```text
/newbot
```

Copy the bot token. It looks like this:

```text
123456789:AAExampleToken
```

Treat this token like a secret.

## 2. Find the Chat ID

For a private chat, send any message to your bot. Then open this URL in a browser after replacing the token:

```text
https://api.telegram.org/bot<your bot token>/getUpdates
```

Look for `message.chat.id` in the response.

For a public channel, you can use the channel username:

```text
@channelusername
```

The bot must be able to post in the target chat or channel.

## 3. Connect Agents Router

Run:

```bash
agents-router setup
```

Choose:

```text
Telegram
```

Paste the bot token and chat id.

Agents Router stores the provider, starts the local service, and sends a test message through the same service route used by real agent events.

## Answer Detail

Agents Router fixes answer detail to `Preview` for Telegram.

Telegram Bot API text messages are limited to 4096 characters. Full answers can be long, so Agents Router keeps Telegram notifications short for reliable delivery.

## Prompt Detail

Agents Router disables prompt detail for Telegram.

Prompts can be long and private. Telegram Bot API text messages are limited to 4096 characters, so Agents Router keeps prompts out of Telegram notifications.

## Manual Config

Telegram is configured in:

```text
~/.config/agents-router/config.toml
```

Simple config:

```toml
[[providers]]
id = "telegram"
type = "telegram"
bot_token = "<your Telegram bot token>"
chat_id = "123456789"

[[routes]]
sources = ["codex_desktop"]
providers = ["telegram"]

[[routes]]
sources = ["agents_router"]
providers = ["telegram"]
```

Advanced: `bot_token_env` is supported, but only use it when the environment variable is visible to the running local service. For normal setup, `bot_token` is simpler and more predictable.

The running service automatically reloads valid config changes. If it is not running, start it:

```bash
agents-router start
```

## Limits

Agents Router sends Telegram messages through `sendMessage` with `chat_id` and `text`.

If a formatted Telegram notification is too long, Agents Router fails the Telegram delivery before sending. It does not silently cut your message.

Agents Router always uses `Preview` answer detail for Telegram.

## If It Does Not Show Up

Check these first:

- The bot token is exact.
- The chat id is exact.
- The bot has received at least one message from you for private chats.
- The bot is a member of the group or channel.
- The bot has permission to post in the channel.
- If you use `bot_token_env`, the environment variable is visible to the running service.
- The local service is running:

```bash
agents-router status
```
