use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use thiserror::Error;
use tracing::{info, warn};

use crate::config::Config;
use crate::signal::Signal;

pub type ProviderFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;

pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn provider_type(&self) -> &str;
    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryReport {
    pub matched_routes: usize,
    pub attempted: usize,
    pub succeeded: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFailure {
    pub provider_id: String,
    pub provider_type: String,
    pub error: String,
}

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("one or more providers failed")]
    ProviderFailures(Vec<ProviderFailure>),
}

pub struct Router<'a> {
    config: &'a Config,
}

impl<'a> Router<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self { config }
    }

    pub async fn route(
        &self,
        signal: &Signal,
        providers: &[&dyn Provider],
    ) -> Result<DeliveryReport, RouterError> {
        let providers_by_id: HashMap<&str, &dyn Provider> = providers
            .iter()
            .map(|provider| (provider.id(), *provider))
            .collect();
        let matching_routes: Vec<_> = self
            .config
            .routes
            .iter()
            .filter(|route| {
                route
                    .sources
                    .iter()
                    .any(|source| source == &signal.source_id)
            })
            .collect();

        info!(
            signal.id = %signal.id,
            source.id = %signal.source_id,
            matched_routes = matching_routes.len(),
            event = "route.matched",
        );

        let mut attempted = 0;
        let mut succeeded = 0;
        let mut failures = Vec::new();

        for route in &matching_routes {
            for provider_id in &route.providers {
                attempted += 1;
                let Some(provider) = providers_by_id.get(provider_id.as_str()) else {
                    failures.push(self.missing_provider_failure(provider_id));
                    continue;
                };

                info!(
                    signal.id = %signal.id,
                    source.id = %signal.source_id,
                    provider.id = %provider.id(),
                    provider.type = %provider.provider_type(),
                    event = "provider.send.started",
                );

                match provider.send(signal).await {
                    Ok(()) => {
                        succeeded += 1;
                        info!(
                            signal.id = %signal.id,
                            source.id = %signal.source_id,
                            provider.id = %provider.id(),
                            provider.type = %provider.provider_type(),
                            event = "provider.send.succeeded",
                        );
                    }
                    Err(error) => {
                        warn!(
                            signal.id = %signal.id,
                            source.id = %signal.source_id,
                            provider.id = %provider.id(),
                            provider.type = %provider.provider_type(),
                            error = %error,
                            event = "provider.send.failed",
                        );
                        failures.push(ProviderFailure {
                            provider_id: provider.id().to_string(),
                            provider_type: provider.provider_type().to_string(),
                            error: error.to_string(),
                        });
                    }
                }
            }
        }

        if failures.is_empty() {
            Ok(DeliveryReport {
                matched_routes: matching_routes.len(),
                attempted,
                succeeded,
            })
        } else {
            Err(RouterError::ProviderFailures(failures))
        }
    }

    fn missing_provider_failure(&self, provider_id: &str) -> ProviderFailure {
        let provider_type = self
            .config
            .provider(provider_id)
            .map(|provider| provider.provider_type.as_str())
            .unwrap_or("unknown");

        ProviderFailure {
            provider_id: provider_id.to_string(),
            provider_type: provider_type.to_string(),
            error: "provider adapter is not registered".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use anyhow::anyhow;
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::config::{
        Config, LogConfig, ProviderConfig, ProviderType, RouteConfig, SourceConfig, SourceType,
    };

    struct TestProvider {
        id: String,
        provider_type: String,
        calls: Arc<Mutex<Vec<String>>>,
        result: anyhow::Result<()>,
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

        fn failing(id: &str, calls: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                id: id.to_string(),
                provider_type: "test".to_string(),
                calls,
                result: Err(anyhow!("send failed")),
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
                    Ok(()) => Ok(()),
                    Err(error) => Err(anyhow!(error.to_string())),
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
        let phone = TestProvider::failing("phone", Arc::clone(&calls));
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
                provider_id: "phone".to_string(),
                provider_type: "test".to_string(),
                error: "send failed".to_string(),
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
            "Codex sent a notification.",
            timestamp,
            BTreeMap::new(),
        )
    }

    fn test_config(routes: Vec<RouteConfig>) -> Config {
        Config {
            schema_version: 1,
            log: LogConfig::default(),
            sources: vec![
                SourceConfig {
                    id: "codex_cli".to_string(),
                    source_type: SourceType::CodexCli,
                },
                SourceConfig {
                    id: "codex_desktop".to_string(),
                    source_type: SourceType::CodexDesktop,
                },
            ],
            providers: vec![
                ProviderConfig {
                    id: "phone".to_string(),
                    provider_type: ProviderType::Ntfy,
                    server: Some("https://ntfy.sh".to_string()),
                    topic: Some("topic".to_string()),
                    url: None,
                    url_env: None,
                    secret: None,
                    secret_env: None,
                },
                ProviderConfig {
                    id: "debug".to_string(),
                    provider_type: ProviderType::Webhook,
                    server: None,
                    topic: None,
                    url: Some("https://example.com/hook".to_string()),
                    url_env: None,
                    secret: None,
                    secret_env: None,
                },
            ],
            routes,
        }
    }
}
