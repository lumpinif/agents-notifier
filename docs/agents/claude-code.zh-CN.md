# Claude Code

English documentation: [claude-code.md](claude-code.md)

当你希望 Claude Code 的生命周期 hooks 在任务结束或需要注意时发通知，就使用 Claude Code 集成。

Claude Code 官方支持在 `Stop`、`Notification` 这类生命周期事件上运行用户自定义命令。Agents Notifier 走的就是这条官方 hook 路径，并读取 Claude Code 通过 stdin 传入的 hook JSON。

Claude Code 官方 hooks 文档：<https://code.claude.com/docs/en/hooks>

## Agents Notifier 需要什么

结构化通知建议让 Claude Code hook 运行：

```bash
agents-notifier ingest --source claude_code --format claude_code_hook
```

`ingest` 会读取 hook payload，并保留 Claude Code 明确暴露的字段，包括 project path、session id、注意力提醒消息和最后一条 assistant message。如果 Claude Code payload 里明确包含 `model`，Agents Notifier 会写入结构化 signal。Claude Code 传入 `transcript_path` 时，Agents Notifier 会校验它存在，但不会把这个本机路径转发给 providers。

如果只需要一条简单自定义消息，也可以让 Claude Code 运行：

```bash
agents-notifier emit \
  --source claude_code \
  --title "Claude Code" \
  --body "Claude Code finished a task."
```

`ingest` 和 `emit` 都不会直接发通知。它们只把事件提交给本机正在运行的 Agents Notifier service，然后由 service 按你的配置转发到 provider。

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
            "command": "agents-notifier ingest --source claude_code --format claude_code_hook"
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
            "command": "agents-notifier ingest --source claude_code --format claude_code_hook"
          }
        ]
      }
    ]
  }
}
```

这条命令应该由 Claude Code runtime hook 自动触发，不要让模型在对话里手动运行它。

如果当前 Claude Code 配置入口拿不到结构化 hook stdin，可以先用上面的简单 `emit` 命令。

## 3. 测试链路

service 运行后，用同一条本机 ingress 路径测试：

```bash
agents-notifier emit \
  --source claude_code \
  --title "Claude Code" \
  --body "Test notification from Claude Code."
```

如果 provider 收到这条通知，说明 Agents Notifier 这边已经正常。

如果你的机器上 Claude Code 因为账号或会员原因跑不起来，这个手工 `emit` 测试仍然是 Agents Notifier 侧最正确的本地验证。它验证的是同一条本机 ingress、source adapter、router 和 provider 链路，和 Claude Code hook 实际调用时走的路径一致。

## 如果失败

先检查这些：

- 本机 service 是否正在运行：

```bash
agents-notifier status
```

- 配置里是否有 `claude_code` source。
- route 里是否包含 `claude_code`。
- hook 命令是否使用了 `--source claude_code`。
- 结构化 hook 是否使用 `agents-notifier ingest --source claude_code --format claude_code_hook`。
- Claude Code 运行 hooks 的 shell 环境里是否能找到 `agents-notifier`。
