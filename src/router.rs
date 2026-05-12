use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use thiserror::Error;
use tracing::{info, warn};

use crate::config::Config;
use crate::delivery::{DeliveryError, DeliveryErrorKind, ProviderSendResult};
use crate::signal::Signal;

pub type ProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProviderSendResult, DeliveryError>> + Send + 'a>>;

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
    pub signal_id: String,
    pub source_id: String,
    pub provider_id: String,
    pub provider_type: String,
    pub kind: DeliveryErrorKind,
    pub message: String,
    pub http_status: Option<u16>,
    pub provider_code: Option<String>,
    pub retriable: bool,
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
                    failures.push(self.missing_provider_failure(signal, provider_id));
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
                    Ok(result) => {
                        succeeded += 1;
                        info!(
                            signal.id = %signal.id,
                            source.id = %signal.source_id,
                            provider.id = %provider.id(),
                            provider.type = %provider.provider_type(),
                            provider.status = %result.status.as_str(),
                            http.status = result.http_status,
                            event = "provider.send.succeeded",
                        );
                    }
                    Err(error) => {
                        warn!(
                            signal.id = %signal.id,
                            source.id = %signal.source_id,
                            provider.id = %provider.id(),
                            provider.type = %provider.provider_type(),
                            error.kind = %error.kind.as_str(),
                            error.phase = %error.context.phase.as_str(),
                            error.retriable = error.retriable,
                            http.status = error.http_status,
                            provider.code = error.provider_code.as_deref(),
                            error = %error.message,
                            event = "provider.send.failed",
                        );
                        failures.push(ProviderFailure {
                            signal_id: error.context.signal_id,
                            source_id: error.context.source_id,
                            provider_id: provider.id().to_string(),
                            provider_type: provider.provider_type().to_string(),
                            kind: error.kind,
                            message: error.message,
                            http_status: error.http_status,
                            provider_code: error.provider_code,
                            retriable: error.retriable,
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

    fn missing_provider_failure(&self, signal: &Signal, provider_id: &str) -> ProviderFailure {
        let provider_type = self
            .config
            .provider(provider_id)
            .map(|provider| provider.provider_type.as_str())
            .unwrap_or("unknown");

        ProviderFailure {
            signal_id: signal.id.clone(),
            source_id: signal.source_id.clone(),
            provider_id: provider_id.to_string(),
            provider_type: provider_type.to_string(),
            kind: DeliveryErrorKind::Config,
            message: "provider adapter is not registered".to_string(),
            http_status: None,
            provider_code: None,
            retriable: false,
        }
    }
}

#[cfg(test)]
mod tests;
