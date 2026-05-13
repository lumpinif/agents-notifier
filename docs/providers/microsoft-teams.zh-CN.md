# Microsoft Teams

English documentation: [microsoft-teams.md](microsoft-teams.md)

当你想把 Agents Notifier 通知发到一个 Microsoft Teams channel 或 chat 时，就用 Microsoft Teams。

Agents Notifier 会把 Adaptive Card JSON payload 发送到 Teams webhook URL。它是 webhook 通知发送器，不是完整的 Microsoft Teams bot app。

## 官方链接

- [Create Incoming Webhooks](https://learn.microsoft.com/en-us/microsoftteams/platform/webhooks-and-connectors/how-to/add-incoming-webhook)
- [Send messages in Teams using incoming webhooks](https://support.microsoft.com/office/send-messages-in-teams-using-incoming-webhooks-8e36fdf7-1a5d-4871-b8ae-98e6f8c88c67)
- [Adaptive Cards and Incoming Webhooks](https://learn.microsoft.com/en-us/microsoftteams/platform/webhooks-and-connectors/how-to/connectors-using)

## 你需要准备

- 一个 Microsoft Teams workspace。
- 在目标 channel 或 chat 里创建 Workflows webhook 或 incoming webhook 的权限。
- 一个 Teams webhook URL。
- 已安装 Agents Notifier。

## 1. 创建 Teams Webhook

Microsoft 现在建议新的 webhook 式推送优先使用 Workflows。在 Teams 里创建一个以 webhook trigger 开始的 workflow，例如：

```text
When a Teams webhook request is received
```

复制生成的 webhook URL。

如果你的 Teams tenant 仍然允许 Incoming Webhook connector URL，也可以使用；但 Microsoft 365 Connectors 正在 retirement。新配置优先用 Workflows。

把 webhook URL 当成 secret。

## 2. 连接 Agents Notifier

运行：

```bash
agents-notifier setup
```

选择：

```text
Microsoft Teams
```

粘贴 Teams webhook URL。

Agents Notifier 会保存 provider、启动本地 service，并通过真实 agent 事件使用的同一条 service route 发送一条测试消息。

## Answer Detail

Agents Notifier 会对 Microsoft Teams 固定使用 `Preview` answer detail。

Teams incoming webhook message 有官方文档记录的 28 KB 大小限制。完整回答可能很长，所以 Agents Notifier 会让 Teams 通知保持短小，保证投递更可靠。

## Prompt Detail

Agents Notifier 会对 Microsoft Teams 禁用 prompt detail。

Prompt 可能很长，也可能包含私人信息。Teams webhook message 有官方文档记录的 28 KB 大小限制，所以 Agents Notifier 不会把 prompt 放进 Teams 通知里。

## 手动配置

Microsoft Teams 配置在：

```text
~/.config/agents-notifier/config.toml
```

简单配置：

```toml
[[providers]]
id = "microsoft_teams"
type = "microsoft_teams"
url = "<your Teams webhook URL>"

[[routes]]
sources = ["codex_desktop"]
providers = ["microsoft_teams"]

[[routes]]
sources = ["agents_notifier"]
providers = ["microsoft_teams"]
```

进阶：支持 `url_env`，但只有当这个环境变量对正在运行的本地 service 可见时才使用它。普通 setup 里，`url` 更简单、更可预测。

正在运行的 service 会自动加载有效的 config 修改。如果 service 没有运行，启动它：

```bash
agents-notifier start
```

## 限制

Agents Notifier 发送的是 Adaptive Card payload：

```text
type = "message"
contentType = "application/vnd.microsoft.card.adaptive"
```

如果序列化后的 Teams webhook payload 超过 28 KB，Agents Notifier 会在发送前让这次 Teams 投递失败。它不会偷偷截断你的消息。

Agents Notifier 会对 Microsoft Teams 始终使用 `Preview` answer detail。

## 如果收不到

先检查这些：

- webhook URL 是否完全正确。
- workflow 或 incoming webhook 是否仍然存在。
- workflow owner 是否仍然有权限。
- workflow 是否启用。
- 如果使用 `url_env`，这个环境变量是否对正在运行的 service 可见。
- 本地 service 是否在运行：

```bash
agents-notifier status
```
