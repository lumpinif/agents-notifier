# GitHub Copilot CLI

中文文档：[github-copilot-cli.zh-CN.md](github-copilot-cli.zh-CN.md)

Use GitHub Copilot CLI integration when you want Copilot CLI system notifications to submit events to the running Agents Notifier service.

GitHub Copilot CLI supports hooks loaded from `.github/hooks/*.json` in the current working directory. Agents Notifier uses the official `notification` hook path and reads the hook JSON Copilot CLI sends on stdin.

Official GitHub Copilot CLI references:

- <https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/use-hooks>
- <https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-hooks-reference>
- <https://docs.github.com/copilot/reference/cli-command-reference>

## What Agents Notifier Needs

Configure this source:

```toml
[[sources]]
id = "github_copilot_cli"
type = "agent_hook"
```

Then route `github_copilot_cli` to your provider.

For structured notifications, configure Copilot CLI to run:

```bash
agents-notifier ingest --source github_copilot_cli --format github_copilot_cli_notification
```

`ingest` reads the notification hook payload from stdin and preserves fields Copilot CLI exposes, including project path, session id, notification type, timestamp, title, and message.

If you only need a simple custom message, Copilot CLI can run this command instead:

```bash
agents-notifier emit \
  --source github_copilot_cli \
  --title "GitHub Copilot CLI" \
  --body "GitHub Copilot CLI emitted a notification."
```

`ingest` and `emit` submit the event to the local service ingress. They do not send provider notifications directly.

## Hook Example

Create `.github/hooks/agents-notifier.json`:

```json
{
  "version": 1,
  "hooks": {
    "notification": [
      {
        "type": "command",
        "bash": "agents-notifier ingest --source github_copilot_cli --format github_copilot_cli_notification",
        "powershell": "agents-notifier ingest --source github_copilot_cli --format github_copilot_cli_notification",
        "timeoutSec": 10
      }
    ]
  }
}
```

GitHub documents the `notification` hook as asynchronous and fire-and-forget. Hook failures do not block the Copilot CLI session.

Keep this command in the runtime hook configuration. Do not ask the agent model to run it manually.

When structured hook stdin is not available, use the simple `emit` command shown above.

## Test the Route

```bash
agents-notifier emit \
  --source github_copilot_cli \
  --title "GitHub Copilot CLI" \
  --body "Test notification from GitHub Copilot CLI."
```

If your provider receives this notification, the Agents Notifier side is working.

## If It Fails

Check these first:

- The local service is running with `agents-notifier status`.
- Your config includes the `github_copilot_cli` source with `type = "agent_hook"`.
- Your route includes `github_copilot_cli`.
- The hook file is valid JSON and is under `.github/hooks/`.
- Structured hooks use `agents-notifier ingest --source github_copilot_cli --format github_copilot_cli_notification`.
- `agents-notifier` is available in the shell environment GitHub Copilot CLI uses for hooks.
