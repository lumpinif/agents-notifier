# WeChat

English documentation: [weixin.md](weixin.md)

当你想通过个人微信的 iLink bot 连接，把 Agents Notifier 通知发到一个微信聊天时，就用 WeChat。

这是腾讯/微信官方 OpenClaw iLink bot 通道里的个人微信连接。它不是企业微信，也不是 WhatsApp。

微信里当前出现的 bot 对话叫 `WeixinClawBot`。Agents Notifier 不能修改这个 bot 名字。这个名字由微信 iLink/OpenClaw 通道控制，不由本机 app 控制。

## 你需要什么

- 一个可以扫码的微信手机端账号。
- 可以访问 iLink 网关的网络。
- 已安装 Agents Notifier。

默认情况下，Agents Notifier 使用：

```text
https://ilinkai.weixin.qq.com
```

setup 里会问两个 iLink 连接参数：

- `WeChat gateway URL`：微信 iLink 网关地址。普通用户直接按 Enter 使用默认值，只有服务方明确给你另一个 URL 时才需要改。
- `Optional WeChat route tag`：高级可选路由标签，对应 iLink 的 `SKRouteTag`。普通用户直接按 Enter 跳过，只有服务方明确给你这个值时才需要填。

## 1. 连接 Agents Notifier

运行：

```bash
agents-notifier setup
```

选择：

```text
WeChat
```

Agents Notifier 支持两种设置方式：

- 扫微信二维码，获取 iLink token。
- 粘贴已有 iLink token。

token 准备好之后，Agents Notifier 会要求你打开微信里刚出现的 bot 对话，例如 `WeixinClawBot`，然后在这个 bot 对话里手动发送一条短消息：

```text
hi
```

这条消息不是发到终端里，而是发到微信里的 bot 对话框里。它会让 Agents Notifier 拿到 iLink `sendmessage` 必需的 `recipient_user_id` 和 `context_token`。

之后 Agents Notifier 会写入 provider config，启动本机 service，并通过真实 route 发送测试通知。

## 实现方式

setup 使用 iLink bot 扫码登录流程：

1. Agents Notifier 向 `/ilink/bot/get_bot_qrcode` 请求二维码。
2. 你用微信扫码。
3. Agents Notifier 轮询 `/ilink/bot/get_qrcode_status`，直到 iLink 返回 bot token。
4. 你在 `WeixinClawBot` 对话里发送 `hi`。
5. Agents Notifier 只在 setup 阶段轮询 `/ilink/bot/getupdates`，读取这条消息，并保存接收人的 id 和 context token。

运行时，Agents Notifier 不会轮询你的微信消息。它只通过下面这个接口发送通知：

```text
POST {base_url}/ilink/bot/sendmessage
```

运行时请求会使用已保存的 token、接收人 id 和 context token。如果 iLink 明确返回非 0 的 `ret` 或 `errcode`，Agents Notifier 会把这次投递判定为失败，并保留错误上下文。

## Answer Detail

Agents Notifier 会对 WeChat 固定使用 `Preview` answer detail。

微信通知应该保持短小。Agents Notifier 对 WeChat iLink text message 使用 3800 字符的本地保护线；如果格式化后的通知太长，会在发送前失败。

## Prompt Detail

Agents Notifier 会对 WeChat 禁用 prompt detail。

Prompt 可能很长，也可能包含私人信息，所以 Agents Notifier 不会把 prompt 放进 WeChat 通知里。

## 手动配置

WeChat 配置在：

```text
~/.config/agents-notifier/config.toml
```

简单配置：

```toml
[[providers]]
id = "weixin"
type = "weixin"
base_url = "https://ilinkai.weixin.qq.com"
token = "<your iLink bot token>"
recipient_user_id = "<recipient iLink user id>"
context_token = "<recipient context token>"
# route_tag = "<optional advanced SKRouteTag>"

[[routes]]
sources = ["codex_desktop", "agents_notifier"]
providers = ["weixin"]
```

进阶：支持 `token_env` 和 `context_token_env`，但只有在环境变量对本机 service 可见时才使用。普通 setup 场景下，直接写入配置更简单、更可预测。

手动修改配置后，重启 service：

```bash
agents-notifier start
```

## 限制

Agents Notifier 只通过 WeChat 发送纯文本。它不通过 WeChat 发送图片、文件、音频、表情或交互卡片。

Agents Notifier 不能修改微信 bot 的名字。这个 bot 名字由微信官方 iLink/OpenClaw 通道控制，当前显示为 `WeixinClawBot`。

Agents Notifier 不会创建自定义微信 bot、公众号、小程序或企业微信应用。它使用现有的 WeChat iLink bot 通道。

`base_url` 必须是 HTTPS origin，例如 `https://ilinkai.weixin.qq.com`。

`token`、`recipient_user_id`、`context_token` 和 `route_tag` 不能包含空白字符。

如果 iLink 返回 `context_token` 过期或无效，Agents Notifier 会让这次 WeChat 投递明确失败，并保留错误。它不会在后台偷偷轮询你的微信消息。

如果 context token 过期，请重新运行 `agents-notifier setup`，重新选择 WeChat 并绑定这个聊天。

## 如果收不到

先检查这些：

- WeChat iLink token 是否有效。
- 被绑定的微信账号是否在 setup 时给 `WeixinClawBot` 对话发送过 `hi`。
- `context_token` 是否已经过期。
- `base_url` 和可选的 `route_tag` 是否与 iLink 服务方提供的一致。
- 如果使用 `token_env` 或 `context_token_env`，环境变量是否对正在运行的 service 可见。
- 本机 service 是否正在运行：

```bash
agents-notifier status
```
