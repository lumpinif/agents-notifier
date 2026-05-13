# Setup

English documentation: [setup.md](setup.md)

用 setup 创建或替换本机配置、启动 service，并发送一条测试通知。

```bash
agents-notifier setup
```

如果还没有配置，setup 会显示推荐默认值。如果已经有配置，setup 会用 `Current` 显示当前答案，直接按 Enter 会保留当前值。
Webhook URL 只显示 host，签名 secret 和私有 provider key 只显示已配置状态，不会把完整敏感内容打印到终端里。

如果要清空飞书/Lark 签名 secret，输入 `none`。

## 语言

Setup 会先问 CLI 语言：

```text
1. English
2. 简体中文
```

默认是 English。选择后会写入 config：

```toml
[cli]
language = "zh-CN"
```

如果要改回英文，使用 `language = "en"`。
也可以在运行 setup 前设置 `AGENTS_NOTIFIER_LANGUAGE=zh-CN`，这样中文会成为默认选择。
Setup 的问题和完成提示会使用你选择的语言。

## Agent

选择 Agents Notifier 要监听哪个 agent：

```text
1. Codex Desktop
2. Codex CLI
3. Claude Code
4. Cursor CLI
5. OpenCode CLI
6. OpenClaw
7. Hermes Agent CLI
8. GitHub Copilot CLI
9. Gemini CLI
10. Aider
```

Codex Desktop 当前在 macOS 和 Windows 上提供。Linux 上，setup 会从 Codex CLI 开始，提供这些 hook-based CLI sources。

## Provider

选择通知要发到哪里：

```text
1. ntfy
2. Slack
3. Discord
4. Pushover
5. Feishu/Lark custom bot
6. Webhook
7. Telegram
8. WhatsApp
9. 微信
10. Microsoft Teams
11. Email SMTP
```

Provider 教程：

- [飞书/Lark Custom Bot](providers/feishu-lark-custom-bot.zh-CN.md)
- [ntfy](providers/ntfy.zh-CN.md)
- [Pushover](providers/pushover.zh-CN.md)
- [Slack](providers/slack.zh-CN.md)
- [Discord](providers/discord.zh-CN.md)
- [Telegram](providers/telegram.zh-CN.md)
- [WhatsApp](providers/whatsapp.zh-CN.md)
- [微信](providers/wechat.zh-CN.md)
- [Microsoft Teams](providers/microsoft-teams.zh-CN.md)
- [Email SMTP](providers/email-smtp.zh-CN.md)
- [Webhook](providers/webhook.zh-CN.md)

## 通知偏好

选择哪些完成的任务需要发送通知：

```text
1. Every completed task (Recommended)
2. Tasks 5 minutes or longer
3. Custom minimum duration
```

直接按 Enter 会使用 `Every completed task`。

默认行为：

- 如果没有设置 `minimum_task_duration_minutes`，完成的任务不会按耗时过滤。
- 选择 `Every completed task` 时，不会写入 `minimum_task_duration_minutes` 字段。
- 选择 `Tasks 5 minutes or longer` 时，会写入 `minimum_task_duration_minutes = 5`。
- 选择 `Custom minimum duration` 时，会写入你输入的正整数分钟数。
- 如果某条 route 设置了 `minimum_task_duration_minutes`，但某个 Signal 没有 `lifecycle.duration_ms`，这条 route 不会匹配。

手动配置：

```toml
[[routes]]
sources = ["codex_desktop"]
providers = ["work_chat"]
minimum_task_duration_minutes = 5

[[routes]]
sources = ["agents_notifier"]
providers = ["work_chat"]
```

`agents_notifier` route 会故意保持独立且不加过滤。setup 测试通知使用这个内部 source，
所以即使真实 agent 通知只转发长时间任务，测试通知仍然能验证你的 provider 是否可用。

如果同一条 route 同时设置了 `minimum_task_duration_minutes` 和
`only_forward_from_project_paths`，两个条件都满足后才会发送通知。

## Answer Detail

选择通知里包含多少回答内容：

```text
1. Preview (Recommended)
2. Full Answer
```

直接按 Enter 会使用 `Preview`。
Full Answer 会包含用户能看到的 assistant 回答，并忽略 Codex App 控制指令。

Answer detail 只对没有小型消息长度限制或本地发送保护线的 provider 开放。

Agents Notifier 会对这些 provider 固定使用 `Preview`：

