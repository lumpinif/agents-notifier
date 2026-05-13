# Microsoft Teams

中文文档：[microsoft-teams.zh-CN.md](microsoft-teams.zh-CN.md)

Use Microsoft Teams when you want Agents Router updates in one Teams channel or chat.

Agents Router sends an Adaptive Card JSON payload to a Teams webhook URL. This is a webhook notification sender, not a full Microsoft Teams bot app.

## Official Links

- [Create Incoming Webhooks](https://learn.microsoft.com/en-us/microsoftteams/platform/webhooks-and-connectors/how-to/add-incoming-webhook)
- [Send messages in Teams using incoming webhooks](https://support.microsoft.com/office/send-messages-in-teams-using-incoming-webhooks-8e36fdf7-1a5d-4871-b8ae-98e6f8c88c67)
- [Adaptive Cards and Incoming Webhooks](https://learn.microsoft.com/en-us/microsoftteams/platform/webhooks-and-connectors/how-to/connectors-using)

## What You Need

- A Microsoft Teams workspace.
- Permission to create a Workflows webhook or incoming webhook for the target channel or chat.
- One Teams webhook URL.
- Agents Router installed.

## 1. Create a Teams Webhook

Microsoft currently recommends Workflows for new webhook-style posting. In Teams, create a workflow that starts with a webhook trigger such as:

```text
When a Teams webhook request is received
```

Copy the generated webhook URL.

Incoming Webhook connector URLs also work if your Teams tenant still allows them, but Microsoft 365 Connectors are being retired. Prefer Workflows for new setup.

Treat the webhook URL like a secret.

## 2. Connect Agents Router

Run:

```bash
agents-router setup
```

Choose:

```text
Microsoft Teams
```

Paste the Teams webhook URL.

Agents Router stores the provider, starts the local service, and sends a test message through the same service route used by real agent events.

## Answer Detail

Agents Router fixes answer detail to `Preview` for Microsoft Teams.

Teams incoming webhook messages have a documented 28 KB message size limit. Full answers can be long, so Agents Router keeps Teams notifications short for reliable delivery.

## Prompt Detail

Agents Router disables prompt detail for Microsoft Teams.

Prompts can be long and private. Teams webhook messages have a documented 28 KB size limit, so Agents Router keeps prompts out of Teams notifications.

## Manual Config

Microsoft Teams is configured in:

```text
~/.config/agents-router/config.toml
```

Simple config:

```toml
[[providers]]
id = "microsoft_teams"
type = "microsoft_teams"
url = "<your Teams webhook URL>"

[[routes]]
sources = ["codex_desktop"]
providers = ["microsoft_teams"]

[[routes]]
sources = ["agents_router"]
providers = ["microsoft_teams"]
```

Advanced: `url_env` is supported, but only use it when the environment variable is visible to the running local service. For normal setup, `url` is simpler and more predictable.

The running service automatically reloads valid config changes. If it is not running, start it:

```bash
agents-router start
```

## Limits

Agents Router sends an Adaptive Card payload with:

```text
type = "message"
contentType = "application/vnd.microsoft.card.adaptive"
```

If the serialized Teams webhook payload exceeds 28 KB, Agents Router fails the Teams delivery before sending. It does not silently cut your message.

Agents Router always uses `Preview` answer detail for Microsoft Teams.

## If It Does Not Show Up

Check these first:

- The webhook URL is exact.
- The workflow or incoming webhook still exists.
- The workflow owner still has access.
- The workflow is enabled.
- If you use `url_env`, the environment variable is visible to the running service.
- The local service is running:

```bash
agents-router status
```
