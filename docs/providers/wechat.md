# WeChat

中文文档：[wechat.zh-CN.md](wechat.zh-CN.md)

Use WeChat when you want Agents Router updates sent to one personal WeChat chat through an iLink bot connection.

This is personal WeChat through the official Tencent/WeChat OpenClaw iLink bot channel. It is not WeChat Work, and it is not WhatsApp.

The WeChat chat currently appears as `WeixinClawBot`. Agents Router cannot rename this bot. The bot display name is controlled by the WeChat iLink/OpenClaw channel, not by this local app.

## What You Need

- A WeChat mobile app account that can scan the iLink QR code.
- Network access to the iLink gateway.
- Agents Router installed.

By default, Agents Router uses:

```text
https://ilinkai.weixin.qq.com
```

Setup asks for two iLink connection settings:

- `WeChat gateway URL`: the WeChat iLink gateway. Most users should press Enter and keep the default. Only change it if your iLink provider gave you another URL.
- `Optional WeChat route tag`: an advanced optional routing value for iLink `SKRouteTag`. Most users should press Enter and skip it. Only enter a value if your iLink provider gave you one.

## 1. Connect Agents Router

Run:

```bash
agents-router setup
```

Choose:

```text
WeChat
```

Agents Router supports two setup paths:

- Scan a WeChat QR code to get an iLink token.
- Paste an existing iLink token.

After the token is ready, Agents Router asks you to open the bot chat that appeared in WeChat, such as `WeixinClawBot`, and manually send this short message in that bot chat:

```text
hi
```

Send it in WeChat, not in Terminal. That message lets Agents Router discover the `recipient_user_id` and `context_token` required by iLink `sendmessage`.

Agents Router then writes the provider config, starts the local service, and sends a test notification through the same provider delivery path used by real agent events.

## How It Works

Setup uses the iLink bot QR login flow:

1. Agents Router requests a QR code from `/ilink/bot/get_bot_qrcode`.
2. You scan the QR code in WeChat.
3. Agents Router polls `/ilink/bot/get_qrcode_status` until iLink returns the bot token.
4. You send `hi` in the `WeixinClawBot` chat.
5. Agents Router polls `/ilink/bot/getupdates` during setup only, reads that message, and stores the recipient id and context token.

At runtime, Agents Router does not poll your WeChat messages. It only sends notifications with:

```text
POST {base_url}/ilink/bot/sendmessage
```

The runtime request uses the stored token, recipient id, and context token. If iLink returns a clear non-zero `ret` or `errcode`, Agents Router treats the delivery as failed and keeps that error visible.

## Answer Detail

Agents Router fixes answer detail to `Preview` for WeChat.

WeChat notifications should stay short. Agents Router uses a 3800-character local guard for WeChat iLink text messages and fails before sending if a formatted notification is too long.

## Prompt Detail

Agents Router disables prompt detail for WeChat.

Prompts can be long and private, so Agents Router keeps prompts out of WeChat notifications.

## Manual Config

WeChat is configured in:

```text
~/.config/agents-router/config.toml
```

Simple config:

```toml
[[providers]]
id = "wechat"
type = "wechat"
base_url = "https://ilinkai.weixin.qq.com"
token = "<your iLink bot token>"
recipient_user_id = "<recipient iLink user id>"
context_token = "<recipient context token>"
# route_tag = "<optional advanced SKRouteTag>"

[[routes]]
sources = ["codex_desktop"]
providers = ["wechat"]

[[routes]]
sources = ["agents_router"]
providers = ["wechat"]
```

Advanced: `token_env` and `context_token_env` are supported, but only use them when the environment variables are visible to the running local service. For normal setup, inline values are simpler and more predictable.

The running service automatically reloads valid config changes. If it is not running, start it:

```bash
agents-router start
```

## Limits

Agents Router sends plain text only. It does not send images, files, audio, stickers, or interactive cards through WeChat.

Agents Router cannot rename the WeChat bot. The bot chat name is controlled by the official WeChat iLink/OpenClaw channel and currently appears as `WeixinClawBot`.

Agents Router does not create a custom WeChat bot, official account, mini program, or WeChat Work app. It uses the existing WeChat iLink bot channel.

`base_url` must be an HTTPS origin such as `https://ilinkai.weixin.qq.com`.

`token`, `recipient_user_id`, `context_token`, and `route_tag` must not contain whitespace.

If iLink returns an expired or invalid `context_token`, Agents Router fails the WeChat delivery and keeps the error visible. It does not silently retry by polling your WeChat messages in the background.

If the context token expires, run `agents-router setup` again and choose WeChat to relink the chat.

## If It Does Not Show Up

Check these first:

- The WeChat iLink token is valid.
- The linked WeChat account sent `hi` to the `WeixinClawBot` chat during setup.
- The `context_token` has not expired.
- The `base_url` and optional `route_tag` match your iLink provider.
- If you use `token_env` or `context_token_env`, the environment variable is visible to the running service.
- The local service is running:

```bash
agents-router status
```
