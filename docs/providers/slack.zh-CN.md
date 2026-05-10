# Slack

English documentation: [slack.md](slack.md)

当你想把 Agents Notifier 通知发到一个 Slack channel 时，就用 Slack。

Agents Notifier 使用 Slack Incoming Webhooks。一个 webhook 会发到创建 webhook 时选择的一个 channel。

消息会以 plain text 发送。

## 官方链接

- [Slack Incoming Webhooks](https://docs.slack.dev/messaging/sending-messages-using-incoming-webhooks/)
- [Slack Rate Limits](https://docs.slack.dev/apis/web-api/rate-limits/)
- [Slack Message Truncation](https://docs.slack.dev/changelog/2018-truncating-really-long-messages/)

## 你需要准备什么

- 一个 Slack workspace。
- 创建或安装启用了 Incoming Webhooks 的 Slack app 的权限。
- 一个目标 channel。
- 已安装 Agents Notifier。

## 1. 创建 Slack Incoming Webhook

打开 Slack app settings。

启用 Incoming Webhooks，然后为接收通知的 channel 创建一个 webhook。

复制 webhook URL。它看起来像这样：

```text
https://hooks.slack.com/services/...
```

这个 URL 是 secret，不要公开。

## 2. 连接 Agents Notifier

运行：

```bash
agents-notifier setup
```

选择：

```text
Slack
```

粘贴 Slack webhook URL。

Agents Notifier 会保存 provider、启动本地 service，并通过真实 agent 事件使用的同一条 service route 发送一条测试消息。

## Answer Detail

Agents Notifier 会对 Slack 固定使用 `Preview` answer detail。

Slack 官方文档记录了消息长度和截断限制。完整回答可能很长，所以 Agents Notifier 会让 Slack 通知保持短小，保证投递更可靠。

## Prompt Detail

Agents Notifier 会对 Slack 禁用 prompt detail。

Prompt 可能很长，也可能包含私人信息。Slack message 有官方文档记录的长度限制，所以 Agents Notifier 不会把 prompt 放进 Slack 通知里。

## 手动配置

Slack 配置在：

```text
~/.config/agents-notifier/config.toml
```

简单配置：

```toml
[[providers]]
id = "slack"
type = "slack"
url = "<your Slack incoming webhook URL>"

[[routes]]
sources = ["codex_desktop", "agents_notifier"]
providers = ["slack"]
```

高级用法：支持 `url_env`，但只有当这个环境变量对正在运行的 macOS LaunchAgent service 可见时才使用它。普通 setup 场景下，`url` 更简单、更可预测。

手动修改后重启 service：

```bash
agents-notifier start
```

## 限制

Slack 建议 posted message text 控制在 4000 个字符以内，并对很长的 posted message 记录了截断行为。Agents Notifier 会把 4000 字符作为 Slack 投递保护线。

如果某条格式化后的 Slack 通知太长，Agents Notifier 会在发送前让这次 Slack 投递失败。它不会偷偷截断你的消息。

Agents Notifier 会对 Slack 始终使用 `Preview` answer detail。

## 如果没有收到

先检查这些：

- Webhook URL 是否完全正确。
- Slack app 是否仍然启用了 Incoming Webhooks。
- Webhook 是否仍然连接到目标 channel。
- 如果使用 `url_env`，这个环境变量是否对正在运行的 service 可见。
- 本地 service 是否正在运行：

```bash
agents-notifier status
```
