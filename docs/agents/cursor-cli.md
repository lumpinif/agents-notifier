# Cursor CLI

Use Cursor CLI integration when you run `cursor-agent` from a terminal and want a completion notification after a scripted run finishes.

Official Cursor CLI references:

- <https://docs.cursor.com/en/cli/overview>
- <https://docs.cursor.com/en/cli/using>
- <https://docs.cursor.com/en/cli/reference/output-format>

Cursor CLI's official docs describe interactive mode, non-interactive `--print` mode, and structured output formats. They do not currently document a native completion hook for the CLI. Because of that, Agents Notifier uses a small wrapper script for Cursor CLI instead of reading Cursor internals.

## What Agents Notifier Needs

Configure this source:

```toml
[[sources]]
id = "cursor_cli"
type = "agent_hook"
```

Then route `cursor_cli` to your provider.

Agents Notifier only needs your wrapper to run this command after Cursor CLI exits successfully:

```bash
agents-notifier emit \
  --source cursor_cli \
  --title "Cursor CLI" \
  --body "Cursor CLI finished a task."
```

`emit` submits the event to the local service ingress. It does not send provider notifications directly.

If your wrapper captures structured fields such as cwd, duration, prompt, answer, or model, use the [Structured Agent Hook](structured-agent-hook.md) format with `--source cursor_cli`.

## Wrapper Example

Create a wrapper such as `cursor-agent-notify`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cursor-agent -p "$@" --output-format text

agents-notifier emit \
  --source cursor_cli \
  --title "Cursor CLI" \
  --body "Cursor CLI finished a task."
```

Make it executable and use it instead of calling `cursor-agent -p` directly.

## Test the Route

```bash
agents-notifier emit \
  --source cursor_cli \
  --title "Cursor CLI" \
  --body "Test notification from Cursor CLI."
```

If your provider receives this notification, the Agents Notifier side is working.

## If It Fails

Check these first:

- The local service is running with `agents-notifier status`.
- Your config includes the `cursor_cli` source with `type = "agent_hook"`.
- Your route includes `cursor_cli`.
- `agents-notifier` is available in the shell environment used by your wrapper.
