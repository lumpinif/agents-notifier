# Claude Code

中文文档：[claude-code.zh-CN.md](claude-code.zh-CN.md)

Use Claude Code integration when you want Claude Code lifecycle hooks to submit completion or attention events to the running Agents Notifier service.

Claude Code hooks are user-defined commands that run at lifecycle points such as `Stop` and `Notification`. Agents Notifier uses that official hook path and reads the hook JSON Claude Code sends on stdin.

Official Claude Code hook reference: <https://code.claude.com/docs/en/hooks>

## What Agents Notifier Needs

For structured notifications, configure Claude Code to run:

```bash
agents-notifier ingest --source claude_code --format claude_code_hook
```

`ingest` reads the hook payload from stdin and preserves fields Claude Code exposes, including project path, session id, transcript path, attention message, and the last assistant message. If Claude Code includes `model`, Agents Notifier includes it in the structured signal.

If you only need a simple custom message, Claude Code can run this command instead:

```bash
agents-notifier emit \
  --source claude_code \
  --title "Claude Code" \
  --body "Claude Code finished a task."
```

`ingest` and `emit` do not send notifications directly. They submit the event to the local service ingress, and the service routes it to your configured providers.

## 1. Set Up the Service

Run:

```bash
agents-notifier setup
```

Choose:

```text
Claude Code
```

Then choose a provider.

## 2. Connect Claude Code

Add command hooks to your Claude Code settings. Use `Stop` when you want a notification after Claude finishes responding. Use `Notification` when you want Claude Code attention prompts to reach your provider too.

For a single machine, use:

```text
~/.claude/settings.json
```

For one project only, use:

```text
.claude/settings.local.json
```

Example:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "agents-notifier ingest --source claude_code --format claude_code_hook"
          }
        ]
      }
    ],
    "Notification": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "agents-notifier ingest --source claude_code --format claude_code_hook"
          }
        ]
      }
    ]
  }
}
```

Keep this command in the runtime hook configuration. Do not ask the agent model to run it manually.

When structured hook stdin is not available, use the simple `emit` command shown above.

## 3. Test the Route

After the service is running, test the same ingress path:

```bash
agents-notifier emit \
  --source claude_code \
  --title "Claude Code" \
  --body "Test notification from Claude Code."
```

If the provider receives this notification, the Agents Notifier side is working.

If Claude Code itself cannot run on your machine, this manual `emit` test is still the right local validation for Agents Notifier. It verifies the same local ingress, source adapter, router, and provider path that a Claude Code hook uses.

## If It Fails

Check these first:

- The local service is running:

```bash
agents-notifier status
```

- Your config includes the `claude_code` source.
- The route includes `claude_code`.
- The hook command uses `--source claude_code`.
- Structured hooks use `agents-notifier ingest --source claude_code --format claude_code_hook`.
- `agents-notifier` is available in the shell environment Claude Code uses for hooks.
