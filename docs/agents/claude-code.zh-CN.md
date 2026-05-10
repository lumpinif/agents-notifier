# Claude Code

English documentation: [claude-code.md](claude-code.md)

当你希望 Claude Code 的生命周期 hooks 在任务结束或需要注意时发通知，就使用 Claude Code 集成。

Claude Code 官方支持在 `Stop`、`Notification` 这类生命周期事件上运行用户自定义命令。Agents Notifier 走的就是这条官方 hook 路径，并且只接收你在命令里明确传入的标题和正文。

Claude Code 官方 hooks 文档：<https://code.claude.com/docs/en/hooks>

## Agents Notifier 需要什么

Agents Notifier 只需要 Claude Code 的 hook 运行这一条命令：

```bash
agents-notifier emit \
  --source claude_code \
  --title "Claude Code" \
  --body "Claude Code finished a task."
```

`emit` 不会直接发通知。它只把事件提交给本机正在运行的 Agents Notifier service，然后由 service 按你的配置转发到 provider。

## 1. 设置 service

运行：

```bash
agents-notifier setup
```

选择：

```text
Claude Code
```

然后选择 provider。

## 2. 连接 Claude Code

把 command hook 加到 Claude Code settings 里。想在 Claude 回复完成后收到通知，就使用 `Stop`。想把 Claude Code 的注意力提醒也转发到 provider，就再加 `Notification`。

只给这台机器用：

```text
~/.claude/settings.json
```

只给某一个项目用：

```text
.claude/settings.local.json
```

示例：

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "agents-notifier emit --source claude_code --title \"Claude Code\" --body \"Claude Code finished a task.\""
          }
        ]
      }
    ],
    "Notification": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "agents-notifier emit --source claude_code --title \"Claude Code\" --body \"Claude Code needs your attention.\""
          }
        ]
      }
    ]
  }
}
```

这条命令应该由 Claude Code runtime hook 自动触发，不要让模型在对话里手动运行它。

## 3. 测试链路

service 运行后，用同一条本机 ingress 路径测试：

```bash
agents-notifier emit \
  --source claude_code \
  --title "Claude Code" \
  --body "Test notification from Claude Code."
```

如果 provider 收到这条通知，说明 Agents Notifier 这边已经正常。

如果你的机器上 Claude Code 因为账号或会员原因跑不起来，这个手工 `emit` 测试仍然是 Agents Notifier 侧最正确的本地验证。它验证的是同一条本机 socket、source adapter、router 和 provider 链路，和 Claude Code hook 实际调用时走的路径一致。

## 如果失败

先检查这些：

- 本机 service 是否正在运行：

```bash
agents-notifier status
```

- 配置里是否有 `claude_code` source。
- route 里是否包含 `claude_code`。
- hook 命令是否使用了 `--source claude_code`。
- Claude Code 运行 hooks 的 shell 环境里是否能找到 `agents-notifier`。
