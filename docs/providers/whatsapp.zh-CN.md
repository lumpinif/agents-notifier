# WhatsApp

English documentation: [whatsapp.md](whatsapp.md)

当你想通过 WhatsApp Business Platform Cloud API，把 Agents Notifier 通知发给一个 WhatsApp 用户时，就用 WhatsApp。

Agents Notifier 发送的是非模板 text message。它是通知发送器，不是双向 WhatsApp chat bot。

## 官方链接

- [WhatsApp Business Platform](https://developers.facebook.com/docs/whatsapp)
- [Cloud API Get Started](https://developers.facebook.com/documentation/business-messaging/whatsapp/get-started)
- [Cloud API](https://developers.facebook.com/docs/whatsapp/cloud-api)

## 你需要准备

- 一个连接到 WhatsApp Business Account 的 Meta developer app。
- 一个 WhatsApp Business phone number ID。
- 一个可以发送 WhatsApp messages 的 system user access token。
- 一个带国家区号、只包含数字的接收方手机号。
- 已安装 Agents Notifier。

## 1. 准备 WhatsApp Cloud API Access

在 Meta 的 WhatsApp Business Platform setup 里拿到：

```text
Phone number ID
System user access token
Recipient WhatsApp phone number
```

Cloud API 发送 text message 的请求形态是：

```text
POST https://graph.facebook.com/v23.0/<PHONE_NUMBER_ID>/messages
Authorization: Bearer <ACCESS_TOKEN>
Content-Type: application/json
```

Agents Notifier 发送：

```json
{
  "messaging_product": "whatsapp",
  "recipient_type": "individual",
  "to": "15551234567",
  "type": "text",
  "text": {
    "body": "Agents Notifier message"
  }
}
```

## 2. 理解 24 小时窗口

WhatsApp 的非模板消息用于 customer service window。大白话说，接收方通常需要先给你的 WhatsApp Business account 发过消息，并且还在允许回复的窗口内。

Agents Notifier 这个 provider 不发送 WhatsApp template message，只发送普通通知文本。

## 3. 连接 Agents Notifier

运行：

```bash
agents-notifier setup
```

选择：

```text
WhatsApp
```

粘贴 access token、phone number ID 和 recipient phone number。

Agents Notifier 会保存 provider、启动本地 service，并通过真实 agent 事件使用的同一条 service route 发送一条测试消息。

## Answer Detail

Agents Notifier 会对 WhatsApp 固定使用 `Preview` answer detail。

WhatsApp 通知应该保持短小。Agents Notifier 对 WhatsApp text body 使用 4096 字符的本地保护线；如果格式化后的通知太长，会在发送前失败。

## Prompt Detail

Agents Notifier 会对 WhatsApp 禁用 prompt detail。

Prompt 可能很长，也可能包含私人信息，所以 Agents Notifier 不会把 prompt 放进 WhatsApp 通知里。

## 手动配置

WhatsApp 配置在：

```text
~/.config/agents-notifier/config.toml
```

简单配置：

```toml
[[providers]]
id = "whatsapp"
type = "whatsapp"
access_token = "<your WhatsApp Cloud API access token>"
phone_number_id = "123456789012345"
recipient_phone_number = "15551234567"

[[routes]]
sources = ["codex_desktop", "agents_notifier"]
providers = ["whatsapp"]
```

进阶：支持 `access_token_env`，但只有当这个环境变量对正在运行的本地 service 可见时才使用它。普通 setup 里，`access_token` 更简单、更可预测。

手动修改后，重启 service：

```bash
agents-notifier start
```

## 限制

Agents Notifier 使用 Graph API version `v23.0`。

`phone_number_id` 必须只包含数字。`recipient_phone_number` 必须是 7 到 15 位数字，不能包含 `+`、空格或标点。

如果某条格式化后的 WhatsApp 通知太长，Agents Notifier 会在发送前让这次 WhatsApp 投递失败。它不会偷偷截断你的消息。

## 如果收不到

先检查这些：

- access token 是否有效、是否过期。
- phone number ID 是否完全正确。
- recipient phone number 是否包含国家区号，并且只包含数字。
- 接收方是否被你的 WhatsApp Business Platform setup 允许。
- 非模板消息是否还在 customer service window 里。
- 如果使用 `access_token_env`，这个环境变量是否对正在运行的 service 可见。
- 本地 service 是否在运行：

```bash
agents-notifier status
```
