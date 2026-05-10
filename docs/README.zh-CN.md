# agents-notifier

两三分钟就能 setup 好。
然后你就可以在手机、飞书、Lark、Pushover 或 Webhook 上收到本地 coding agents 的消息。

---

English documentation: [../README.md](../README.md)

[快速开始](#-安装---step-1)

> _想象一下：你的 [Codex Desktop App](https://openai.com/codex/) 在后台工作，你去煮咖啡或者洗衣服，或者暂时离开电脑。_
>
> _任务完成的那一刻，你会收到通知，然后知道现在该回来了。_

⚡ 为 AI coding agents 提供本地优先的通知。

适用于 [Codex Desktop](https://openai.com/codex/)、[Codex CLI](https://github.com/openai/codex)、[Claude Code](https://claude.com/product/claude-code) 这类本地 agent。

使用 Rust 🦀 构建。快速、小巧，并且安静地在后台运行。

```text
你 Mac 上的 Agent -> Agents Notifier -> 你的通知渠道
```

不需要云账号。不需要托管后端。不需要额外 dashboard。

## ✅ 支持情况

Agents：

- macOS 上的 [Codex Desktop App](https://openai.com/codex/)
- 通过 hooks 接入的 [Codex CLI](https://github.com/openai/codex)
- 通过 hooks 接入的 [Claude Code](https://claude.com/product/claude-code)
- 更多本地 agents 即将支持

Providers（你想在哪里收到通知？）：

- ntfy
- Feishu/Lark Custom Bot
- Pushover
- Webhook
- 更多 providers 即将支持

## 🔒 隐私

Agents Notifier 在本地运行。

你的数据不会发送到 Agents Notifier 的云端，因为这个项目没有托管云服务。

通知会直接从你的 Mac 发到你配置的 provider。

对于 Codex Desktop，它只读取生成通知所需的完成信息：

- 项目名
- 项目路径
- session
- Codex thread 链接
- 运行时长
- 分支
- 时间
- 默认发送最终回答 preview，也可以改成完整回答
- 只有用户明确开启时才发送 prompt
- Mac 电脑名称

在 Feishu/Lark 中，通知会以 Codex 风格的 interactive card 发送，并带有可点击的 Open in Codex 按钮。
这个按钮会先打开一个本地浏览器 URL，然后再交给 Codex Desktop。

## ⚙️ 安装 - Step 1

安装方式二选一就够。

推荐方式：

复制到 Terminal 里运行：

```bash
curl -fsSL https://raw.githubusercontent.com/lumpinif/agents-notifier/main/install.sh | sh
agents-notifier setup
```

从源码安装：

```bash
git clone https://github.com/lumpinif/agents-notifier.git
cd agents-notifier
cargo install --path .
agents-notifier setup
```

## 🚀 设置 - Step 2

```bash
agents-notifier setup
```

只需要回答 2 个必选问题：

1. 要监听哪个 agent？
2. 通知要发到哪里？

然后它会写入配置、启动 service，并发送一条测试通知。

Answer detail、是否包含 prompt 等可选设置见 [Setup](setup.zh-CN.md)。

## 🎉 就这样

在 macOS 上，这个 service 会作为 LaunchAgent 运行。
不想继续使用 service 时，运行 `agents-notifier stop` 关闭。

Provider 设置教程：

- [飞书/Lark Custom Bot](providers/feishu-lark-custom-bot.zh-CN.md)
- [ntfy](providers/ntfy.zh-CN.md)
- [Pushover](providers/pushover.zh-CN.md)
- [Webhook](providers/webhook.zh-CN.md)

Agent 设置教程：

- [Codex CLI](agents/codex-cli.zh-CN.md)
- [Claude Code](agents/claude-code.zh-CN.md)

## 🧹 卸载

一行命令干净卸载：

```bash
agents-notifier uninstall
```

## 🧭 命令

```bash
agents-notifier setup    # 设置或修改 agent/provider
agents-notifier start    # 启动已有 service
agents-notifier status   # 查看 service 状态
agents-notifier stop     # 停止 service
agents-notifier uninstall # 删除 service、配置、日志、状态和二进制
agents-notifier watch    # 前台 debug worker
```

CLI agent hooks 可以这样提交事件：

```bash
agents-notifier emit \
  --source codex_cli \
  --title "Codex" \
  --body "Ready for review."
```

```bash
agents-notifier emit \
  --source claude_code \
  --title "Claude Code" \
  --body "Claude Code finished a task."
```

`emit` 只和本地 service 通信。真正发送到 provider 的动作由 service 完成。

## ✨ 示例

```text
Codex Desktop

Project: agents-notifier
Session: README polish
Open in Codex: codex://threads/019e1049-2d6d-7de2-bcdf-f47346930b71
Duration: 1m 32s
Branch: main
Time: 2026-05-10 01:35:42 +08:00

Preview: Updated the README with a clearer setup flow...
```

## 📝 配置

```text
~/.config/agents-notifier/config.toml
```

大多数用户应该直接使用 `agents-notifier setup`。

## 🧩 核心

```text
Source -> Signal -> Router -> Provider
```

核心保持简单。后续会逐步支持更多 agents 和 providers。

欢迎贡献。💛
