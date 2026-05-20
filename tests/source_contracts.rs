mod support;

use std::collections::BTreeMap;

use agents_router::config::SourceType;
use agents_router::local_ingress::{LocalIngressState, route_event_with_state};
use agents_router::signal::{
    SignalAnswerKind, SignalEvent, SignalEventKind, SignalLifecycleStatus, SignalWorkspace,
};
use agents_router::sources::{
    agent_hook, claude_code, codex_cli, gemini_cli, github_copilot_cli, opencode_cli,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use support::{SignalCaptureProvider, source_contract_config};

#[tokio::test]
async fn claude_code_session_start_model_is_merged_into_stop_signal() {
    let config = source_contract_config("claude_code", SourceType::ClaudeCode);
    let state = LocalIngressState::default();
    let capture = SignalCaptureProvider::new();

    let session_start = claude_code::local_event_from_hook(
        "claude_code",
        serde_json::from_str(include_str!(
            "fixtures/sources/claude_code/session_start.json"
        ))
        .expect("Claude Code SessionStart fixture should parse"),
    )
    .expect("Claude Code SessionStart fixture should create local event");
    route_event_with_state(&config, &[&capture], &state, session_start)
        .await
        .expect("SessionStart fixture should store session context");
    assert!(capture.signals().is_empty());

    let stop = claude_code::local_event_from_hook(
        "claude_code",
        serde_json::from_str(include_str!("fixtures/sources/claude_code/stop.json"))
            .expect("Claude Code Stop fixture should parse"),
    )
    .expect("Claude Code Stop fixture should create local event");
    route_event_with_state(&config, &[&capture], &state, stop)
        .await
        .expect("Stop fixture should route signal");

    let signals = capture.signals();
    assert_eq!(signals.len(), 1);
    let signal = &signals[0];
    assert_eq!(signal.source.id, "claude_code");
    assert_eq!(signal.source.source_type, "claude_code");
    assert_eq!(signal.event.kind, SignalEventKind::TurnCompleted);
    assert_eq!(signal.event.raw_name.as_deref(), Some("Stop"));
    assert_eq!(
        signal
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.session_id.as_deref()),
        Some("claude-session-1")
    );
    assert_eq!(
        signal
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.model.as_deref()),
        Some("claude-sonnet-4-6")
    );
    assert_eq!(
        signal
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.answer.as_ref())
            .map(|answer| (&answer.kind, answer.content.as_str())),
        Some((&SignalAnswerKind::Full, "Implemented the requested change."))
    );
}

#[tokio::test]
async fn claude_code_user_prompt_submit_start_is_merged_into_stop_duration() {
    let config = source_contract_config("claude_code", SourceType::ClaudeCode);
    let state = LocalIngressState::default();
    let capture = SignalCaptureProvider::new();
    let started_at = timestamp();
    let completed_at = started_at + ChronoDuration::minutes(7);

    let mut user_prompt = claude_code::local_event_from_hook(
        "claude_code",
        serde_json::from_str(include_str!(
            "fixtures/sources/claude_code/user_prompt_submit.json"
        ))
        .expect("Claude Code UserPromptSubmit fixture should parse"),
    )
    .expect("Claude Code UserPromptSubmit fixture should create local event");
    user_prompt
        .lifecycle
        .as_mut()
        .expect("UserPromptSubmit lifecycle should be present")
        .started_at = Some(started_at);

    route_event_with_state(&config, &[&capture], &state, user_prompt)
        .await
        .expect("UserPromptSubmit fixture should store turn start");
    assert!(capture.signals().is_empty());

    let mut stop = claude_code::local_event_from_hook(
        "claude_code",
        serde_json::from_str(include_str!("fixtures/sources/claude_code/stop.json"))
            .expect("Claude Code Stop fixture should parse"),
    )
    .expect("Claude Code Stop fixture should create local event");
    stop.lifecycle
        .as_mut()
        .expect("Stop lifecycle should be present")
        .completed_at = Some(completed_at);

    route_event_with_state(&config, &[&capture], &state, stop)
        .await
        .expect("Stop fixture should route signal");

    let signals = capture.signals();
    assert_eq!(signals.len(), 1);
    let lifecycle = signals[0]
        .lifecycle
        .as_ref()
        .expect("Stop signal should include lifecycle");
    assert_eq!(lifecycle.started_at, Some(started_at));
    assert_eq!(lifecycle.completed_at, Some(completed_at));
    assert_eq!(lifecycle.duration_ms, Some(420_000));
}

