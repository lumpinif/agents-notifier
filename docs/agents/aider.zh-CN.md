# Aider

English documentation: [aider.md](aider.md)

当你希望 Aider 在 LLM 生成完回复、等待你输入时发通知，就用 Aider 集成。

Aider 官方支持 notifications，也支持自定义 `notifications_command`。Agents Notifier 走这条官方 command 路径，并且只接收你在命令里明确传入的标题和正文。

Aider 官方文档：

- <https://aider.chat/docs/usage/notifications.html>

## Agents Notifier 需要什么

配置这个 source：

```toml
[[sources]]
id = "aider"
type = "agent_hook"
```

然后把 `aider` route 到你的 provider。

Agents Notifier 只需要 Aider 运行这一条命令：

```bash
agents-notifier emit \
  --source aider \
  --title "Aider" \
  --body "Aider is ready for input."
```

`emit` 只把事件提交给本地 service ingress。它不会直接发送 provider 通知。

## 命令行示例

用自定义 notification command 启动 Aider：

```bash
aider --notifications --notifications-command "agents-notifier emit --source aider --title \"Aider\" --body \"Aider is ready for input.\""
```

## 配置文件示例

把这个加到 Aider 配置文件里：

```yaml
notifications: true
notifications_command: "agents-notifier emit --source aider --title \"Aider\" --body \"Aider is ready for input.\""
```

这条命令应该放在 Aider notification 配置里。不要让 agent 模型在对话里手动运行它。

## 测试 Route

```bash
agents-notifier emit \
  --source aider \
  --title "Aider" \
  --body "Test notification from Aider."
```

如果 provider 收到这条通知，说明 Agents Notifier 这边已经正常。

## 如果失败

先检查这些：

- 本地 service 是否在运行：`agents-notifier status`。
- 配置里是否有 `aider` source，并且 `type = "agent_hook"`。
- route 是否包含 `aider`。
- Aider notifications 是否已开启。
- Aider notification command 的 shell 环境里是否能找到 `agents-notifier`。
