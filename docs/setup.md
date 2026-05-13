# Setup

中文文档：[setup.zh-CN.md](setup.zh-CN.md)

Use setup to create or replace the local config, start the service, and send a test notification.

```bash
agents-router setup
```

Without an existing config, setup shows recommended defaults. If a config already exists, setup
prints `Current` for existing answers, and pressing Enter keeps the current value. Webhook URLs are
shown by host only. Signing secrets and private provider keys are shown only as configured.

For a Feishu/Lark signing secret, type `none` to clear the existing secret.

## Language

Setup asks for the CLI language first:

```text
1. English
2. 简体中文
```

English is the default. The chosen language is saved in config:

```toml
[cli]
language = "en"
```

Use `language = "zh-CN"` for Simplified Chinese. You can also set
`AGENTS_ROUTER_LANGUAGE=zh-CN` before running setup to make Chinese the default selection.
Setup prompts and setup confirmation output use the selected language.

## Agent

Choose the agent Agents Router should watch:

```text
1. Codex Desktop
2. Codex CLI
3. Claude Code
4. Cursor CLI
5. OpenCode CLI
6. OpenClaw
7. Hermes Agent CLI
8. GitHub Copilot CLI
9. Gemini CLI
10. Aider
```

Codex Desktop is offered on macOS and Windows. On Linux, setup starts at Codex CLI and offers the hook-based CLI sources.

## Provider

Choose where notifications should go:

```text
1. ntfy
2. Slack
3. Discord
4. Pushover
5. Feishu/Lark custom bot
6. Webhook
7. Telegram
8. WhatsApp
9. WeChat
10. Microsoft Teams
11. Email SMTP
```

Provider guides:

- [Feishu/Lark Custom Bot](providers/feishu-lark-custom-bot.md)
- [ntfy](providers/ntfy.md)
- [Pushover](providers/pushover.md)
- [Slack](providers/slack.md)
- [Discord](providers/discord.md)
- [Telegram](providers/telegram.md)
- [WhatsApp](providers/whatsapp.md)
- [WeChat](providers/wechat.md)
- [Microsoft Teams](providers/microsoft-teams.md)
- [Email SMTP](providers/email-smtp.md)
- [Webhook](providers/webhook.md)

## Provider IDs

Setup uses the provider type as the default provider id. For example, the default
Feishu/Lark provider is:

```toml
[[providers]]
id = "feishu_lark"
type = "feishu_lark"
```

Routes reference provider ids:

```toml
[[routes]]
sources = ["codex_desktop"]
providers = ["feishu_lark"]
```

If you need two providers of the same type, give each provider a clear unique id:

```toml
[[providers]]
id = "feishu_lark_engineering"
type = "feishu_lark"

[[providers]]
id = "feishu_lark_personal"
type = "feishu_lark"
```

## Notification Preference

Choose which completed tasks should send notifications:

```text
1. Every completed task (Recommended)
2. Tasks 5 minutes or longer
3. Custom minimum duration
```

Press Enter to keep `Every completed task`.

Default behavior:

- If `minimum_task_duration_minutes` is not set, completed tasks are not filtered by duration.
- `Every completed task` writes no `minimum_task_duration_minutes` field.
- `Tasks 5 minutes or longer` writes `minimum_task_duration_minutes = 5`.
- `Custom minimum duration` writes the positive integer number of minutes you enter.
- If a route has `minimum_task_duration_minutes`, a signal without `lifecycle.duration_ms` does not match that route.
- Setup only asks for this preference when the selected integration can reliably provide task duration. For wrapper-based integrations, configure the route manually and pass duration through `agents-router emit --duration-ms` or the structured hook `lifecycle.duration_ms` field.

Manual config:

```toml
[[routes]]
sources = ["codex_desktop"]
providers = ["feishu_lark"]
minimum_task_duration_minutes = 5

[[routes]]
sources = ["agents_router"]
providers = ["feishu_lark"]
```

Wrapper example:

```bash
agents-router emit \
  --source aider \
  --title "Aider" \
  --body "Aider finished a task." \
  --duration-ms 420000
```

The `agents_router` route is intentionally separate and unfiltered. Setup test notifications use
that internal source, so the test still verifies your provider even when real agent notifications
are limited to long-running tasks.

If a route has both `minimum_task_duration_minutes` and `only_forward_from_project_paths`, both
filters must match before the notification is sent.

