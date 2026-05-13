# ntfy

中文文档：[ntfy.zh-CN.md](ntfy.zh-CN.md)

2 minutes. Pick a topic. Subscribe on your phone. Agents Router sends updates there.

## What Is ntfy?

[ntfy](https://ntfy.sh/) is a simple HTTP-based pub-sub notification service.

The official docs describe it as a way to send push notifications to your phone or desktop from scripts on any computer, using simple HTTP PUT or POST requests.

For Agents Router, ntfy is the fastest phone setup.

## Official Links

- [ntfy Getting Started](https://docs.ntfy.sh/)
- [ntfy Server Config](https://docs.ntfy.sh/config/)
- [Subscribe from the web app](https://docs.ntfy.sh/subscribe/web/)
- [ntfy GitHub](https://github.com/binwiederhier/ntfy)
- [ntfy iOS App Store](https://apps.apple.com/us/app/ntfy/id1625396347)
- [ntfy Android Google Play](https://play.google.com/store/apps/details?id=io.heckel.ntfy)

## What You Need

- The ntfy app on your phone, or the ntfy web app.
- One topic name.
- Agents Router installed.

## 1. Choose a Topic

Use a topic that is hard to guess:

```text
agents-router-felix-8k29
```

Topics on the public `ntfy.sh` server are public unless you reserve or protect them. Do not use a simple topic like `codex`.

## 2. Subscribe

Open the ntfy app.

Add a subscription:

```text
Server: https://ntfy.sh
Topic: your-topic-name
```

The app is now listening.

## 3. Connect Agents Router

Run:

```bash
agents-router setup
```

Choose:

```text
ntfy
```

Press Enter to use the generated topic, or paste your own topic.

## Answer Detail

Agents Router fixes answer detail to `Preview` for ntfy.

ntfy has a documented message body size limit. The default server limit is 4K. Full answers can be long, so Agents Router keeps ntfy notifications short for reliable delivery.

## Prompt Detail

Agents Router disables prompt detail for ntfy.

ntfy has a documented message body size limit. The default server limit is 4K. Prompts can be long, so Agents Router keeps prompts out of ntfy notifications to avoid unreliable delivery.

## 4. Confirm

Agents Router starts the local service and sends a test notification.

You should see it on your phone.

## If It Does Not Show Up

Check these first:

- Your phone subscribed to the same topic.
- The server is `https://ntfy.sh`.
- The topic has no `/`.
- The local service is running:

```bash
agents-router status
```

## Custom Server

Agents Router supports a custom ntfy server in config:

```toml
[[providers]]
id = "ntfy"
type = "ntfy"
server = "https://ntfy.example.com"
topic = "agents-router"
```
