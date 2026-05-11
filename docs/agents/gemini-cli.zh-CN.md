# Gemini CLI

English documentation: [gemini-cli.md](gemini-cli.md)

当你希望 Gemini CLI 的 hook 事件能提交通知到正在运行的 Agents Notifier service 时，就用 Gemini CLI 集成。

Gemini CLI 官方支持 JSON settings files，也支持 `AfterAgent`、`Notification` 这类 lifecycle hooks。Agents Notifier 走这些官方 hook 事件，并且只接收你在命令里明确传入的标题和正文。

Gemini CLI 官方文档：

- <https://google-gemini.github.io/gemini-cli/docs/cli/configuration.html>
- <https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/configuration.md>
- <https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/writing-hooks.md>
- <https://raw.githubusercontent.com/google-gemini/gemini-cli/main/schemas/settings.schema.json>

## Agents Notifier 需要什么

配置这个 source：

```toml
[[sources]]
id = "gemini_cli"
type = "agent_hook"
```

然后把 `gemini_cli` route 到你的 provider。

Agents Notifier 只需要 Gemini CLI hook 运行这一条命令：

```bash
agents-notifier emit \
  --source gemini_cli \
  --title "Gemini CLI" \
  --body "Gemini CLI finished a task."
```

`emit` 只把事件提交给本地 service ingress。它不会直接发送 provider 通知。

## Hook 示例

把 hooks 加到 Gemini CLI settings file 里，例如 `~/.gemini/settings.json` 或项目里的 `.gemini/settings.json`：

```json
{
  "hooksConfig": {
    "enabled": true
  },
  "hooks": {
    "AfterAgent": [
      {
        "matcher": "*",
        "hooks": [
          {
            "name": "agents-notifier-after-agent",
            "type": "command",
            "command": "agents-notifier emit --source gemini_cli --title \"Gemini CLI\" --body \"Gemini CLI finished a task.\"",
            "timeout": 10000
          }
        ]
      }
    ],
    "Notification": [
      {
        "matcher": "*",
        "hooks": [
          {
            "name": "agents-notifier-notification",
            "type": "command",
            "command": "agents-notifier emit --source gemini_cli --title \"Gemini CLI\" --body \"Gemini CLI needs your attention.\"",
            "timeout": 10000
          }
        ]
      }
    ]
  }
}
```

Gemini CLI settings schema 里，hook entry 的结构是 `matcher` 加 `hooks`，command hook 会运行 shell command。

这条命令应该放在 runtime hook 配置里。不要让 agent 模型在对话里手动运行它。

## 测试 Route

```bash
agents-notifier emit \
  --source gemini_cli \
  --title "Gemini CLI" \
  --body "Test notification from Gemini CLI."
```

如果 provider 收到这条通知，说明 Agents Notifier 这边已经正常。

## 如果失败

先检查这些：

- 本地 service 是否在运行：`agents-notifier status`。
- 配置里是否有 `gemini_cli` source，并且 `type = "agent_hook"`。
- route 是否包含 `gemini_cli`。
- Gemini CLI settings file 是否是合法 JSON。
- `hooksConfig.enabled` 是否没有被关闭。
- Gemini CLI 运行 hooks 的 shell 环境里是否能找到 `agents-notifier`。
