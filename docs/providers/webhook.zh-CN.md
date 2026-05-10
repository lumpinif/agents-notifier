# Webhook

English documentation: [webhook.md](webhook.md)

当你想把每一条 `Signal` POST 到自己的服务时，就用 Webhook。

它适合内部工具、自动化流程、dashboard，或者你自己的通知转发服务。

## Payload

Agents Notifier 会把完整 `Signal` 作为 JSON 发出去：

```json
{
  "schema_version": 1,
  "id": "signal-1",
  "source_id": "codex_cli",
  "source_type": "codex_cli",
  "title": "Codex",
  "body": "Ready for review.",
  "timestamp": "2026-05-08T12:00:00Z",
  "metadata": {}
}
```

你的 endpoint 必须返回 `2xx` 状态码。

其他状态码都会被当成发送失败。

## 你需要准备什么

- 一个接受 `POST` 的 HTTPS endpoint。
- 一个稳定 URL。
- 已经安装 Agents Notifier。

## 1. 准备 Endpoint

你的 endpoint 需要接受：

```text
POST /your/path
Content-Type: application/json
```

先保持简单。第一步只打印 request body。确认收到第一条 signal 后，再加后续处理。

## 2. 添加 Provider

Webhook 配置在：

```text
~/.config/agents-notifier/config.toml
```

LaunchAgent service 推荐直接使用 `url`：

```toml
[[providers]]
id = "debug_webhook"
type = "webhook"
url = "https://example.com/agents-notifier"

[[routes]]
sources = ["codex_desktop", "codex_cli"]
providers = ["debug_webhook"]
```

重启 service：

```bash
agents-notifier start
```

进阶用法：也支持 `url_env`。但只有在你确认环境变量能被运行中的 service 读到时再用。普通 macOS LaunchAgent 场景下，直接写 `url` 更简单、更稳定。

## 3. 确认成功

触发一条测试通知，或者等 coding agent 产生事件。

你的 endpoint 应该收到一条 JSON payload。

## 如果没收到

先检查这几件事：

- endpoint 是否能从你的 Mac 访问。
- endpoint 是否返回 `2xx`。
- 如果你使用 `url_env`，环境变量是否真的能被运行中的 service 读到。
- route 是否包含你想监听的 source。
- 本地 service 是否正在运行：

```bash
agents-notifier status
```

## 安全

把 webhook URL 当成 secret。

使用 HTTPS。

不要打印 token、secret 或完整 webhook URL。