- ntfy，因为 ntfy 有可配置的 message body size limit，默认是 4K。
- Pushover，因为 Pushover message 最多 1024 个字符。
- Slack，因为 Slack 官方文档记录了 message length 和 truncation 限制。
- Discord，因为 Discord webhook content 最多 2000 个字符。
- Telegram，因为 Telegram Bot API text message 最多 4096 个字符。
- WhatsApp，因为 Agents Notifier 对 WhatsApp text body 使用 4096 字符本地保护线。
- 微信，因为 Agents Notifier 对 微信 iLink text message 使用 3800 字符本地保护线。
- Microsoft Teams，因为 Teams webhook message 有官方文档记录的 28 KB 大小限制。

## Prompt Detail

选择通知里是否包含你发给 agent 的原始 prompt：

```text
1. No (Recommended)
2. Yes
```

直接按 Enter 会使用 `No`。Prompt detail 默认关闭，因为 prompt 里可能包含私有需求、代码、日志、路径或 secret。
对于 Codex Desktop，prompt 来自 Codex 本地 `user_message` 记录。这个记录可能包含 IDE context，例如 active file 和 open tabs。
如果某个 source 没有提供 prompt，通知里就不会显示 Prompt 区块。

手动配置：

```toml
[notification]
answer_detail = "preview"
prompt_detail = "off"
```

Prompt detail 只对没有小型消息长度限制或本地发送保护线的 provider 开放。

Agents Notifier 会对这些 provider 禁用 prompt detail：

- ntfy，因为 ntfy 有可配置的 message body size limit，默认是 4K。
- Pushover，因为 Pushover message 最多 1024 个字符。
- Slack，因为 Slack 官方文档记录了 message length 和 truncation 限制。
- Discord，因为 Discord webhook content 最多 2000 个字符。
- Telegram，因为 Telegram Bot API text message 最多 4096 个字符。
- WhatsApp，因为 Agents Notifier 对 WhatsApp text body 使用 4096 字符本地保护线。
- 微信，因为 Agents Notifier 对 微信 iLink text message 使用 3800 字符本地保护线。
- Microsoft Teams，因为 Teams webhook message 有官方文档记录的 28 KB 大小限制。

如果要包含 prompt：

```toml
[notification]
prompt_detail = "on"
```

如果要发送完整回答：

```toml
[notification]
answer_detail = "full"
```

如果要同时发送完整回答并包含 prompt：

```toml
[notification]
answer_detail = "full"
prompt_detail = "on"
```

正在运行的 service 会自动加载有效的 config 修改。如果 service 没有运行，启动它：

```bash
agents-notifier start
```

如果手动修改后的 config 无效，正在运行的 service 会继续使用上一份有效 config，并在日志里记录 reload 失败。

## 高级项目过滤

如果只想转发具体项目下的通知，可以手动把项目路径加到真实 agent 的 route 上：

```toml
[[routes]]
sources = ["codex_desktop"]
providers = ["work_chat"]
only_forward_from_project_paths = [
  "/Users/felix/Desktop/felix-projects/agents-notifier",
  "/Users/felix/Desktop/felix-projects/another-project",
]

[[routes]]
sources = ["agents_notifier"]
providers = ["work_chat"]
```

设置这个过滤后，只有当 Signal 的 `workspace.project_path` 等于这些路径之一，或位于这些路径之下时，
Agents Notifier 才会转发。项目路径必须是干净的绝对路径。如果某个 source 没有提供
`workspace.project_path`，带项目过滤的 route 就不会匹配。

默认行为：

- 如果没有设置 `only_forward_from_project_paths`，或它是空数组，通知不会按项目路径过滤。
- Setup 不会询问这个选项。需要只转发指定项目时，手动把它加到真实 agent 的 route 上。
- 这个值是数组，所以同一条 route 可以允许多个项目路径。
- 路径必须是非空绝对路径，不能包含 `.` 或 `..` 路径组件。
- 匹配使用路径组件语义，不是字符串前缀。例如 `/Users/me/app` 会匹配 `/Users/me/app/api`，但不会匹配 `/Users/me/app-copy`。
- 如果同一条 route 同时设置了 `only_forward_from_project_paths` 和 `minimum_task_duration_minutes`，两个条件都必须满足。

正在运行的 service 会自动加载有效的 config 修改。如果 service 没有运行，启动它：

```bash
agents-notifier start
```

如果手动修改后的 config 无效，正在运行的 service 会继续使用上一份有效 config，并在日志里记录 reload 失败。

## 结果

Setup 会写入：

```text
~/.config/agents-notifier/config.toml
```

然后它会启动本机 service，并通过同一条 provider 投递链路发送测试通知。
macOS 使用 LaunchAgent，Linux 使用 systemd user service，Windows 使用 Task Scheduler。
