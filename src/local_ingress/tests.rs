use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::time::{Duration, Instant};
use tempfile::TempDir;
#[cfg(unix)]
use tempfile::tempdir;
#[cfg(unix)]
use tokio::net::UnixListener;

use crate::config::{
    AnswerDetail, CliConfig, LogConfig, NotificationConfig, PromptDetail, ProviderType, RawConfig,
    RawProviderConfig, RouteConfig, SourceConfig, SourceType, ValidatedConfig,
};
use crate::delivery::ProviderSendResult;
use crate::delivery_safety::DeliverySafetyGuard;
use crate::router::{Provider, ProviderFuture};
use crate::runtime::RuntimeState;
use crate::signal::{
    Signal, SignalAnswerKind, SignalEvent, SignalEventKind, SignalLifecycle, SignalLifecycleStatus,
    SignalWorkspace,
};

use super::*;

#[cfg(unix)]
#[tokio::test]
async fn unix_ready_requires_accepting_service_not_just_socket_file() {
    let dir = tempdir().expect("tempdir should be created");
    let socket_path = dir.path().join("agents-router.sock");
    fs::write(&socket_path, "stale").expect("stale socket path should be written");

    assert!(!is_ready(&crate::paths::IngressEndpoint::UnixSocket(socket_path)).await);
}

#[cfg(unix)]
#[tokio::test]
async fn unix_ready_times_out_when_peer_does_not_respond() {
    let dir = tempdir().expect("tempdir should be created");
    let socket_path = dir.path().join("agents-router.sock");
    let listener = UnixListener::bind(&socket_path).expect("test socket should bind");
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.expect("client should connect");
        tokio::time::sleep(Duration::from_secs(60)).await;
    });

    let started = Instant::now();
    let ready = is_ready(&crate::paths::IngressEndpoint::UnixSocket(socket_path)).await;
    server.abort();

    assert!(!ready);
    assert!(started.elapsed() < Duration::from_secs(5));
}

struct TestProvider {
    calls: Arc<Mutex<Vec<String>>>,
}

struct SignalCaptureProvider {
    signals: Arc<Mutex<Vec<Signal>>>,
}

impl Provider for TestProvider {
    fn id(&self) -> &str {
        "debug"
    }

    fn provider_type(&self) -> &str {
        "test"
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("calls lock should not be poisoned")
                .push(format!("{}:{}", signal.source_id(), signal.summary()));
            Ok(ProviderSendResult::sent(
                self.id(),
                self.provider_type(),
                signal,
            ))
        })
    }
}

impl Provider for SignalCaptureProvider {
    fn id(&self) -> &str {
        "debug"
    }

    fn provider_type(&self) -> &str {
        "test"
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            self.signals
                .lock()
                .expect("signals lock should not be poisoned")
                .push(signal.clone());
            Ok(ProviderSendResult::sent(
                self.id(),
                self.provider_type(),
                signal,
            ))
        })
    }
}

#[tokio::test]
async fn route_event_creates_codex_cli_signal_and_uses_service_router() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };

    route_event(
        &test_config("codex_cli", SourceType::CodexCli),
        &[&provider],
        LocalSignalEvent::new("codex_cli", "Codex", "Ready for review."),
    )
    .await
    .expect("event should route through service router");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["codex_cli:Ready for review.".to_string()]
    );
}

#[tokio::test]
async fn route_event_creates_agents_router_signal_for_service_tests() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };

    route_event(
        &test_config("agents_router", SourceType::AgentsRouter),
        &[&provider],
        LocalSignalEvent::new(
            "agents_router",
            "Agents Router",
            "Test notification from your computer.",
        ),
    )
    .await
    .expect("service test event should route through service router");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["agents_router:Test notification from your computer.".to_string()]
    );
}

#[tokio::test]
async fn route_event_creates_claude_code_signal_and_uses_service_router() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };

    route_event(
        &test_config("claude_code", SourceType::ClaudeCode),
        &[&provider],
        LocalSignalEvent::new("claude_code", "Claude Code", "Claude Code finished a task."),
    )
    .await
    .expect("Claude Code event should route through service router");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["claude_code:Claude Code finished a task.".to_string()]
    );
}

