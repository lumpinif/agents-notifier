# Aider

中文文档：[aider.zh-CN.md](aider.zh-CN.md)

Use Aider integration when you want Aider to notify you after the LLM finishes generating a response and is waiting for input.

Aider officially supports notifications and a custom `notifications_command`. Agents Router uses that command path and receives only the title and body you choose to send.

Official Aider reference:

- <https://aider.chat/docs/usage/notifications.html>

## What Agents Router Needs

Configure this source:

```toml
[[sources]]
id = "aider"
type = "agent_hook"
```

Then route `aider` to your provider.

Agents Router only needs Aider to run this command:

```bash
agents-router emit \
  --source aider \
  --title "Aider" \
  --body "Aider is ready for input."
```

`emit` submits the event to the local service ingress. It does not send provider notifications directly.

If you wrap Aider with your own script and can capture structured fields such as cwd, duration, prompt, answer, or model, use the [Structured Agent Hook](structured-agent-hook.md) format with `--source aider`.

## Command-Line Example

Run Aider with a custom notification command:

```bash
aider --notifications --notifications-command "agents-router emit --source aider --title \"Aider\" --body \"Aider is ready for input.\""
```

## Config Example

Add this to your Aider configuration file:

```yaml
notifications: true
notifications_command: "agents-router emit --source aider --title \"Aider\" --body \"Aider is ready for input.\""
```

Keep this command in Aider notification configuration. Do not ask the agent model to run it manually.

## Test the Route

```bash
agents-router emit \
  --source aider \
  --title "Aider" \
  --body "Test notification from Aider."
```

If your provider receives this notification, the Agents Router side is working.

## If It Fails

Check these first:

- The local service is running with `agents-router status`.
- Your config includes the `aider` source with `type = "agent_hook"`.
- Your route includes `aider`.
- Aider notifications are enabled.
- `agents-router` is available in the shell environment Aider uses for notification commands.
