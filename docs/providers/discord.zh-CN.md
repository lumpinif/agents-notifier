# Discord

English documentation: [discord.md](discord.md)

当你想把 Agents Notifier 通知发到一个 Discord channel 时，就用 Discord。

Agents Notifier 使用 Discord Incoming Webhooks。一个 webhook 会发到一个 channel。

## 官方链接

- [Discord Webhook Resource](https://docs.discord.com/developers/resources/webhook)
- [Discord Webhooks Overview](https://docs.discord.com/developers/platform/webhooks)
- [Discord Rate Limits](https://docs.discord.com/developers/topics/rate-limits)

## 你需要准备什么

- 一个 Discord server 和 channel。
- 在目标 channel 里管理 webhook 的权限。
- 一个 Discord channel webhook URL。
- 已安装 Agents Notifier。

## 1. 创建 Discord Channel Webhook

打开 Discord channel settings。

为接收通知的 channel 创建一个 webhook。

复制 webhook URL。它看起来像这样：

```text
https://discord.com/api/webhooks/123456789012345678/your-webhook-token
```

这个 URL 是 secret，不要公开。

## 2. 连接 Agents Notifier

运行：

```bash
agents-notifier setup
```

选择：

```text
Discord
```

粘贴 Discord webhook URL。

Agents Notifier 会保存 provider、启动本地 service，并通过真实 agent 事件使用的同一条 service route 发送一条测试消息。

## Answer Detail

Agents Notifier 会对 Discord 固定使用 `Preview` answer detail。

Discord webhook `content` 最多 2000 个字符。完整回答可能很长，所以 Agents Notifier 会让 Discord 通知保持短小，保证投递更可靠。

## Prompt Detail

Agents Notifier 会对 Discord 禁用 prompt detail。

Discord webhook `content` 最多 2000 个字符。Prompt 可能很长，也可能包含私人信息，所以 Agents Notifier 不会把 prompt 放进 Discord 通知里。

## 手动配置

Discord 配置在：

```text
~/.config/agents-notifier/config.toml
```

简单配置：

```toml
[[providers]]
id = "discord"
type = "discord"
url = "https://discord.com/api/webhooks/123456789012345678/your-webhook-token"

[[routes]]
sources = ["codex_desktop"]
providers = ["discord"]

[[routes]]
sources = ["agents_notifier"]
providers = ["discord"]
```

高级用法：支持 `url_env`，但只有当这个环境变量对正在运行的本机 service 可见时才使用它。普通 setup 场景下，`url` 更简单、更可预测。

手动修改后，正在运行的 service 会自动加载有效的 config 修改。如果 service 没有运行，启动它：

```bash
agents-notifier start
```

## 限制

Discord 限制 webhook `content` 最多 2000 个字符。

Agents Notifier 会用 `wait=true` 发送，让 Discord 确认已创建消息。它也会禁用自动 mention，所以通知正文里即使包含 `@everyone`，也不会 ping 整个 server。

如果某条格式化后的 Discord 通知太长，Agents Notifier 会在发送前让这次 Discord 投递失败。它不会偷偷截断你的消息。

Agents Notifier 会对 Discord 始终使用 `Preview` answer detail。

## 如果没有收到

先检查这些：

- Webhook URL 是否完全正确。
- 这个 webhook 是否仍然存在于 Discord channel 里。
- 这个 channel 是否仍然允许 webhook 发消息。
- 如果使用 `url_env`，这个环境变量是否对正在运行的 service 可见。
- 本地 service 是否正在运行：

```bash
agents-notifier status
```
