use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};

use super::*;
use crate::config::{
    CliConfig, Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType, RouteConfig,
    SourceConfig, SourceType,
};
use crate::delivery::DeliveryErrorContext;

struct TestProvider {
    id: String,
    provider_type: String,
    calls: Arc<Mutex<Vec<String>>>,
    result: Result<(), DeliveryErrorKind>,
}

impl TestProvider {
    fn succeeding(id: &str, calls: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id: id.to_string(),
            provider_type: "test".to_string(),
            calls,
            result: Ok(()),
        }
    }

    fn failing(id: &str, calls: Arc<Mutex<Vec<String>>>, kind: DeliveryErrorKind) -> Self {
        Self {
            id: id.to_string(),
            provider_type: "test".to_string(),
            calls,
            result: Err(kind),
        }
    }
}

impl Provider for TestProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        &self.provider_type
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(signal.id.clone());
            match &self.result {
                Ok(()) => Ok(ProviderSendResult::sent(
                    &self.id,
                    &self.provider_type,
                    signal,
                )),
                Err(kind) => Err(DeliveryError::new(
                    *kind,
                    DeliveryErrorContext::provider_send(signal, &self.id, &self.provider_type),
                    "send failed",
                )),
            }
        })
    }
}

#[tokio::test]
async fn sends_signal_to_all_matching_providers() {
    let config = test_config(vec![RouteConfig {
        sources: vec!["codex_cli".to_string()],
        providers: vec!["phone".to_string(), "debug".to_string()],
    }]);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let phone = TestProvider::succeeding("phone", Arc::clone(&calls));
    let debug = TestProvider::succeeding("debug", Arc::clone(&calls));

    let report = Router::new(&config)
        .route(&test_signal("codex_cli"), &[&phone, &debug])
        .await
        .expect("providers should succeed");

    assert_eq!(report.matched_routes, 1);
    assert_eq!(report.attempted, 2);
    assert_eq!(report.succeeded, 2);
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn unmatched_signal_does_not_send() {
    let config = test_config(vec![RouteConfig {
        sources: vec!["codex_desktop".to_string()],
        providers: vec!["phone".to_string()],
    }]);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let phone = TestProvider::succeeding("phone", Arc::clone(&calls));

    let report = Router::new(&config)
        .route(&test_signal("codex_cli"), &[&phone])
        .await
        .expect("no matching route is not an error");

    assert_eq!(report.matched_routes, 0);
    assert_eq!(report.attempted, 0);
    assert_eq!(report.succeeded, 0);
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn provider_failure_does_not_block_other_providers() {
    let config = test_config(vec![RouteConfig {
        sources: vec!["codex_cli".to_string()],
        providers: vec!["phone".to_string(), "debug".to_string()],
    }]);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let phone = TestProvider::failing(
        "phone",
        Arc::clone(&calls),
        DeliveryErrorKind::ProviderRejected,
    );
    let debug = TestProvider::succeeding("debug", Arc::clone(&calls));

    let err = Router::new(&config)
        .route(&test_signal("codex_cli"), &[&phone, &debug])
        .await
        .expect_err("provider failure should be returned");

    assert_eq!(calls.lock().unwrap().len(), 2);
    let RouterError::ProviderFailures(failures) = err;
    assert_eq!(
        failures,
        vec![ProviderFailure {
            signal_id: "signal-1".to_string(),
            source_id: "codex_cli".to_string(),
            provider_id: "phone".to_string(),
            provider_type: "test".to_string(),
            kind: DeliveryErrorKind::ProviderRejected,
            message: "send failed".to_string(),
            http_status: None,
            provider_code: None,
            retriable: false,
        }]
    );
}

fn test_signal(source_id: &str) -> Signal {
    let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    Signal::new_with_timestamp(
        "signal-1",
        source_id,
        source_id,
        "Codex",
        "Ready for review.",
        timestamp,
        BTreeMap::new(),
    )
}

fn test_config(routes: Vec<RouteConfig>) -> Config {
    Config {
        schema_version: 1,
        cli: CliConfig::default(),
        log: LogConfig::default(),
        notification: NotificationConfig::default(),
        sources: vec![
            SourceConfig {
                id: "codex_cli".to_string(),
                source_type: SourceType::CodexCli,
            },
            SourceConfig {
                id: "codex_desktop".to_string(),
                source_type: SourceType::CodexDesktop,
            },
            SourceConfig {
                id: "claude_code".to_string(),
                source_type: SourceType::ClaudeCode,
            },
        ],
        providers: vec![
            ProviderConfig {
                id: "phone".to_string(),
                provider_type: ProviderType::Ntfy,
                base_url: None,
                server: Some("https://ntfy.sh".to_string()),
                topic: Some("topic".to_string()),
                url: None,
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
            },
            ProviderConfig {
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
            },
        ],
        routes,
    }
}
