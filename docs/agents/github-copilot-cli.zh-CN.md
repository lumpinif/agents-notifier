# GitHub Copilot CLI

English documentation: [github-copilot-cli.md](github-copilot-cli.md)

当你希望 GitHub Copilot CLI 的系统通知能提交到正在运行的 Agents Notifier service 时，就用 GitHub Copilot CLI 集成。

GitHub Copilot CLI 官方支持从当前工作目录的 `.github/hooks/*.json` 加载 hooks。Agents Notifier 走的是官方 `notification` hook 路径，并读取 Copilot CLI 通过 stdin 传入的 hook JSON。

GitHub Copilot CLI 官方文档：

- <https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/use-hooks>
- <https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-hooks-reference>
- <https://docs.github.com/copilot/reference/cli-command-reference>

## Agents Notifier 需要什么

配置这个 source：

```toml
[[sources]]
id = "github_copilot_cli"
type = "agent_hook"
```

然后把 `github_copilot_cli` route 到你的 provider。

结构化通知建议让 Copilot CLI hook 运行：

```bash
agents-notifier ingest --source github_copilot_cli --format github_copilot_cli_notification
```

`ingest` 会读取 notification hook payload，并保留 Copilot CLI 明确暴露的字段，包括 project path、session id、notification type、timestamp、title 和 message。

如果只需要一条简单自定义消息，也可以让 Copilot CLI 运行：

```bash
agents-notifier emit \
  --source github_copilot_cli \
  --title "GitHub Copilot CLI" \
  --body "GitHub Copilot CLI emitted a notification."
```

`ingest` 和 `emit` 都只把事件提交给本地 service ingress。它们不会直接发送 provider 通知。

## Hook 示例

创建 `.github/hooks/agents-notifier.json`：

```json
{
  "version": 1,
  "hooks": {
    "notification": [
      {
        "type": "command",
        "bash": "agents-notifier ingest --source github_copilot_cli --format github_copilot_cli_notification",
        "powershell": "agents-notifier ingest --source github_copilot_cli --format github_copilot_cli_notification",
        "timeoutSec": 10
      }
    ]
  }
}
```

GitHub 官方文档说明 `notification` hook 是异步 fire-and-forget。hook 失败不会阻塞 Copilot CLI session。

这条命令应该放在 runtime hook 配置里。不要让 agent 模型在对话里手动运行它。

如果当前 Copilot CLI 配置入口拿不到结构化 hook stdin，可以先用上面的简单 `emit` 命令。

## 测试 Route

```bash
agents-notifier emit \
  --source github_copilot_cli \
  --title "GitHub Copilot CLI" \
  --body "Test notification from GitHub Copilot CLI."
```

如果 provider 收到这条通知，说明 Agents Notifier 这边已经正常。

## 如果失败

先检查这些：

- 本地 service 是否在运行：`agents-notifier status`。
- 配置里是否有 `github_copilot_cli` source，并且 `type = "agent_hook"`。
- route 是否包含 `github_copilot_cli`。
- hook file 是否是合法 JSON，并且是否放在 `.github/hooks/` 下。
- 结构化 hook 是否使用 `agents-notifier ingest --source github_copilot_cli --format github_copilot_cli_notification`。
- GitHub Copilot CLI 运行 hooks 的 shell 环境里是否能找到 `agents-notifier`。
