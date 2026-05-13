# Gemini CLI

中文文档：[gemini-cli.zh-CN.md](gemini-cli.zh-CN.md)

Use Gemini CLI integration when you want Gemini CLI hook events to submit notifications to the running Agents Router service.

Gemini CLI supports JSON settings files and lifecycle hooks such as `AfterAgent` and `Notification`. Agents Router uses those official hook events and reads the hook JSON Gemini CLI sends on stdin.

Official Gemini CLI references:

- <https://google-gemini.github.io/gemini-cli/docs/cli/configuration.html>
- <https://github.com/google-gemini/gemini-cli/blob/main/docs/reference/configuration.md>
- <https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/writing-hooks.md>
- <https://raw.githubusercontent.com/google-gemini/gemini-cli/main/schemas/settings.schema.json>

## What Agents Router Needs

Configure this source:

```toml
[[sources]]
id = "gemini_cli"
type = "agent_hook"
```

Then route `gemini_cli` to your provider.

For structured notifications, configure Gemini CLI to run:

```bash
agents-router ingest --source gemini_cli --format gemini_cli_hook
```

`ingest` reads the hook payload from stdin and preserves fields Gemini CLI exposes, including project path, session id, timestamp, prompt, response, notification type, and message. If Gemini CLI includes `model`, Agents Router includes it in the structured signal. Agents Router validates `transcript_path` when Gemini CLI sends it, but does not forward that local path to providers.

If you only need a simple custom message, Gemini CLI can run this command instead:

```bash
agents-router emit \
  --source gemini_cli \
  --title "Gemini CLI" \
  --body "Gemini CLI finished a task."
```

`ingest` and `emit` submit the event to the local service ingress. They do not send provider notifications directly.

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
            "name": "agents-router-after-agent",
            "type": "command",
            "command": "agents-router ingest --source gemini_cli --format gemini_cli_hook",
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
            "name": "agents-router-notification",
            "type": "command",
            "command": "agents-router ingest --source gemini_cli --format gemini_cli_hook",
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

When structured hook stdin is not available, use the simple `emit` command shown above.

## Test the Route

```bash
agents-router emit \
  --source gemini_cli \
  --title "Gemini CLI" \
  --body "Test notification from Gemini CLI."
```

If your provider receives this notification, the Agents Router side is working.

## If It Fails

Check these first:

- The local service is running with `agents-router status`.
- Your config includes the `gemini_cli` source with `type = "agent_hook"`.
- Your route includes `gemini_cli`.
- The Gemini CLI settings file is valid JSON.
- `hooksConfig.enabled` is not disabled.
- Structured hooks use `agents-router ingest --source gemini_cli --format gemini_cli_hook`.
- `agents-router` is available in the shell environment Gemini CLI uses for hooks.
