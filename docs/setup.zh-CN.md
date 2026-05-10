# Setup

English documentation: [setup.md](setup.md)

用 setup 创建或替换本机配置、启动 service，并发送一条测试通知。

```bash
agents-notifier setup
```

如果已经有配置，setup 会把现有答案作为默认值。任何问题直接按 Enter，都会保留当前显示的默认值。
Webhook URL 只显示 host，签名 secret 只显示已配置状态，不会把完整敏感内容打印到终端里。

如果要清空飞书/Lark 签名 secret，输入 `none`。

## Agent

选择 Agents Notifier 要监听哪个 agent：

```text
1. Codex Desktop
2. Codex CLI
3. Claude Code
```

## Answer Detail

选择通知里包含多少回答内容：

```text
1. Preview (Recommended)
2. Full Answer
```

直接按 Enter 会使用 `Preview`。
Full Answer 会包含用户能看到的 assistant 回答，并忽略 Codex App 控制指令。

## Prompt Detail

选择通知里是否包含你发给 agent 的原始 prompt：

```text
1. No (Recommended)
2. Yes
```

直接按 Enter 会使用 `No`。Prompt detail 默认关闭，因为 prompt 里可能包含私有需求、代码、日志、路径或 secret。
如果某个 source 没有提供 prompt，通知里就不会显示 Prompt 区块。

手动配置：

```toml
[notification]
answer_detail = "preview"
prompt_detail = "off"
```

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

手动修改后，重启 service：

```bash
agents-notifier stop
agents-notifier start
```

## Provider

选择通知要发到哪里：

```text
1. ntfy
2. Feishu/Lark custom bot
3. Webhook
```

Provider 教程：

- [飞书/Lark Custom Bot](providers/feishu-lark-custom-bot.zh-CN.md)
- [ntfy](providers/ntfy.zh-CN.md)
- [Webhook](providers/webhook.zh-CN.md)

## 结果

Setup 会写入：

```text
~/.config/agents-notifier/config.toml
```

然后它会启动 macOS LaunchAgent service，并通过同一条 route 发送测试通知。
