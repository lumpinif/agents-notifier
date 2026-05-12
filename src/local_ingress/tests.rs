use std::sync::{Arc, Mutex};

use crate::config::{
    CliConfig, Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType, RouteConfig,
    SourceConfig, SourceType,
};
use crate::delivery::ProviderSendResult;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

use super::*;

struct TestProvider {
    calls: Arc<Mutex<Vec<String>>>,
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

#[tokio::test]
async fn route_event_creates_codex_cli_signal_and_uses_service_router() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };

    route_event(
        &test_config("codex_cli", SourceType::CodexCli),
        &[&provider],
        LocalSignalEvent {
            source_id: "codex_cli".to_string(),
            title: "Codex".to_string(),
            body: "Ready for review.".to_string(),
        },
    )
    .await
    .expect("event should route through service router");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["codex_cli:Ready for review.".to_string()]
    );
}

#[tokio::test]
async fn route_event_creates_agents_notifier_signal_for_service_tests() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = TestProvider {
        calls: Arc::clone(&calls),
    };

    route_event(
        &test_config("agents_notifier", SourceType::AgentsNotifier),
        &[&provider],
        LocalSignalEvent {
            source_id: "agents_notifier".to_string(),
            title: "Agents Notifier".to_string(),
            body: "Test notification from your computer.".to_string(),
        },
    )
    .await
    .expect("service test event should route through service router");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["agents_notifier:Test notification from your computer.".to_string()]
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
        LocalSignalEvent {
            source_id: "claude_code".to_string(),
            title: "Claude Code".to_string(),
            body: "Claude Code finished a task.".to_string(),
        },
    )
    .await
    .expect("Claude Code event should route through service router");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["claude_code:Claude Code finished a task.".to_string()]
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
        LocalSignalEvent {
            source_id: "opencode_cli".to_string(),
            title: "OpenCode CLI".to_string(),
            body: "OpenCode CLI finished a task.".to_string(),
        },
    )
    .await
    .expect("generic agent hook event should route through service router");

    assert_eq!(
        *calls.lock().expect("calls lock should not be poisoned"),
        vec!["opencode_cli:OpenCode CLI finished a task.".to_string()]
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
        LocalSignalEvent {
            source_id: "codex_desktop".to_string(),
            title: "Codex Desktop".to_string(),
            body: "Body".to_string(),
        },
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
        routes: vec![RouteConfig {
            sources: vec![source_id.to_string()],
            providers: vec!["debug".to_string()],
        }],
    }
}