#[tokio::test]
async fn codex_cli_stop_fixture_routes_turn_completed_signal() {
    let config = source_contract_config("codex_cli", SourceType::CodexCli);
    let state = LocalIngressState::default();
    let capture = SignalCaptureProvider::new();
    let event = codex_cli::local_event_from_stop_hook(
        "codex_cli",
        serde_json::from_str(include_str!("fixtures/sources/codex_cli/stop.json"))
            .expect("Codex CLI Stop fixture should parse"),
    )
    .expect("Codex CLI Stop fixture should create local event");

    route_event_with_state(&config, &[&capture], &state, event)
        .await
        .expect("Codex CLI fixture should route signal");

    let signals = capture.signals();
    assert_eq!(signals.len(), 1);
    let signal = &signals[0];
    assert_eq!(signal.source.id, "codex_cli");
    assert_eq!(signal.event.kind, SignalEventKind::TurnCompleted);
    let conversation = signal
        .conversation
        .as_ref()
        .expect("conversation should be present");
    assert_eq!(conversation.session_id.as_deref(), Some("codex-session-1"));
    assert_eq!(conversation.turn_id.as_deref(), Some("turn-1"));
    assert_eq!(conversation.model.as_deref(), Some("gpt-5.2-codex"));
    assert_eq!(
        conversation
            .answer
            .as_ref()
            .map(|answer| answer.content.as_str()),
        Some("Ready for review.")
    );
}

fn timestamp() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[tokio::test]
async fn gemini_cli_after_agent_fixture_routes_turn_completed_signal() {
    let config = source_contract_config("gemini_cli", SourceType::AgentHook);
    let state = LocalIngressState::default();
    let capture = SignalCaptureProvider::new();
    let event = gemini_cli::local_event_from_hook(
        "gemini_cli",
        serde_json::from_str(include_str!("fixtures/sources/gemini_cli/after_agent.json"))
            .expect("Gemini CLI AfterAgent fixture should parse"),
    )
    .expect("Gemini CLI AfterAgent fixture should create local event");

    route_event_with_state(&config, &[&capture], &state, event)
        .await
        .expect("Gemini CLI fixture should route signal");

    let signals = capture.signals();
    assert_eq!(signals.len(), 1);
    let signal = &signals[0];
    assert_eq!(signal.source.id, "gemini_cli");
    assert_eq!(signal.source.source_type, "agent_hook");
    assert_eq!(signal.event.kind, SignalEventKind::TurnCompleted);
    assert_eq!(
        signal
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.project_name.as_deref()),
        Some("agents-router")
    );
    let conversation = signal
        .conversation
        .as_ref()
        .expect("conversation should be present");
    assert_eq!(conversation.session_id.as_deref(), Some("gemini-session-1"));
    assert_eq!(conversation.prompt.as_deref(), Some("Review this patch."));
    assert_eq!(conversation.model, None);
    assert_eq!(
        conversation
            .answer
            .as_ref()
            .map(|answer| answer.content.as_str()),
        Some("Ready for review.")
    );
}

#[tokio::test]
async fn github_copilot_notification_fixture_routes_custom_signal() {
    let config = source_contract_config("github_copilot_cli", SourceType::AgentHook);
    let state = LocalIngressState::default();
    let capture = SignalCaptureProvider::new();
    let event = github_copilot_cli::local_event_from_notification_hook(
        "github_copilot_cli",
        serde_json::from_str(include_str!(
            "fixtures/sources/github_copilot_cli/notification.json"
        ))
        .expect("GitHub Copilot CLI notification fixture should parse"),
    )
    .expect("GitHub Copilot CLI notification fixture should create local event");

    route_event_with_state(&config, &[&capture], &state, event)
        .await
        .expect("GitHub Copilot CLI fixture should route signal");

    let signals = capture.signals();
    assert_eq!(signals.len(), 1);
    let signal = &signals[0];
    assert_eq!(signal.source.id, "github_copilot_cli");
    assert_eq!(signal.event.kind, SignalEventKind::Custom);
    assert_eq!(
        signal.display.summary,
        "GitHub Copilot CLI emitted a notification."
    );
    assert_eq!(
        signal
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.session_id.as_deref()),
        Some("copilot-session-1")
    );
    assert_eq!(
        signal.metadata.get("notification_type").map(String::as_str),
        Some("agent_idle")
    );
}

