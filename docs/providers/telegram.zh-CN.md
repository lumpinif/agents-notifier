# Telegram

English documentation: [telegram.md](telegram.md)

当你想把 Agents Notifier 通知发到一个 Telegram 私聊、群组或频道时，就用 Telegram。

Agents Notifier 使用 Telegram Bot API 的 `sendMessage` 方法，只发送纯文本。

## 官方链接

- [Telegram Bot API](https://core.telegram.org/bots/api)
- [sendMessage](https://core.telegram.org/bots/api#sendmessage)
- [BotFather](https://core.telegram.org/bots/features#botfather)

## 你需要准备

- 一个 Telegram 账号。
- 从 BotFather 创建出来的 Telegram bot token。
- 一个目标 chat id、group id、channel id，或公开频道 username。
- 已安装 Agents Notifier。

## 1. 创建 Telegram Bot

在 Telegram 里打开 BotFather，创建一个 bot：

```text
/newbot
```

复制 bot token。它看起来像这样：

```text
123456789:AAExampleToken
```

把这个 token 当成 secret。

## 2. 找到 Chat ID

如果是私聊，先给你的 bot 发任意一条消息。然后把 token 换进去，在浏览器里打开：

```text
https://api.telegram.org/bot<your bot token>/getUpdates
```

在返回结果里找 `message.chat.id`。

如果是公开频道，可以直接用频道 username：

```text
@channelusername
```

bot 必须能在目标 chat 或 channel 里发消息。

## 3. 连接 Agents Notifier

运行：

```bash
agents-notifier setup
```

选择：

```text
Telegram
```

粘贴 bot token 和 chat id。

Agents Notifier 会保存 provider、启动本地 service，并通过真实 agent 事件使用的同一条 service route 发送一条测试消息。

## Answer Detail

Agents Notifier 会对 Telegram 固定使用 `Preview` answer detail。

Telegram Bot API text message 最多 4096 个字符。完整回答可能很长，所以 Agents Notifier 会让 Telegram 通知保持短小，保证投递更可靠。

## Prompt Detail

Agents Notifier 会对 Telegram 禁用 prompt detail。

Prompt 可能很长，也可能包含私人信息。Telegram Bot API text message 最多 4096 个字符，所以 Agents Notifier 不会把 prompt 放进 Telegram 通知里。

## 手动配置

Telegram 配置在：

```text
~/.config/agents-notifier/config.toml
```

简单配置：

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
sources = ["agents_notifier"]
providers = ["telegram"]
```

进阶：支持 `bot_token_env`，但只有当这个环境变量对正在运行的本地 service 可见时才使用它。普通 setup 里，`bot_token` 更简单、更可预测。

正在运行的 service 会自动加载有效的 config 修改。如果 service 没有运行，启动它：

```bash
agents-notifier start
```

## 限制

Agents Notifier 通过 `sendMessage` 发送 Telegram 消息，字段是 `chat_id` 和 `text`。

如果某条格式化后的 Telegram 通知太长，Agents Notifier 会在发送前让这次 Telegram 投递失败。它不会偷偷截断你的消息。

Agents Notifier 会对 Telegram 始终使用 `Preview` answer detail。

## 如果收不到

先检查这些：

- bot token 是否完全正确。
- chat id 是否完全正确。
- 如果是私聊，你是否已经给 bot 发过至少一条消息。
- bot 是否已经在目标 group 或 channel 里。
- bot 是否有权限在 channel 里发消息。
- 如果使用 `bot_token_env`，这个环境变量是否对正在运行的 service 可见。
- 本地 service 是否在运行：

```bash
agents-notifier status
```
