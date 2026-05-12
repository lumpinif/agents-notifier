# Codex CLI

English documentation: [codex-cli.md](codex-cli.md)

当你希望终端里的 Codex CLI 工作流在任务完成时发通知，就使用 Codex CLI 集成。

## Agents Notifier 需要什么

结构化通知建议让 Codex CLI 把 Stop hook 的 JSON 通过 stdin 传给：

```bash
agents-notifier ingest --source codex_cli --format codex_cli_stop
```

`ingest` 会读取 hook payload，并保留 Codex CLI 明确暴露的字段，包括 project path、session id、turn id、model 和最后一条 assistant message。

如果只需要一条简单自定义消息，也可以让 Codex CLI 运行：

```bash
agents-notifier emit \
  --source codex_cli \
  --title "Codex CLI" \
  --body "Codex CLI finished a task."
```

`ingest` 和 `emit` 都不会直接发通知。它们只把事件提交给本机正在运行的 Agents Notifier service，然后由 service 按你的配置转发到 provider。

## 1. 设置 service

运行：

```bash
agents-notifier setup
```

选择：

```text
Codex CLI
```

然后选择 provider。

## 2. 连接 Codex CLI

把上面的 `agents-notifier ingest` 命令配置到 Codex CLI 的 Stop hook 里，并把 hook JSON 传入 stdin。

如果当前 Codex CLI 配置入口拿不到结构化 hook stdin，可以先用 notify 的简单 `emit` 路径。

这条命令应该由 Codex CLI runtime 自动触发，不要让模型在对话里手动运行它。

## 3. 测试链路

service 运行后，用同一条本机 ingress 路径测试：

```bash
agents-notifier emit \
  --source codex_cli \
  --title "Codex CLI" \
  --body "Test notification from Codex CLI."
```

如果 provider 收到这条通知，说明 Agents Notifier 这边已经正常。

## 如果失败

先检查这些：

- 本机 service 是否正在运行：

```bash
agents-notifier status
```

- 配置里是否有 `codex_cli` source。
- route 里是否包含 `codex_cli`。
- hook 命令是否使用了 `--source codex_cli`。
