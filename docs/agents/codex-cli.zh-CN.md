# Codex CLI

English documentation: [codex-cli.md](codex-cli.md)

当你希望终端里的 Codex CLI 工作流在任务完成时发通知，就使用 Codex CLI 集成。

## Agents Router 需要什么

Agents Router 把 Codex CLI Stop hook 作为主接入路径。

只要当前生效的 Agents Router config 里包含 canonical 的 `codex_cli` source，Agents Router
就会自动配置这条链路。`agents-router setup`、`agents-router start`、`agents-router watch`
以及成功的 config hot reload，都会确保 `~/.codex/config.toml` 开启 Codex hooks，并有一个
Stop hook，把 hook JSON 通过 stdin 传给：

```bash
agents-router ingest --source codex_cli --format codex_cli_stop
```

`ingest` 会读取 hook payload，并保留 Codex CLI 明确暴露的字段，包括 project path、session id、turn id、model 和最后一条 assistant message。

`ingest` 不会直接发通知。它只把事件提交给本机正在运行的 Agents Router service，然后由 service 按你的配置转发到 provider。

Agents Router 不会覆盖 Codex CLI 的 `notify`。`notify` 是单个命令槽位，可能已经被 Codex Computer Use 或其他本地集成占用。Stop hook 可以和现有 `notify` 共存，所以它是默认、优先、长期更稳的方案。

Codex Desktop 和 Codex CLI 可以同时配置。它们可能共享同一份 `~/.codex/config.toml`，
所以 Codex Desktop 也可能触发 Stop hook。当 `codex_desktop` 已启用，并且 Agents Router
能证明这次 hook session 来自 Codex Desktop 时，这个 hook 会被忽略，由 Codex Desktop
watcher 作为权威来源处理。无法确认来源的 session 不会被忽略，会继续走 `codex_cli`，
避免误丢终端里的 Codex 通知。

## 1. 设置 service

运行：

```bash
agents-router setup
```

选择：

```text
Codex CLI
```

然后选择 provider。

setup 写完 Agents Router route 后，会自动添加 Codex CLI Stop hook。

如果你手动修改 Agents Router config，source id 必须使用 canonical 值：

```toml
[[sources]]
id = "codex_cli"
type = "codex_cli"
```

Codex CLI 只有一个全局 Stop hook，所以这个 source 不接受 `my_codex` 这类自定义 source id。
正在运行的 service hot reload 到包含 `codex_cli` 的有效 config 时，会先确保 Stop hook 已经写好，再使用新的 runtime config。如果 Stop hook 写不进去，这次 reload 会失败，service 继续使用上一份有效 runtime config。

如果同一份 config 里也包含 `codex_desktop`，并且你希望同时接收 Desktop 和终端 Codex
完成通知，就把两个 source 都保留在 route 里。只有当 Stop hook payload 能被识别为
Codex Desktop session 时，Agents Router 才会忽略这次 hook。

## 2. 手动配置 Stop hook

如果你直接修改 `~/.codex/config.toml`，应该使用这个形状：

```toml
[features]
hooks = true

[[hooks.Stop]]
[[hooks.Stop.hooks]]
type = "command"
command = "agents-router ingest --source codex_cli --format codex_cli_stop"
timeout = 10
statusMessage = "Forwarding completion to Agents Router"
```

这条命令应该由 Codex CLI runtime hook 自动触发，不要让模型在对话里手动运行它。

## 3. `notify` fallback

只有在你明确想替换当前 Codex CLI notify 命令，或者当前 Codex CLI 环境不能使用 Stop hook 时，才使用 `notify`。

修改 `notify` 前，先看当前值：

```bash
rg -n '(^\s*hooks\s*=|hooks\.Stop|notify)' ~/.codex/config.toml
```

如果 `notify` 已经指向其他程序，不要直接覆盖，除非你确认要断开那个程序。更推荐添加上面的 Stop hook。

如果你明确选择简单 notify fallback，就让 `notify` 指向 Agents Router：

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

这个 fallback 通过 `emit` 发送固定消息。它不会包含 Stop hook 里的结构化字段，例如 session id、turn id、model 或最后一条 assistant message。

## 4. 测试链路

service 运行后，用同一条本机 ingress 路径测试：

```bash
agents-router emit \
  --source codex_cli \
  --title "Codex CLI" \
  --body "Test notification from Codex CLI."
```

如果 provider 收到这条通知，说明 Agents Router 这边已经正常。

## 如果失败

先检查这些：

- 本机 service 是否正在运行：

```bash
agents-router status
```

- 配置里是否有 `codex_cli` source。
- route 里是否包含 `codex_cli`。
- `~/.codex/config.toml` 的 `[features]` 下面是否有 `hooks = true`。
- Stop hook 命令是否是 `agents-router ingest --source codex_cli --format codex_cli_stop`。
- 如果你同时使用 Codex Desktop，是否已经配置了 `codex_desktop`，让 Desktop 完成通知由
  Desktop watcher 处理，而不是共享 Stop hook。
- 如果你使用 `notify` fallback，`notify` 是否指向 `agents-router emit --source codex_cli`。
- Codex CLI 运行 hooks 的 shell 环境里是否能找到 `agents-router`。
