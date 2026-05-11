# Weixin

中文文档：[weixin.zh-CN.md](weixin.zh-CN.md)

Use Weixin when you want Agents Notifier updates sent to one personal WeChat chat through an iLink bot connection.

This is personal WeChat through the official Tencent/Weixin OpenClaw iLink bot channel. It is not WeChat Work, and it is not WhatsApp.

The WeChat chat currently appears as `WeixinClawBot`. Agents Notifier cannot rename this bot. The bot display name is controlled by the Weixin iLink/OpenClaw channel, not by this local app.

## What You Need

- A WeChat mobile app account that can scan the iLink QR code.
- Network access to the iLink gateway.
- Agents Notifier installed.

By default, Agents Notifier uses:

```text
https://ilinkai.weixin.qq.com
```

Setup asks for two iLink connection settings:

- `Weixin gateway URL`: the Weixin iLink gateway. Most users should press Enter and keep the default. Only change it if your iLink provider gave you another URL.
- `Optional Weixin route tag`: an advanced optional routing value for iLink `SKRouteTag`. Most users should press Enter and skip it. Only enter a value if your iLink provider gave you one.

## 1. Connect Agents Notifier

Run:

```bash
agents-notifier setup
```

Choose:

```text
Weixin
```

Agents Notifier supports two setup paths:

- Scan a WeChat QR code to get an iLink token.
- Paste an existing iLink token.

After the token is ready, Agents Notifier asks you to open the bot chat that appeared in WeChat, such as `WeixinClawBot`, and manually send this short message in that bot chat:

```text
hi
```

Send it in WeChat, not in Terminal. That message lets Agents Notifier discover the `recipient_user_id` and `context_token` required by iLink `sendmessage`.

Agents Notifier then writes the provider config, starts the local service, and sends a test notification through the same route used by real agent events.

## How It Works

Setup uses the iLink bot QR login flow:

1. Agents Notifier requests a QR code from `/ilink/bot/get_bot_qrcode`.
2. You scan the QR code in WeChat.
3. Agents Notifier polls `/ilink/bot/get_qrcode_status` until iLink returns the bot token.
4. You send `hi` in the `WeixinClawBot` chat.
5. Agents Notifier polls `/ilink/bot/getupdates` during setup only, reads that message, and stores the recipient id and context token.

At runtime, Agents Notifier does not poll your WeChat messages. It only sends notifications with:

```text
POST {base_url}/ilink/bot/sendmessage
```

The runtime request uses the stored token, recipient id, and context token. If iLink returns a clear non-zero `ret` or `errcode`, Agents Notifier treats the delivery as failed and keeps that error visible.

## Answer Detail

Agents Notifier fixes answer detail to `Preview` for Weixin.

Weixin notifications should stay short. Agents Notifier uses a 3800-character local guard for Weixin iLink text messages and fails before sending if a formatted notification is too long.

## Prompt Detail

Agents Notifier disables prompt detail for Weixin.

Prompts can be long and private, so Agents Notifier keeps prompts out of Weixin notifications.

## Manual Config

Weixin is configured in:

```text
~/.config/agents-notifier/config.toml
```

Simple config:

```toml
[[providers]]
id = "weixin"
type = "weixin"
base_url = "https://ilinkai.weixin.qq.com"
token = "<your iLink bot token>"
recipient_user_id = "<recipient iLink user id>"
context_token = "<recipient context token>"
# route_tag = "<optional advanced SKRouteTag>"

[[routes]]
sources = ["codex_desktop", "agents_notifier"]
providers = ["weixin"]
```

Advanced: `token_env` and `context_token_env` are supported, but only use them when the environment variables are visible to the running local service. For normal setup, inline values are simpler and more predictable.

Restart the service after manual edits:

```bash
agents-notifier start
```

## Limits

Agents Notifier sends plain text only. It does not send images, files, audio, stickers, or interactive cards through Weixin.

Agents Notifier cannot rename the Weixin bot. The bot chat name is controlled by the official Weixin iLink/OpenClaw channel and currently appears as `WeixinClawBot`.

Agents Notifier does not create a custom WeChat bot, official account, mini program, or WeChat Work app. It uses the existing Weixin iLink bot channel.

`base_url` must be an HTTPS origin such as `https://ilinkai.weixin.qq.com`.

`token`, `recipient_user_id`, `context_token`, and `route_tag` must not contain whitespace.

If iLink returns an expired or invalid `context_token`, Agents Notifier fails the Weixin delivery and keeps the error visible. It does not silently retry by polling your WeChat messages in the background.

If the context token expires, run `agents-notifier setup` again and choose Weixin to relink the chat.

## If It Does Not Show Up

Check these first:

- The Weixin iLink token is valid.
- The linked WeChat account sent `hi` to the `WeixinClawBot` chat during setup.
- The `context_token` has not expired.
- The `base_url` and optional `route_tag` match your iLink provider.
- If you use `token_env` or `context_token_env`, the environment variable is visible to the running service.
- The local service is running:

```bash
agents-notifier status
```
