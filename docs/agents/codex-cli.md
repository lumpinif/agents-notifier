# Codex CLI

中文文档：[codex-cli.zh-CN.md](codex-cli.zh-CN.md)

Use Codex CLI integration when you want a terminal Codex workflow to submit completion events to the running Agents Notifier service.

## What Agents Notifier Needs

Agents Notifier only needs Codex CLI to run one command from its notification or hook mechanism:

```bash
agents-notifier emit \
  --source codex_cli \
  --title "Codex CLI" \
  --body "Codex CLI finished a task."
```

`emit` does not send notifications directly. It submits the event to the local service ingress, and the service routes it to your configured providers.

## 1. Set Up the Service

Run:

```bash
agents-notifier setup
```

Choose:

```text
Codex CLI
```

Then choose a provider.

## 2. Connect Codex CLI

Configure your Codex CLI notification or hook command to run the `agents-notifier emit` command above.

Keep this command in the runtime hook configuration. Do not ask the agent model to run it manually.

## 3. Test the Route

After the service is running, test the same ingress path:

```bash
agents-notifier emit \
  --source codex_cli \
  --title "Codex CLI" \
  --body "Test notification from Codex CLI."
```

If the provider receives this notification, the Agents Notifier side is working.

## If It Fails

Check these first:

- The local service is running:

```bash
agents-notifier status
```

- Your config includes the `codex_cli` source.
- The route includes `codex_cli`.
- The hook command uses `--source codex_cli`.
