# Hermes Agent CLI

Use Hermes Agent CLI integration when you want Hermes CLI turns to submit completion notifications to Agents Router.

Official Hermes Agent reference:

- <https://hermes-agent.nousresearch.com/docs/user-guide/features/hooks>

Hermes has gateway hooks, plugin hooks, and shell hooks. Gateway hooks only run in the gateway. For CLI coverage, use plugin hooks or shell hooks. For successful turn completion, `post_llm_call` is the clearest signal. For cleanup or failed/interrupted turns, `on_session_end` is available too.

## What Agents Router Needs

Configure this source:

```toml
[[sources]]
id = "hermes_agent_cli"
type = "agent_hook"
```

Then route `hermes_agent_cli` to your provider.

Agents Router only needs a Hermes plugin hook to run this command after a successful turn:

```bash
agents-router emit \
  --source hermes_agent_cli \
  --title "Hermes Agent CLI" \
  --body "Hermes Agent CLI finished a task."
```

If your Hermes hook receives structured fields such as session id, prompt, response, duration, or model, use the [Structured Agent Hook](structured-agent-hook.md) format with `--source hermes_agent_cli` instead of `emit`.

## Plugin Hook Example

Create a Hermes plugin that registers `post_llm_call`:

```python
import subprocess


def notify_agents_router(platform=None, **kwargs):
    if platform != "cli":
        return

    subprocess.Popen(
        [
            "agents-router",
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
    ctx.register_hook("post_llm_call", notify_agents_router)
```

Use `on_session_end` only if you also want notifications for failed or interrupted turns.

## Test the Route

```bash
agents-router emit \
  --source hermes_agent_cli \
  --title "Hermes Agent CLI" \
  --body "Test notification from Hermes Agent CLI."
```

If your provider receives this notification, the Agents Router side is working.

## If It Fails

Check these first:

- The local service is running with `agents-router status`.
- Your config includes the `hermes_agent_cli` source with `type = "agent_hook"`.
- Your route includes `hermes_agent_cli`.
- The Hermes plugin is loaded by CLI sessions, not only by the gateway.
- `agents-router` is available in the shell environment Hermes uses for plugins.
