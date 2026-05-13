# ntfy

English documentation: [ntfy.md](ntfy.md)

两分钟。选一个 topic。手机订阅它。Agents Router 就能把本地 coding agents 的消息发到你的手机上。

## ntfy 是什么？

[ntfy](https://ntfy.sh/) 是一个简单的 HTTP pub-sub 通知服务。

官方文档的核心意思是：你可以用简单的 HTTP PUT 或 POST 请求，从任何电脑上的脚本把 push notification 发到手机或桌面。

对 Agents Router 来说，ntfy 是最快的手机通知方案。

## 官方链接

- [ntfy Getting Started](https://docs.ntfy.sh/)
- [ntfy Server Config](https://docs.ntfy.sh/config/)
- [ntfy Web App 订阅说明](https://docs.ntfy.sh/subscribe/web/)
- [ntfy GitHub](https://github.com/binwiederhier/ntfy)
- [ntfy iOS App Store](https://apps.apple.com/us/app/ntfy/id1625396347)
- [ntfy Android Google Play](https://play.google.com/store/apps/details?id=io.heckel.ntfy)

## 你需要准备什么

- 手机上的 ntfy app，或者 ntfy web app。
- 一个 topic 名称。
- 已经安装 Agents Router。

## 1. 选一个 Topic

使用一个别人猜不到的 topic：

```text
agents-router-felix-8k29
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

## 3. 连接 Agents Router

运行：

```bash
agents-router setup
```

选择：

```text
ntfy
```

直接按 Enter 使用自动生成的 topic，或者粘贴你自己的 topic。

## Answer Detail

Agents Router 会对 ntfy 固定使用 `Preview` answer detail。

ntfy 有官方文档记录的 message body size limit，默认 server 限制是 4K。完整回答可能很长，所以 Agents Router 会让 ntfy 通知保持短小，保证投递更可靠。

## Prompt Detail

Agents Router 会对 ntfy 禁用 prompt detail。

ntfy 有官方文档记录的 message body size limit，默认 server 限制是 4K。Prompt 可能很长，所以 Agents Router 不会把 prompt 放进 ntfy 通知里，避免投递变得不可靠。

## 4. 确认成功

Agents Router 会启动本地 service，并发送一条测试通知。

你应该能在手机上看到它。

## 如果没收到

先检查这几件事：

- 手机订阅的 topic 是否和 setup 里的 topic 完全一致。
- server 是否是 `https://ntfy.sh`。
- topic 里不要有 `/`。
- 本地 service 是否正在运行：

```bash
agents-router status
```

## 自定义 Server

Agents Router 支持在 config 里使用自定义 ntfy server：

```toml
[[providers]]
id = "phone"
type = "ntfy"
server = "https://ntfy.example.com"
topic = "agents-router"
```
