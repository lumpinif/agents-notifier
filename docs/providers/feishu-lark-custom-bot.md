# Feishu/Lark Custom Bot

中文文档：[feishu-lark-custom-bot.zh-CN.md](feishu-lark-custom-bot.zh-CN.md)

2-3 minutes. One group bot. One webhook URL. Then your local coding agent updates land in Feishu or Lark.

## Official Links

- Feishu: [Custom bot usage guide](https://open.feishu.cn/document/client-docs/bot-v3/add-custom-bot)
- Lark: [Custom bot usage guide](https://open.larksuite.com/document/client-docs/bot-v3/add-custom-bot)

## What You Need

- A Feishu or Lark group.
- Permission to add a group bot.
- Agents Router installed.

## 1. Add the Bot

Open the group that should receive notifications.

Then:

```text
Group settings -> Group bot -> Add bot -> Custom Bot
```

Name it something clear:

```text
Agents Router
```

Finish the bot setup.

## 2. Copy the Webhook URL

After the bot is created, copy its webhook URL.

It should look like one of these:

```text
https://open.feishu.cn/open-apis/bot/v2/hook/...
https://open.larksuite.com/open-apis/bot/v2/hook/...
```

Keep it private. Anyone with this URL can post to that group.

## 3. Security

Recommended:

```text
Signature Verification
```

If you enable it, copy the signing secret.

Avoid keyword security for Agents Router. A keyword rule can block normal notifications unless every message contains that keyword.

## 4. Connect Agents Router

Run:

```bash
agents-router setup
```

Choose:

```text
Feishu/Lark custom bot
```

Paste the webhook URL.

If you enabled Signature Verification, paste the signing secret.  
If not, press Enter.

## 5. Confirm

Agents Router starts the local service and sends a test notification.

You should see a card in the group.

## If It Does Not Show Up

Check these first:

- The webhook URL starts with `https://open.feishu.cn/open-apis/bot/v2/hook/` or `https://open.larksuite.com/open-apis/bot/v2/hook/`.
- If Signature Verification is enabled, the signing secret is exact.
- Keyword security is disabled.
- The bot is in the group you are watching.
- The local service is running:

```bash
agents-router status
```

