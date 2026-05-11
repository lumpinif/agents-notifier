# OpenCode CLI

Use OpenCode CLI integration when you want OpenCode session completion events to submit notifications to Agents Notifier.

Official OpenCode references:

- <https://opencode.ai/docs/plugins/>
- <https://opencode.ai/docs/server/>
- <https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/session/status.ts>

OpenCode plugins can observe session events. The public docs list both `session.status` and `session.idle`; the current source still publishes `session.idle` for compatibility, but marks it deprecated. Prefer `session.status` with `status.type === "idle"`.

## What Agents Notifier Needs

Configure this source:

```toml
[[sources]]
id = "opencode_cli"
type = "agent_hook"
```

Then route `opencode_cli` to your provider.

Agents Notifier only needs an OpenCode plugin to run this command when the session becomes idle:

```bash
agents-notifier emit \
  --source opencode_cli \
  --title "OpenCode CLI" \
  --body "OpenCode CLI finished a task."
```

## Plugin Example

Create `.opencode/plugins/agents-notifier.js`:

```javascript
export const AgentsNotifier = async ({ $ }) => {
  return {
    event: async ({ event }) => {
      if (
        event.type !== "session.status" ||
        event.properties?.status?.type !== "idle"
      ) {
        return;
      }

      await $`agents-notifier emit --source opencode_cli --title "OpenCode CLI" --body "OpenCode CLI finished a task."`;
    },
  };
};
```

Project plugins in `.opencode/plugins/` are loaded by OpenCode at startup.

## Test the Route

```bash
agents-notifier emit \
  --source opencode_cli \
  --title "OpenCode CLI" \
  --body "Test notification from OpenCode CLI."
```

If your provider receives this notification, the Agents Notifier side is working.

## If It Fails

Check these first:

- The local service is running with `agents-notifier status`.
- Your config includes the `opencode_cli` source with `type = "agent_hook"`.
- Your route includes `opencode_cli`.
- The OpenCode plugin file is in `.opencode/plugins/` or `~/.config/opencode/plugins/`.
- `agents-notifier` is available in the shell environment OpenCode uses for plugins.
