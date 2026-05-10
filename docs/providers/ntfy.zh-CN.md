# ntfy

English documentation: [ntfy.md](ntfy.md)

两分钟。选一个 topic。手机订阅它。Agents Notifier 就能把本地 coding agents 的消息发到你的手机上。

## ntfy 是什么？

[ntfy](https://ntfy.sh/) 是一个简单的 HTTP pub-sub 通知服务。

官方文档的核心意思是：你可以用简单的 HTTP PUT 或 POST 请求，从任何电脑上的脚本把 push notification 发到手机或桌面。

对 Agents Notifier 来说，ntfy 是最快的手机通知方案。

## 官方链接

- [ntfy Getting Started](https://docs.ntfy.sh/)
- [ntfy Web App 订阅说明](https://docs.ntfy.sh/subscribe/web/)
- [ntfy GitHub](https://github.com/binwiederhier/ntfy)
- [ntfy iOS App Store](https://apps.apple.com/us/app/ntfy/id1625396347)
- [ntfy Android Google Play](https://play.google.com/store/apps/details?id=io.heckel.ntfy)

## 你需要准备什么

- 手机上的 ntfy app，或者 ntfy web app。
- 一个 topic 名称。
- 已经安装 Agents Notifier。

## 1. 选一个 Topic

使用一个别人猜不到的 topic：

```text
agents-notifier-felix-8k29
```

如果使用公开的 `ntfy.sh` server，topic 默认是公开的，除非你自己 reserve 或保护它。不要用 `codex` 这种太简单的 topic。

## 2. 在手机上订阅

打开 ntfy app。

添加订阅：

```text
Server: https://ntfy.sh
Topic: your-topic-name
```

现在手机已经开始监听这个 topic。

## 3. 连接 Agents Notifier

运行：

```bash
agents-notifier setup
```

选择：

```text
ntfy
```

直接按 Enter 使用自动生成的 topic，或者粘贴你自己的 topic。

## 4. 确认成功

Agents Notifier 会启动本地 service，并发送一条测试通知。

你应该能在手机上看到它。

## 如果没收到

先检查这几件事：

- 手机订阅的 topic 是否和 setup 里的 topic 完全一致。
- server 是否是 `https://ntfy.sh`。
- topic 里不要有 `/`。
- 本地 service 是否正在运行：

```bash
agents-notifier status
```

## 自定义 Server

Agents Notifier 支持在 config 里使用自定义 ntfy server：

```toml
[[providers]]
id = "phone"
type = "ntfy"
server = "https://ntfy.example.com"
topic = "agents-notifier"
```

