# GitHub Copilot CLI

中文文档：[github-copilot-cli.zh-CN.md](github-copilot-cli.zh-CN.md)

Use GitHub Copilot CLI integration when you want Copilot CLI system notifications to submit events to the running Agents Notifier service.

GitHub Copilot CLI supports hooks loaded from `.github/hooks/*.json` in the current working directory. Agents Notifier uses the official `notification` hook path and receives only the title and body you choose to send.

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

Agents Notifier only needs a Copilot CLI hook to run this command:

```bash
agents-notifier emit \
  --source github_copilot_cli \
  --title "GitHub Copilot CLI" \
  --body "GitHub Copilot CLI emitted a notification."
```

`emit` submits the event to the local service ingress. It does not send provider notifications directly.

## Hook Example

Create `.github/hooks/agents-notifier.json`:

```json
{
  "version": 1,
  "hooks": {
    "notification": [
      {
        "type": "command",
        "bash": "agents-notifier emit --source github_copilot_cli --title \"GitHub Copilot CLI\" --body \"GitHub Copilot CLI emitted a notification.\"",
        "powershell": "agents-notifier emit --source github_copilot_cli --title \"GitHub Copilot CLI\" --body \"GitHub Copilot CLI emitted a notification.\"",
        "timeoutSec": 10
      }
    ]
  }
}
```

GitHub documents the `notification` hook as asynchronous and fire-and-forget. Hook failures do not block the Copilot CLI session.

Keep this command in the runtime hook configuration. Do not ask the agent model to run it manually.

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
- `agents-notifier` is available in the shell environment GitHub Copilot CLI uses for hooks.
