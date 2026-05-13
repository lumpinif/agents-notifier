# Codex CLI

中文文档：[codex-cli.zh-CN.md](codex-cli.zh-CN.md)

Use Codex CLI integration when you want a terminal Codex workflow to submit completion events to the running Agents Router service.

Official Codex CLI references:

- <https://developers.openai.com/codex/config-reference/>
- <https://developers.openai.com/codex/hooks/>
- <https://github.com/openai/codex>

## What Agents Router Needs

Agents Router uses the Codex CLI Stop hook as the primary integration path.

Agents Router configures this automatically when your active config includes the canonical
`codex_cli` source. `agents-router setup`, `agents-router start`, `agents-router watch`, and
successful config hot reloads all ensure `~/.codex/config.toml` enables Codex hooks and has a Stop
hook that pipes the hook JSON into:

```bash
agents-router ingest --source codex_cli --format codex_cli_stop
```

`ingest` reads the hook payload from stdin and preserves fields Codex CLI exposes, including project path, session id, turn id, model, and the last assistant message.

`ingest` does not send notifications directly. It submits the event to the local service ingress, and the service routes it to your configured providers.

Agents Router does not overwrite Codex CLI `notify`. `notify` is a single command slot and may already be used by Codex Computer Use or another local integration. Stop hooks can coexist with that existing `notify` command, so they are the safest default.

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

Setup adds the Codex CLI Stop hook after it writes the Agents Router route.

If you manually edit the Agents Router config, use the canonical source id:

```toml
[[sources]]
id = "codex_cli"
type = "codex_cli"
```

Codex CLI has one global Stop hook, so custom source ids such as `my_codex` are rejected for this
source. When the running service hot reloads a valid config that includes `codex_cli`, it ensures
the Stop hook before using the new runtime config. If the hook cannot be written, reload fails and
the service keeps the last valid runtime config.

## 2. Manual Stop Hook

If you edit `~/.codex/config.toml` directly, use this shape:

```toml
[features]
codex_hooks = true

[[hooks.Stop]]
[[hooks.Stop.hooks]]
type = "command"
command = "agents-router ingest --source codex_cli --format codex_cli_stop"
timeout = 10
statusMessage = "Forwarding completion to Agents Router"
```

Keep this command in the runtime hook configuration. Do not ask the agent model to run it manually.

## 3. `notify` Fallback

Only use `notify` when you explicitly want to replace the current Codex CLI notify command or when your Codex CLI environment cannot use Stop hooks.

Before changing `notify`, inspect the existing value:

```bash
rg -n 'codex_hooks|hooks\.Stop|notify' ~/.codex/config.toml
```

If `notify` already points to another program, do not overwrite it unless you want to disconnect that program. Add the Stop hook above instead.

If you intentionally choose the simple notify fallback, make `notify` point to Agents Router:

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

This fallback sends a fixed message with `emit`. It does not include the structured Stop hook fields such as session id, turn id, model, or last assistant message.

## 4. Test the Route

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
- `~/.codex/config.toml` has `codex_hooks = true`.
- The Stop hook command uses `agents-router ingest --source codex_cli --format codex_cli_stop`.
- If you are using the `notify` fallback, `notify` points to `agents-router emit --source codex_cli`.
- `agents-router` is available in the shell environment Codex CLI uses for hooks.
