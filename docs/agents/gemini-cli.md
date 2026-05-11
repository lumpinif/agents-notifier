# Gemini CLI

中文文档：[gemini-cli.zh-CN.md](gemini-cli.zh-CN.md)

Use Gemini CLI integration when you want Gemini CLI hook events to submit notifications to the running Agents Notifier service.

Gemini CLI supports JSON settings files and lifecycle hooks such as `AfterAgent` and `Notification`. Agents Notifier uses those official hook events and receives only the title and body you choose to send.

Official Gemini CLI references:

- <https://google-gemini.github.io/gemini-cli/docs/cli/configuration.html>
- <https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/configuration.md>
- <https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/writing-hooks.md>
- <https://raw.githubusercontent.com/google-gemini/gemini-cli/main/schemas/settings.schema.json>

## What Agents Notifier Needs

Configure this source:

```toml
[[sources]]
id = "gemini_cli"
type = "agent_hook"
```

Then route `gemini_cli` to your provider.

Agents Notifier only needs a Gemini CLI hook to run this command:

```bash
agents-notifier emit \
  --source gemini_cli \
  --title "Gemini CLI" \
  --body "Gemini CLI finished a task."
```

`emit` submits the event to the local service ingress. It does not send provider notifications directly.

## Hook Example

Add hooks to your Gemini CLI settings file, such as `~/.gemini/settings.json` or your project `.gemini/settings.json`:

```json
{
  "hooksConfig": {
    "enabled": true
  },
  "hooks": {
    "AfterAgent": [
      {
        "matcher": "*",
        "hooks": [
          {
            "name": "agents-notifier-after-agent",
            "type": "command",
            "command": "agents-notifier emit --source gemini_cli --title \"Gemini CLI\" --body \"Gemini CLI finished a task.\"",
            "timeout": 10000
          }
        ]
      }
    ],
    "Notification": [
      {
        "matcher": "*",
        "hooks": [
          {
            "name": "agents-notifier-notification",
            "type": "command",
            "command": "agents-notifier emit --source gemini_cli --title \"Gemini CLI\" --body \"Gemini CLI needs your attention.\"",
            "timeout": 10000
          }
        ]
      }
    ]
  }
}
```

The Gemini CLI settings schema defines hook entries as `matcher` plus `hooks`, and command hooks run shell commands.

Keep this command in the runtime hook configuration. Do not ask the agent model to run it manually.

## Test the Route

```bash
agents-notifier emit \
  --source gemini_cli \
  --title "Gemini CLI" \
  --body "Test notification from Gemini CLI."
```

If your provider receives this notification, the Agents Notifier side is working.

## If It Fails

Check these first:

- The local service is running with `agents-notifier status`.
- Your config includes the `gemini_cli` source with `type = "agent_hook"`.
- Your route includes `gemini_cli`.
- The Gemini CLI settings file is valid JSON.
- `hooksConfig.enabled` is not disabled.
- `agents-notifier` is available in the shell environment Gemini CLI uses for hooks.
