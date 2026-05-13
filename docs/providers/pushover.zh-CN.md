# Pushover

English documentation: [pushover.md](pushover.md)

当你想在 Pushover 手机或桌面 app 里收到 Agents Notifier 通知时，就用 Pushover。

## 官方链接

- [Pushover Message API](https://pushover.net/api)
- [Pushover Apps and Devices](https://pushover.net/clients)
- [Pushover Dashboard](https://pushover.net/)
- [Pushover API Knowledge Base](https://support.pushover.net/s1-pushover/knowledgebase/default/c2-api-integration)

## 你需要准备什么

- 一个 Pushover 账号。
- 至少一台已经登录 Pushover app 的设备。
- 一个 Pushover application API token。
- 你的 Pushover user key，或者 group key。
- 已安装 Agents Notifier。

## 1. 创建 Pushover Application

打开 Pushover dashboard，创建一个 application。

复制 application API token。它是一个私有的 30 位字符值。

## 2. 复制 User Key

从 Pushover dashboard 复制你的 user key。

你也可以使用 Pushover group key。Agents Notifier 不区分 user key 和 group key，因为 Pushover API 本身也是这样处理的。

application token 和 user key 都要保密。

## 3. 连接 Agents Notifier

运行：

```bash
agents-notifier setup
```

选择：

```text
Pushover
```

粘贴：

- Pushover application API token
- Pushover user 或 group key

可选字段：

- Device name，直接按 Enter 会发送到所有设备。
- Sound name，直接按 Enter 会使用你的 Pushover 账号默认声音。

## Answer Detail

Agents Notifier 会对 Pushover 固定使用 `Preview` answer detail。

Pushover message 最多 1024 个字符。完整回答可能很长，所以 Agents Notifier 会让 Pushover 通知保持短小，保证投递更可靠。

## Prompt Detail

Agents Notifier 会对 Pushover 禁用 prompt detail。

Pushover message 最多 1024 个字符。Prompt 可能很长，所以 Agents Notifier 不会把 prompt 放进 Pushover 通知里，避免投递变得不可靠。

## 手动配置

Pushover 配置在：

```text
~/.config/agents-notifier/config.toml
```

最简单配置：

```toml
[[providers]]
id = "pushover"
type = "pushover"
app_token = "your-application-api-token"
user_key = "your-user-or-group-key"

[[routes]]
sources = ["codex_desktop"]
providers = ["pushover"]

[[routes]]
sources = ["agents_notifier"]
providers = ["pushover"]
```

可选 device 和 sound：

```toml
[[providers]]
id = "pushover"
type = "pushover"
app_token = "your-application-api-token"
user_key = "your-user-or-group-key"
device = "iphone"
sound = "pushover"
```

高级用法：支持 `app_token_env` 和 `user_key_env`，但只有当这些环境变量对正在运行的本机 service 可见时才应该使用。

手动修改后，正在运行的 service 会自动加载有效的 config 修改。如果 service 没有运行，启动它：

```bash
agents-notifier start
```

## 限制

Pushover 限制 title 最多 250 个字符，message body 最多 1024 个字符。

如果某条通知太长，Agents Notifier 会在发送前让这次 Pushover 投递失败。它不会偷偷截断你的消息。

Agents Notifier 会对 Pushover 始终使用 `Preview` answer detail。

## 如果没有收到

先检查这些：

- application API token 是否完全正确。
- user 或 group key 是否完全正确。
- 至少有一台 Pushover 设备处于 active 状态。
- 如果配置了 `device`，device name 是否完全正确。
- 如果使用 env var，运行中的 service 是否能看到这些环境变量。
- 本地 service 是否正在运行：

```bash
agents-notifier status
```
