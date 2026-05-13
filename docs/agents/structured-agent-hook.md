# Structured Agent Hook

Use this format when an agent does not expose a native Agents Router integration, but your wrapper or plugin can write JSON safely.

Command:

```bash
agents-router ingest --source <source_id> --format agent_hook_event
```

The command reads one JSON object from stdin:

```json
{
  "display": {
    "title": "Cursor CLI",
    "summary": "Cursor CLI finished a task."
  },
  "event": {
    "kind": "turn_completed",
    "raw_name": "wrapper.completed"
  },
  "workspace": {
    "cwd": "/Users/alex/project",
    "project_name": "project",
    "project_path": "/Users/alex/project",
    "branch": "main"
  },
  "conversation": {
    "session_id": "session-1",
    "turn_id": "turn-1",
    "prompt": "Fix the failing test.",
    "answer": "The test now passes.",
    "model": "agent-model"
  },
  "lifecycle": {
    "status": "completed",
    "duration_ms": 1200
  },
  "metadata": {
    "wrapper": "cursor-agent-notify"
  }
}
```

Required fields:

- `display.title`
- `display.summary`
- `event.kind`

Valid `event.kind` values are `turn_completed` and `custom`.

Optional fields follow the shared Signal model. The service still applies the configured prompt and answer privacy policy before routing the notification to providers.

If your wrapper only needs to submit a title, body, and duration, the simpler `emit` command can also provide duration:

```bash
agents-router emit \
  --source cursor_cli \
  --title "Cursor CLI" \
  --body "Cursor CLI finished a task." \
  --duration-ms 420000
```