#[tokio::test]
async fn route_event_merges_claude_session_start_model_into_stop_signal() {
    let signals = Arc::new(Mutex::new(Vec::new()));
    let provider = SignalCaptureProvider {
        signals: Arc::clone(&signals),
    };
    let state = LocalIngressState::default();
    let config = test_config("claude_code", SourceType::ClaudeCode);

    let mut session_start =
        LocalSignalEvent::new("claude_code", "Claude Code", "Claude Code session started.");
    session_start.action = LocalSignalAction::StoreSessionContext;
    session_start.conversation = Some(LocalSignalConversation {
        session_id: Some("session-1".to_string()),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: None,
        model: Some("claude-sonnet-4-6".to_string()),
    });

    route_event_with_state(&config, &[&provider], &state, session_start)
        .await
        .expect("SessionStart should store Claude Code session context");
    assert!(
        signals
            .lock()
            .expect("signals lock should not be poisoned")
            .is_empty(),
        "SessionStart should not route a provider notification"
    );

    let mut stop =
        LocalSignalEvent::new("claude_code", "Claude Code", "Claude Code finished a task.");
    stop.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some("Stop".to_string()),
    });
    stop.conversation = Some(LocalSignalConversation {
        session_id: Some("session-1".to_string()),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: Some("Ready for review.".to_string()),
        model: None,
    });

    route_event_with_state(&config, &[&provider], &state, stop)
        .await
        .expect("Stop should route a Claude Code completion signal");

    let routed = signals.lock().expect("signals lock should not be poisoned");
    assert_eq!(routed.len(), 1);
    assert_eq!(
        routed[0]
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.model.as_deref()),
        Some("claude-sonnet-4-6")
    );
}

#[tokio::test]
async fn route_event_computes_claude_duration_from_user_prompt_submit_and_stop() {
    let signals = Arc::new(Mutex::new(Vec::new()));
    let provider = SignalCaptureProvider {
        signals: Arc::clone(&signals),
    };
    let state = LocalIngressState::default();
    let mut config = test_config("claude_code", SourceType::ClaudeCode);
    config.routes[0].minimum_task_duration_minutes = Some(5);
    let started_at = timestamp();
    let completed_at = started_at + ChronoDuration::minutes(7);

    let mut user_prompt = LocalSignalEvent::new(
        "claude_code",
        "Claude Code",
        "Claude Code turn timing started.",
    );
    user_prompt.action = LocalSignalAction::StoreTurnStart;
    user_prompt.conversation = Some(LocalSignalConversation {
        session_id: Some("session-1".to_string()),
        session_title: None,
        turn_id: None,
        prompt: Some("Do not store this prompt.".to_string()),
        answer: None,
        model: None,
    });
    user_prompt.lifecycle = Some(SignalLifecycle {
        status: None,
        started_at: Some(started_at),
        completed_at: None,
        duration_ms: None,
    });

    route_event_with_state(&config, &[&provider], &state, user_prompt)
        .await
        .expect("UserPromptSubmit should store Claude Code turn start");
    assert!(
        signals
            .lock()
            .expect("signals lock should not be poisoned")
            .is_empty(),
        "UserPromptSubmit should not route a provider notification"
    );

    let mut stop =
        LocalSignalEvent::new("claude_code", "Claude Code", "Claude Code finished a task.");
    stop.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some("Stop".to_string()),
    });
    stop.conversation = Some(LocalSignalConversation {
        session_id: Some("session-1".to_string()),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: Some("Ready for review.".to_string()),
        model: None,
    });
    stop.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at: Some(completed_at),
        duration_ms: None,
    });

    route_event_with_state(&config, &[&provider], &state, stop)
        .await
        .expect("Stop should route a Claude Code completion signal");

    let routed = signals.lock().expect("signals lock should not be poisoned");
    assert_eq!(routed.len(), 1);
    let lifecycle = routed[0]
        .lifecycle
        .as_ref()
        .expect("computed lifecycle should be present");
    assert_eq!(lifecycle.started_at, Some(started_at));
    assert_eq!(lifecycle.completed_at, Some(completed_at));
    assert_eq!(lifecycle.duration_ms, Some(420_000));
    assert_eq!(
        routed[0]
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.prompt.as_deref()),
        None
    );
}

