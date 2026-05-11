# Hermes Agent CLI

Use Hermes Agent CLI integration when you want Hermes CLI turns to submit completion notifications to Agents Notifier.

Official Hermes Agent reference:

- <https://hermes-agent.nousresearch.com/docs/user-guide/features/hooks>

Hermes has gateway hooks, plugin hooks, and shell hooks. Gateway hooks only run in the gateway. For CLI coverage, use plugin hooks or shell hooks. For successful turn completion, `post_llm_call` is the clearest signal. For cleanup or failed/interrupted turns, `on_session_end` is available too.

## What Agents Notifier Needs

Configure this source:

```toml
[[sources]]
id = "hermes_agent_cli"
type = "agent_hook"
```

Then route `hermes_agent_cli` to your provider.

Agents Notifier only needs a Hermes plugin hook to run this command after a successful turn:

```bash
agents-notifier emit \
  --source hermes_agent_cli \
  --title "Hermes Agent CLI" \
  --body "Hermes Agent CLI finished a task."
```

## Plugin Hook Example

Create a Hermes plugin that registers `post_llm_call`:

```python
import subprocess


def notify_agents_notifier(platform=None, **kwargs):
    if platform != "cli":
        return

    subprocess.Popen(
        [
            "agents-notifier",
            "emit",
            "--source",
            "hermes_agent_cli",
            "--title",
            "Hermes Agent CLI",
            "--body",
            "Hermes Agent CLI finished a task.",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def register(ctx):
    ctx.register_hook("post_llm_call", notify_agents_notifier)
```

Use `on_session_end` only if you also want notifications for failed or interrupted turns.

## Test the Route

```bash
agents-notifier emit \
  --source hermes_agent_cli \
  --title "Hermes Agent CLI" \
  --body "Test notification from Hermes Agent CLI."
```

If your provider receives this notification, the Agents Notifier side is working.

## If It Fails

Check these first:

- The local service is running with `agents-notifier status`.
- Your config includes the `hermes_agent_cli` source with `type = "agent_hook"`.
- Your route includes `hermes_agent_cli`.
- The Hermes plugin is loaded by CLI sessions, not only by the gateway.
- `agents-notifier` is available in the shell environment Hermes uses for plugins.
