use std::sync::{Arc, Mutex};

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::time::{Duration, Instant};
#[cfg(unix)]
use tempfile::tempdir;
#[cfg(unix)]
use tokio::net::UnixListener;

use crate::config::{
    AnswerDetail, CliConfig, Config, LogConfig, NotificationConfig, PromptDetail, ProviderConfig,
    ProviderType, RouteConfig, SourceConfig, SourceType,
};
use crate::delivery::ProviderSendResult;
use crate::router::{Provider, ProviderFuture};
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

fn test_config(source_id: &str, source_type: SourceType) -> Config {
    Config {
        schema_version: 1,
        cli: CliConfig::default(),
        log: LogConfig::default(),
        notification: NotificationConfig::default(),
        sources: vec![SourceConfig {
            id: source_id.to_string(),
            source_type,
        }],
        providers: vec![ProviderConfig {
            id: "debug".to_string(),
            provider_type: ProviderType::Webhook,
            base_url: None,
            server: None,
            topic: None,
            url: Some("https://example.com/hook".to_string()),
            url_env: None,
            secret: None,
            secret_env: None,
            app_token: None,
            app_token_env: None,
            user_key: None,
            user_key_env: None,
            device: None,
            sound: None,
            bot_token: None,
            bot_token_env: None,
            chat_id: None,
            access_token: None,
            access_token_env: None,
            phone_number_id: None,
            recipient_phone_number: None,
            host: None,
            port: None,
            security: None,
            username: None,
            username_env: None,
            password: None,
            password_env: None,
            from: None,
            to: None,
            reply_to: None,
            token: None,
            token_env: None,
            recipient_user_id: None,
            context_token: None,
            context_token_env: None,
            route_tag: None,
        }],
        routes: vec![RouteConfig::new(
            vec![source_id.to_string()],
            vec!["debug".to_string()],
        )],
    }
}
