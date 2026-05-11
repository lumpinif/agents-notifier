# Email SMTP

English documentation: [email-smtp.md](email-smtp.md)

当你想通过自己的 SMTP server 或邮件服务商，把 Agents Notifier 通知发送成纯文本邮件时，就用 Email SMTP。

Agents Notifier 做的是 SMTP 发信。它不收邮件、不读 inbox、不管理 OAuth，也不发送附件。

## 官方链接

- [SMTP Message Submission, RFC 6409](https://www.rfc-editor.org/rfc/rfc6409)
- [Cleartext Considered Obsolete, RFC 8314](https://www.rfc-editor.org/rfc/rfc8314)
- [Internet Message Format, RFC 5322](https://www.rfc-editor.org/rfc/rfc5322)
- [Google Workspace SMTP settings](https://support.google.com/a/answer/176600?hl=en)
- [Microsoft 365 SMTP client submission](https://learn.microsoft.com/en-us/troubleshoot/exchange/email-delivery/fix-issues-with-printers-scanners-and-lob-applications-that-send-email-using-off)
- [Amazon SES SMTP interface](https://docs.aws.amazon.com/ses/latest/dg/smtp-connect.html)

## 你需要准备

- 一个 SMTP host。
- 一个 port。
- 一个 TLS mode。
- 一个发件人邮箱。
- 一个或多个收件人邮箱。
- SMTP username 和 password，除非你的 SMTP relay 明确允许免认证。
- 已安装 Agents Notifier。

## 1. 选择 TLS 模式

只能使用其中一种：

```text
starttls
implicit_tls
```

推荐默认：

```text
host = "smtp.example.com"
port = 587
security = "starttls"
```

只有当你的服务商明确给你 port 465 时，才使用 `implicit_tls`。

Agents Notifier V1 不支持远程明文 SMTP。

## 2. 准备 Credentials

很多服务商的 SMTP password 不是你的普通账号密码。

常见例子：

- Gmail 或 Google Workspace：app password，或 Workspace SMTP relay credentials。
- Microsoft 365：mailbox 或 tenant 必须开启 SMTP AUTH。
- Amazon SES：使用 SES 生成的 SMTP credentials。
- SendGrid：username 通常是 `apikey`，password 是 API key。
- Mailgun：使用 sending domain settings 里的 SMTP credentials。

## 3. 连接 Agents Notifier

运行：

```bash
agents-notifier setup
```

选择：

```text
Email SMTP
```

输入 SMTP host、security mode、port、必要时输入 username/password、发件人、收件人，以及可选 Reply-To。

Agents Notifier 会保存 provider、启动本地 service，并通过真实 agent 事件使用的同一条 service route 发送一封测试邮件。

## Answer Detail

Email SMTP 不像手机或聊天 provider 那样有很小的消息长度限制。

Agents Notifier 对 Email SMTP 允许 `Preview` 或 `Full Answer`。

## Prompt Detail

Agents Notifier 对 Email SMTP 允许 prompt detail，但要注意隐私。邮件可能经过你的邮件服务商服务器，并保存在收件人 inbox 里。

Prompt detail 默认仍然关闭。

## 手动配置

Email SMTP 配置在：

```text
~/.config/agents-notifier/config.toml
```

简单认证配置：

```toml
[[providers]]
id = "email"
type = "email_smtp"
host = "smtp.example.com"
port = 587
security = "starttls"
username = "alerts@example.com"
password = "<your SMTP password or API key>"
from = "Agents Notifier <alerts@example.com>"
to = ["you@example.com"]

[[routes]]
sources = ["codex_desktop", "agents_notifier"]
providers = ["email"]
```

进阶：支持 `username_env` 和 `password_env`，但只有当这些环境变量对正在运行的本地 service 可见时才使用它们。

```toml
[[providers]]
id = "email"
type = "email_smtp"
host = "smtp.example.com"
port = 587
security = "starttls"
username_env = "AGENTS_NOTIFIER_EMAIL_SMTP_USERNAME"
password_env = "AGENTS_NOTIFIER_EMAIL_SMTP_PASSWORD"
from = "Agents Notifier <alerts@example.com>"
to = ["you@example.com", "team@example.com"]
reply_to = "reply@example.com"
```

免认证 relay 配置：

```toml
[[providers]]
id = "email"
type = "email_smtp"
host = "smtp-relay.internal.example.com"
port = 587
security = "starttls"
from = "Agents Notifier <alerts@example.com>"
to = ["you@example.com"]
```

手动修改后，重启 service：

```bash
agents-notifier start
```

## 投递语义

SMTP 成功只代表 SMTP server 已经接收这封邮件并准备投递。它不保证邮件最终一定进入收件人的 inbox。

Agents Notifier 按 SMTP 状态码这样处理：

- 2xx：sent。
- 4xx：临时 provider failure，可重试。
- 5xx：永久 provider rejection，不做立即重试。

## 如果收不到

先检查这些：

- SMTP host 是否是 hostname，而不是 URL。
- port 是否和 TLS mode 匹配。
- username/password 或 SMTP API key 是否完全正确。
- SMTP provider 是否允许这个 sender address 发信。
- SMTP provider 或 relay 是否允许这个 recipient。
- 如果使用 Microsoft 365 authenticated submission，SMTP AUTH 是否已开启。
- 如果使用 `username_env` 或 `password_env`，环境变量是否对正在运行的 service 可见。
- 本地 service 是否在运行：

```bash
agents-notifier status
```
