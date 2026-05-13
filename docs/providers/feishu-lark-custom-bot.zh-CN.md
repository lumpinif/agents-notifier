# 飞书/Lark Custom Bot

English documentation: [feishu-lark-custom-bot.md](feishu-lark-custom-bot.md)

两三分钟。一个群机器人。一个 webhook URL。然后你就能在飞书或 Lark 群里收到本地 coding agents 的消息。

## 官方链接

- 飞书：[自定义机器人使用指南](https://open.feishu.cn/document/client-docs/bot-v3/add-custom-bot)
- Lark：[Custom bot usage guide](https://open.larksuite.com/document/client-docs/bot-v3/add-custom-bot)

## 你需要准备什么

- 一个飞书或 Lark 群。
- 你有权限给这个群添加机器人。
- 已经安装 Agents Router。

## 1. 添加机器人

打开你想接收通知的群。

然后进入：

```text
群设置 -> 群机器人 -> 添加机器人 -> 自定义机器人
```

名字建议直接写：

```text
Agents Router
```

完成机器人创建。

## 2. 复制 Webhook URL

机器人创建完成后，复制它的 webhook URL。

它应该长这样：

```text
https://open.feishu.cn/open-apis/bot/v2/hook/...
https://open.larksuite.com/open-apis/bot/v2/hook/...
```

不要公开这个 URL。拿到它的人可以往你的群里发消息。

## 3. 安全设置

推荐使用：

```text
签名校验
```

如果你开启了签名校验，把 signing secret 也复制下来。

不建议给 Agents Router 使用关键词安全策略。关键词规则可能会拦截正常通知，因为每条消息都必须包含那个关键词。

## 4. 连接 Agents Router

运行：

```bash
agents-router setup
```

选择：

```text
Feishu/Lark custom bot
```

粘贴 webhook URL。

如果你开启了签名校验，就粘贴 signing secret。  
如果没有开启，直接按 Enter。

## 5. 确认成功

Agents Router 会启动本地 service，并发送一条测试通知。

你应该能在群里看到一张卡片。

## 如果没收到

先检查这几件事：

- webhook URL 是否以 `https://open.feishu.cn/open-apis/bot/v2/hook/` 或 `https://open.larksuite.com/open-apis/bot/v2/hook/` 开头。
- 如果开启了签名校验，signing secret 是否完全一致。
- 是否关闭了关键词安全策略。
- 机器人是否添加到了你想接收通知的那个群。
- 本地 service 是否正在运行：

```bash
agents-router status
```

