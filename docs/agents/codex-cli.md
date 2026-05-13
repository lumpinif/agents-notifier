# Codex CLI

中文文档：[codex-cli.zh-CN.md](codex-cli.zh-CN.md)

Use Codex CLI integration when you want a terminal Codex workflow to submit completion events to the running Agents Router service.

Official Codex CLI references:

- <https://developers.openai.com/codex/config-reference/>
- <https://developers.openai.com/codex/hooks/>
- <https://github.com/openai/codex>

## What Agents Router Needs

For structured notifications, configure Codex CLI to pipe its Stop hook JSON into:

```bash
agents-router ingest --source codex_cli --format codex_cli_stop
```

`ingest` reads the hook payload from stdin and preserves fields Codex CLI exposes, including project path, session id, turn id, model, and the last assistant message.

If you only need a simple custom message, Codex CLI can run this command instead:

```bash
agents-router emit \
  --source codex_cli \
  --title "Codex CLI" \
  --body "Codex CLI finished a task."
```

`ingest` and `emit` do not send notifications directly. They submit the event to the local service ingress, and the service routes it to your configured providers.

## 1. Set Up the Service

Run:

```bash
agents-router setup
```

Choose:

```text
Codex CLI
```

Then choose a provider.

## 2. Connect Codex CLI

Configure your Codex CLI Stop hook command to run the `agents-router ingest` command above with the hook JSON on stdin.

When structured hook stdin is not available, Codex CLI's `notify` setting can use the simple `emit` path. Add this to `~/.codex/config.toml`:


```toml
notify = [
  "agents-router",
  "emit",
  "--source",
  "codex_cli",
  "--title",
  "Codex CLI",
  "--body",
  "Codex CLI finished a task.",
]
```

Keep this command in the runtime hook configuration. Do not ask the agent model to run it manually.

## 3. Test the Route

After the service is running, test the same ingress path:

```bash
agents-router emit \
  --source codex_cli \
  --title "Codex CLI" \
  --body "Test notification from Codex CLI."
```

If the provider receives this notification, the Agents Router side is working.

## If It Fails

Check these first:

- The local service is running:

```bash
agents-router status
```

- Your config includes the `codex_cli` source.
- The route includes `codex_cli`.
- The hook command uses `--source codex_cli`.
