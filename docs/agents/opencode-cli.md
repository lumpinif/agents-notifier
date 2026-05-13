# OpenCode CLI

Use OpenCode CLI integration when you want OpenCode session completion events to submit notifications to Agents Router.

Official OpenCode references:

- <https://opencode.ai/docs/plugins/>
- <https://opencode.ai/docs/server/>
- <https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/session/status.ts>

OpenCode plugins can observe session events. The public docs list both `session.status` and `session.idle`; the current source still publishes `session.idle` for compatibility, but marks it deprecated. Prefer `session.status` with `status.type === "idle"`.

## What Agents Router Needs

Configure this source:

```toml
[[sources]]
id = "opencode_cli"
type = "agent_hook"
```

Then route `opencode_cli` to your provider.

For structured notifications, configure an OpenCode plugin to submit the idle session event:

```bash
agents-router ingest --source opencode_cli --format opencode_cli_session
```

`ingest` reads JSON from stdin and preserves fields OpenCode exposes through the plugin event and context, including project path, session id, status, worktree, and model when your plugin includes it.

If you only need a simple custom message, OpenCode can run this command when the session becomes idle:

```bash
agents-router emit \
  --source opencode_cli \
  --title "OpenCode CLI" \
  --body "OpenCode CLI finished a task."
```

## Plugin Example

Create `.opencode/plugins/agents-router.js`:

```javascript
import { spawn } from "node:child_process";

function submitAgentsRouterEvent(payload) {
  return new Promise((resolve, reject) => {
    const child = spawn("agents-router", [
      "ingest",
      "--source",
      "opencode_cli",
      "--format",
      "opencode_cli_session",
    ], {
      stdio: ["pipe", "ignore", "ignore"],
    });

    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`agents-router exited with code ${code}`));
      }
    });
    child.stdin.end(JSON.stringify(payload));
  });
}

export const AgentsRouter = async ({ directory, worktree }) => {
  return {
    event: async ({ event }) => {
      if (
        event.type !== "session.status" ||
        event.properties?.status?.type !== "idle"
      ) {
        return;
      }

      await submitAgentsRouterEvent({
        event,
        cwd: directory ?? process.cwd(),
        worktree,
      });
    },
  };
};
```

Project plugins in `.opencode/plugins/` are loaded by OpenCode at startup.

If your OpenCode plugin receives `directory`, `worktree`, or model information in its plugin context, include those fields in the payload as `cwd`, `worktree`, and `model`.

## Test the Route

```bash
agents-router emit \
  --source opencode_cli \
  --title "OpenCode CLI" \
  --body "Test notification from OpenCode CLI."
```

If your provider receives this notification, the Agents Router side is working.

## If It Fails

Check these first:

- The local service is running with `agents-router status`.
- Your config includes the `opencode_cli` source with `type = "agent_hook"`.
- Your route includes `opencode_cli`.
- The OpenCode plugin file is in `.opencode/plugins/` or `~/.config/opencode/plugins/`.
- Structured plugins call `agents-router ingest --source opencode_cli --format opencode_cli_session`.
- `agents-router` is available in the shell environment OpenCode uses for plugins.