#[tokio::test]
async fn opencode_cli_idle_session_routes_turn_completed_signal() {
    let config = source_contract_config("opencode_cli", SourceType::AgentHook);
    let state = LocalIngressState::default();
    let capture = SignalCaptureProvider::new();
    let event = opencode_cli::local_event_from_session_input(
        "opencode_cli",
        serde_json::from_str(
            r#"{
                "event": {
                    "type": "session.status",
                    "properties": {
                        "sessionID": "opencode-session-1",
                        "status": { "type": "idle" }
                    }
                },
                "cwd": "/Users/tester/projects/agents-router",
                "worktree": "main",
                "timestamp": "2026-05-12T10:15:30Z",
                "model": "opencode-model"
            }"#,
        )
        .expect("OpenCode fixture should parse"),
    )
    .expect("OpenCode fixture should create local event");

    route_event_with_state(&config, &[&capture], &state, event)
        .await
        .expect("OpenCode fixture should route signal");

    let signals = capture.signals();
    assert_eq!(signals.len(), 1);
    let signal = &signals[0];
    assert_eq!(signal.source.id, "opencode_cli");
    assert_eq!(signal.source.source_type, "agent_hook");
    assert_eq!(signal.event.kind, SignalEventKind::TurnCompleted);
    assert_eq!(
        signal
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.worktree.as_deref()),
        Some("main")
    );
    assert_eq!(
        signal
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.session_id.as_deref()),
        Some("opencode-session-1")
    );
    assert_eq!(
        signal
            .lifecycle
            .as_ref()
            .and_then(|lifecycle| lifecycle.status),
        Some(SignalLifecycleStatus::Completed)
    );
    assert_eq!(
        signal.metadata.get("status_type").map(String::as_str),
        Some("idle")
    );
}

#[tokio::test]
async fn generic_structured_agent_hook_preserves_user_supplied_metadata() {
    let config = source_contract_config("cursor_cli", SourceType::AgentHook);
    let state = LocalIngressState::default();
    let capture = SignalCaptureProvider::new();
    let mut metadata = BTreeMap::new();
    metadata.insert("user_key".to_string(), "user_value".to_string());
    let event = agent_hook::local_event_from_structured_hook(
        "cursor_cli",
        agent_hook::StructuredAgentHookInput {
            display: agent_hook::StructuredAgentHookDisplay {
                title: "Cursor CLI".to_string(),
                summary: "Cursor CLI finished a task.".to_string(),
            },
            event: SignalEvent {
                kind: SignalEventKind::TurnCompleted,
                raw_name: Some("wrapper.completed".to_string()),
            },
            workspace: Some(SignalWorkspace {
                cwd: Some("/Users/tester/projects/agents-router".to_string()),
                project_name: Some("agents-router".to_string()),
                project_path: Some("/Users/tester/projects/agents-router".to_string()),
                branch: None,
                worktree: None,
            }),
            conversation: None,
            lifecycle: None,
            links: Vec::new(),
            metadata,
        },
    )
    .expect("structured hook should create local event");

    route_event_with_state(&config, &[&capture], &state, event)
        .await
        .expect("structured hook should route signal");

    let signals = capture.signals();
    assert_eq!(signals.len(), 1);
    let signal = &signals[0];
    assert_eq!(signal.source.id, "cursor_cli");
    assert_eq!(signal.source.source_type, "agent_hook");
    assert_eq!(signal.event.raw_name.as_deref(), Some("wrapper.completed"));
    assert_eq!(
        signal.metadata.get("user_key").map(String::as_str),
        Some("user_value")
    );
}