#[tokio::test]
async fn route_event_uses_explicit_stop_start_when_computing_claude_duration() {
    let signals = Arc::new(Mutex::new(Vec::new()));
    let provider = SignalCaptureProvider {
        signals: Arc::clone(&signals),
    };
    let state = LocalIngressState::default();
    let config = test_config("claude_code", SourceType::ClaudeCode);
    let stored_started_at = timestamp();
    let explicit_started_at = stored_started_at + ChronoDuration::minutes(2);
    let completed_at = stored_started_at + ChronoDuration::minutes(7);

    let mut user_prompt = LocalSignalEvent::new(
        "claude_code",
        "Claude Code",
        "Claude Code turn timing started.",
    );
    user_prompt.action = LocalSignalAction::StoreTurnStart;
    user_prompt.conversation = Some(LocalSignalConversation {
        session_id: Some("session-1".to_string()),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: None,
        model: None,
    });
    user_prompt.lifecycle = Some(SignalLifecycle {
        status: None,
        started_at: Some(stored_started_at),
        completed_at: None,
        duration_ms: None,
    });

    route_event_with_state(&config, &[&provider], &state, user_prompt)
        .await
        .expect("UserPromptSubmit should store Claude Code turn start");

    let mut stop =
        LocalSignalEvent::new("claude_code", "Claude Code", "Claude Code finished a task.");
    stop.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some("Stop".to_string()),
    });
    stop.conversation = Some(LocalSignalConversation {
        session_id: Some("session-1".to_string()),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: Some("Ready for review.".to_string()),
        model: None,
    });
    stop.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: Some(explicit_started_at),
        completed_at: Some(completed_at),
        duration_ms: None,
    });

    route_event_with_state(&config, &[&provider], &state, stop)
        .await
        .expect("Stop should route a Claude Code completion signal");

    let routed = signals.lock().expect("signals lock should not be poisoned");
    assert_eq!(routed.len(), 1);
    let lifecycle = routed[0]
        .lifecycle
        .as_ref()
        .expect("computed lifecycle should be present");
    assert_eq!(lifecycle.started_at, Some(explicit_started_at));
    assert_eq!(lifecycle.completed_at, Some(completed_at));
    assert_eq!(lifecycle.duration_ms, Some(300_000));
}

#[tokio::test]
async fn route_event_creates_generic_agent_hook_signal_and_uses_service_router() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };

    route_event(
        &test_config("opencode_cli", SourceType::AgentHook),
        &[&provider],
        LocalSignalEvent::new(
            "opencode_cli",
            "OpenCode CLI",
            "OpenCode CLI finished a task.",
        ),
    )
    .await
    .expect("generic agent hook event should route through service router");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["opencode_cli:OpenCode CLI finished a task.".to_string()]
    );
}

#[tokio::test]
async fn route_event_supplied_duration_matches_duration_filter_for_agent_hook() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };
    let mut config = test_config("aider", SourceType::AgentHook);
    config.routes[0].minimum_task_duration_minutes = Some(5);
    let mut event = LocalSignalEvent::new("aider", "Aider", "Aider finished a long-running task.");
    event.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at: None,
        duration_ms: Some(300_000),
    });

    route_event(&config, &[&provider], event)
        .await
        .expect("supplied duration should satisfy the route filter");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["aider:Aider finished a long-running task.".to_string()]
    );
}

#[tokio::test]
async fn route_event_rejects_codex_desktop_local_ingress() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };

    let err = route_event(
        &test_config("codex_desktop", SourceType::CodexDesktop),
        &[&provider],
        LocalSignalEvent::new("codex_desktop", "Codex Desktop", "Body"),
    )
    .await
    .expect_err("Codex Desktop should not accept local ingress events");

    assert!(err.to_string().contains("local ingress only accepts"));
    assert!(
        calls
            .lock()
            .expect("calls lock should not be poisoned")
            .is_empty()
    );
}

#[tokio::test]
async fn route_event_ignores_desktop_origin_codex_cli_stop_when_desktop_source_is_enabled() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };
    let sessions_dir = write_codex_session("session-1", "Codex Desktop");
    let state = LocalIngressState::default();

    route_event_with_state_and_codex_sessions_dir(
        &codex_desktop_and_cli_config(),
        &[&provider],
        &state,
        codex_stop_event("session-1"),
        Some(sessions_dir.path()),
    )
    .await
    .expect("desktop-origin hook should be ignored");

    assert!(
        calls
            .lock()
            .expect("calls lock should not be poisoned")
            .is_empty()
    );
}

