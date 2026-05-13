# OpenClaw

Use OpenClaw integration when you want OpenClaw agent completion events to submit notifications to Agents Router.

Official OpenClaw references:

- <https://docs.openclaw.ai/plugins/hooks>
- <https://docs.openclaw.ai/automation/hooks>

OpenClaw has internal hooks and plugin hooks. For task completion, use the typed plugin hook `agent_end`. The internal `command:stop` event only means a user issued `/stop`; it is not a natural agent completion signal.

## What Agents Router Needs

Configure this source:

```toml
[[sources]]
id = "openclaw"
type = "agent_hook"
```

Then route `openclaw` to your provider.

Agents Router only needs an OpenClaw plugin hook to run this command from `agent_end`:

```bash
agents-router emit \
  --source openclaw \
  --title "OpenClaw" \
  --body "OpenClaw finished a task."
```

If your plugin can access structured agent data from `agent_end`, use the [Structured Agent Hook](structured-agent-hook.md) format with `--source openclaw` instead of `emit`.

## Plugin Hook Sketch

Use OpenClaw's plugin entry API and register `agent_end`:

```typescript
import { definePluginEntry } from "openclaw/plugin-sdk/plugin-entry";
import { spawn } from "node:child_process";

function emitNotification() {
  const child = spawn("agents-router", [
    "emit",
    "--source",
    "openclaw",
    "--title",
    "OpenClaw",
    "--body",
    "OpenClaw finished a task.",
  ], {
    stdio: "ignore",
    detached: true,
  });

  child.unref();
}

export default definePluginEntry({
  id: "agents-router",
  name: "Agents Router",
  register(api) {
    api.on("agent_end", async () => {
      emitNotification();
    });
  },
});
```

Keep the hook fast. Agents Router handles provider delivery in its own local service.

For non-bundled plugins, OpenClaw requires conversation hook access for hooks such as `agent_end`:

```json
{
  "plugins": {
    "entries": {
      "agents-router": {
        "hooks": {
          "allowConversationAccess": true
        }
      }
    }
  }
}
```

## Test the Route

```bash
agents-router emit \
  --source openclaw \
  --title "OpenClaw" \
  --body "Test notification from OpenClaw."
```

If your provider receives this notification, the Agents Router side is working.

## If It Fails

Check these first:

- The local service is running with `agents-router status`.
- Your config includes the `openclaw` source with `type = "agent_hook"`.
- Your route includes `openclaw`.
- The OpenClaw plugin is enabled and allowed to run conversation hooks.
- `agents-router` is available in the shell environment OpenClaw uses for plugins.