## Answer Detail

Choose how much answer text notifications include:

```text
1. Preview (Recommended)
2. Full Answer
```

Press Enter to keep `Preview`.
Full Answer includes the visible assistant answer and omits Codex App control directives.

Answer detail is only configurable for providers without a small message size limit or delivery guard.

Agents Router fixes answer detail to `Preview` for:

- ntfy, because ntfy has a configurable message body size limit that defaults to 4K.
- Pushover, because Pushover messages are limited to 1024 characters.
- Slack, because Slack has documented message length and truncation limits.
- Discord, because Discord webhook content is limited to 2000 characters.
- Telegram, because Telegram Bot API text messages are limited to 4096 characters.
- WhatsApp, because Agents Router uses a 4096-character guard for WhatsApp text bodies.
- WeChat, because Agents Router uses a 3800-character guard for WeChat iLink text messages.
- Microsoft Teams, because Teams webhook messages have a documented 28 KB size limit.

## Prompt Detail

Choose whether notifications include your original prompt:

```text
1. No (Recommended)
2. Yes
```

Press Enter to keep `No`. Prompt detail is off by default because prompts can contain private
requirements, code, logs, paths, or secrets.
For Codex Desktop, the prompt comes from Codex's local `user_message` record. Codex may include
IDE context such as the active file and open tabs in that record.
If a source does not provide a prompt, no Prompt section is shown.

Manual config:

```toml
[notification]
answer_detail = "preview"
prompt_detail = "off"
```

Prompt detail is only configurable for providers without a small message size limit or delivery guard.

Agents Router disables prompt detail for:

- ntfy, because ntfy has a configurable message body size limit that defaults to 4K.
- Pushover, because Pushover messages are limited to 1024 characters.
- Slack, because Slack has documented message length and truncation limits.
- Discord, because Discord webhook content is limited to 2000 characters.
- Telegram, because Telegram Bot API text messages are limited to 4096 characters.
- WhatsApp, because Agents Router uses a 4096-character guard for WhatsApp text bodies.
- WeChat, because Agents Router uses a 3800-character guard for WeChat iLink text messages.
- Microsoft Teams, because Teams webhook messages have a documented 28 KB size limit.

To include prompts:

```toml
[notification]
prompt_detail = "on"
```

To send full answers:

```toml
[notification]
answer_detail = "full"
```

To send full answers and include prompts:

```toml
[notification]
answer_detail = "full"
prompt_detail = "on"
```

The running service automatically reloads valid config changes. If the service is not running,
start it:

```bash
agents-router start
```

If a manual edit makes the config invalid, the running service keeps the last valid config and logs
the reload failure.

## Advanced Project Filter

To forward notifications only from specific projects, manually add project paths to the real agent
route:

```toml
[[routes]]
sources = ["codex_desktop"]
providers = ["feishu_lark"]
only_forward_from_project_paths = [
  "/Users/felix/Desktop/felix-projects/agents-router",
  "/Users/felix/Desktop/felix-projects/another-project",
]

[[routes]]
sources = ["agents_router"]
providers = ["feishu_lark"]
```

When this filter is set, Agents Router forwards a signal only when its `workspace.project_path`
is one of those paths or inside one of those paths. A project path must be a clean absolute path.
If a source does not provide `workspace.project_path`, filtered routes do not match.

Default behavior:

- If `only_forward_from_project_paths` is not set or is an empty array, notifications are not filtered by project path.
- Setup does not ask for this option. Add it manually when you want a route to forward only selected projects.
- The value is an array, so one route can allow multiple project paths.
- Paths must be absolute, non-empty, and must not contain `.` or `..` path components.
- Matching uses path components, not string prefixes. For example, `/Users/me/app` matches `/Users/me/app/api`, but does not match `/Users/me/app-copy`.
- If a route has both `only_forward_from_project_paths` and `minimum_task_duration_minutes`, both filters must match.

The running service automatically reloads valid config changes. If the service is not running,
start it:

```bash
agents-router start
```

If a manual edit makes the config invalid, the running service keeps the last valid config and logs
the reload failure.

## Result

Setup writes:

```text
~/.config/agents-router/config.toml
```

Then it starts the local service and sends a test notification through the same provider delivery path.
On macOS this is a LaunchAgent. On Linux this is a systemd user service. On Windows this is a Task Scheduler task.