#[tokio::test]
async fn route_event_keeps_unknown_origin_codex_cli_stop_when_desktop_source_is_enabled() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };
    let sessions_dir = TempDir::new().expect("sessions dir should be created");
    let state = LocalIngressState::default();

    route_event_with_state_and_codex_sessions_dir(
        &codex_desktop_and_cli_config(),
        &[&provider],
        &state,
        codex_stop_event("unknown-session-1"),
        Some(sessions_dir.path()),
    )
    .await
    .expect("unknown-origin hook should route");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["codex_cli:Codex CLI finished a task.".to_string()]
    );
}

#[tokio::test]
async fn delivery_safety_reset_request_clears_live_guard() {
    let dir = TempDir::new().expect("tempdir should be created");
    let state_path = dir.path().join("delivery-safety-state.json");
    std::fs::write(
        &state_path,
        r#"{
  "schema_version": 1,
  "global_pause": {
    "paused_at": "2026-05-14T00:00:00Z",
    "reason": "multiple_duplicate_storms",
    "message_fingerprint_count": 3,
    "window_seconds": 120
  }
}"#,
    )
    .expect("safety state should be written");
    let guard = DeliverySafetyGuard::load(state_path.clone()).expect("guard should load pause");
    assert!(guard.status().expect("status should load").paused);
    let runtime = RuntimeState::new_with_delivery_safety(
        test_config("codex_cli", SourceType::CodexCli),
        guard.clone(),
    )
    .expect("runtime should be created");
    let state = LocalIngressState::default();

    let response = handle_request_bytes(&runtime, &state, br#"{"kind":"delivery_safety_reset"}"#)
        .await
        .expect("reset request should be handled");

    let response: serde_json::Value =
        serde_json::from_slice(&response).expect("response should be JSON");
    assert_eq!(response["ok"], true);
    assert!(!guard.status().expect("status should load").paused);
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(state_path).expect("state should be readable"))
            .expect("state should be JSON");
    assert_eq!(persisted["global_pause"], serde_json::Value::Null);
}

#[test]
fn local_signal_event_is_converted_to_draft_at_single_ingress_boundary() {
    let mut metadata = BTreeMap::new();
    metadata.insert("safe_key".to_string(), "safe_value".to_string());
    let mut event =
        LocalSignalEvent::new("gemini_cli", "Gemini CLI", "Gemini CLI finished a task.");
    event.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some("AfterAgent".to_string()),
    });
    event.workspace = Some(SignalWorkspace {
        cwd: Some("/Users/tester/projects/agents-router".to_string()),
        project_name: Some("agents-router".to_string()),
        project_path: Some("/Users/tester/projects/agents-router".to_string()),
        branch: None,
        worktree: None,
    });
    event.conversation = Some(LocalSignalConversation {
        session_id: Some("session-1".to_string()),
        session_title: None,
        turn_id: None,
        prompt: Some("Review this patch.".to_string()),
        answer: Some("Ready for review.".to_string()),
        model: Some("gemini-3-pro".to_string()),
    });
    event.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at: Some(timestamp()),
        duration_ms: Some(420_000),
    });
    event.metadata = metadata;

    let draft = event.into_signal_draft(SourceType::AgentHook);

    assert_eq!(draft.source_id, "gemini_cli");
    assert_eq!(draft.expected_source_type, SourceType::AgentHook);
    assert_eq!(draft.display.title, "Gemini CLI");
    assert_eq!(draft.display.summary, "Gemini CLI finished a task.");
    assert_eq!(draft.event.kind, SignalEventKind::TurnCompleted);
    assert_eq!(draft.event.raw_name.as_deref(), Some("AfterAgent"));
    assert_eq!(
        draft
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.project_name.as_deref()),
        Some("agents-router")
    );
    assert_eq!(
        draft
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.prompt.as_deref()),
        Some("Review this patch.")
    );
    assert_eq!(
        draft
            .lifecycle
            .as_ref()
            .and_then(|lifecycle| lifecycle.duration_ms),
        Some(420_000)
    );
    assert_eq!(
        draft.metadata.get("safe_key").map(String::as_str),
        Some("safe_value")
    );
}

#[test]
fn create_signal_applies_structured_fields_with_notification_policy() {
    let mut config = test_config("codex_cli", SourceType::CodexCli);
    config.notification = NotificationConfig {
        answer_detail: AnswerDetail::Preview,
        prompt_detail: PromptDetail::Off,
    };

    let mut event = LocalSignalEvent::new("codex_cli", "Codex CLI", "Codex CLI finished a task.");
    event.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some("Stop".to_string()),
    });
    event.workspace = Some(SignalWorkspace {
        cwd: Some("/Users/tester/projects/agents-router".to_string()),
        project_name: Some("agents-router".to_string()),
        project_path: Some("/Users/tester/projects/agents-router".to_string()),
        branch: None,
        worktree: None,
    });
    event.conversation = Some(LocalSignalConversation {
        session_id: Some("session-1".to_string()),
        session_title: None,
        turn_id: Some("turn-1".to_string()),
        prompt: Some("Do not include this prompt.".to_string()),
        answer: Some(format!("{} done", "word ".repeat(120))),
        model: Some("gpt-5.2-codex".to_string()),
    });
    event.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at: None,
        duration_ms: None,
    });

    let signal =
        create_signal(&config, event).expect("structured local event should create signal");
    let conversation = signal
        .conversation
        .as_ref()
        .expect("conversation should be present");
    let answer = conversation
        .answer
        .as_ref()
        .expect("answer preview should be present");

    assert_eq!(&signal.event.kind, &SignalEventKind::TurnCompleted);
    assert_eq!(signal.event.raw_name.as_deref(), Some("Stop"));
    assert_eq!(
        signal
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.project_name.as_deref()),
        Some("agents-router")
    );
    assert_eq!(conversation.prompt, None);
    assert_eq!(conversation.model.as_deref(), Some("gpt-5.2-codex"));
    assert_eq!(answer.kind, SignalAnswerKind::Preview);
    assert!(answer.content.len() <= 363);
    assert!(answer.content.ends_with("..."));
}

fn test_config(source_id: &str, source_type: SourceType) -> ValidatedConfig {
    let mut provider = RawProviderConfig::new("debug", ProviderType::Webhook);
    provider.url = Some("https://example.com/hook".to_string());

    RawConfig {
        schema_version: 1,
        cli: CliConfig::default(),
        log: LogConfig::default(),
        notification: NotificationConfig::default(),
        sources: vec![SourceConfig {
            id: source_id.to_string(),
            source_type,
        }],
        providers: vec![provider],
        routes: vec![RouteConfig::new(
            vec![source_id.to_string()],
            vec!["debug".to_string()],
        )],
    }
    .validate()
    .expect("test config should validate")
}

fn codex_desktop_and_cli_config() -> ValidatedConfig {
    let mut config = test_config("codex_cli", SourceType::CodexCli);
    config.sources.push(SourceConfig {
        id: "codex_desktop".to_string(),
        source_type: SourceType::CodexDesktop,
    });
    config.routes[0].sources.push("codex_desktop".to_string());
    config
}

fn codex_stop_event(session_id: &str) -> LocalSignalEvent {
    let mut event = LocalSignalEvent::new("codex_cli", "Codex CLI", "Codex CLI finished a task.");
    event.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some("Stop".to_string()),
    });
    event.conversation = Some(LocalSignalConversation {
        session_id: Some(session_id.to_string()),
        session_title: None,
        turn_id: Some("turn-1".to_string()),
        prompt: None,
        answer: Some("Ready for review.".to_string()),
        model: Some("gpt-5.2-codex".to_string()),
    });
    event.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at: None,
        duration_ms: None,
    });
    event
}

fn write_codex_session(session_id: &str, originator: &str) -> TempDir {
    let dir = TempDir::new().expect("sessions dir should be created");
    let path = dir
        .path()
        .join(format!("rollout-2026-05-14T00-00-00-{session_id}.jsonl"));
    std::fs::write(
        path,
        format!(
            r#"{{"timestamp":"2026-05-14T00:00:00.000Z","type":"session_meta","payload":{{"id":"{session_id}","originator":"{originator}","cwd":"/Users/tester/projects/agents-router","model":"gpt-5.2-codex"}}}}
"#
        ),
    )
    .expect("session meta should be written");
    dir
}

fn timestamp() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
