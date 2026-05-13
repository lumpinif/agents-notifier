use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use tracing::{debug, info, warn};

use crate::config::{Config, RouteConfig, is_clean_absolute_project_path};
use crate::delivery::{DeliveryError, DeliveryErrorKind, ProviderSendResult};
use crate::delivery_safety::{
    DeliveryAttempt, DeliverySafetyDecision, DeliverySafetyGuard, DeliverySuppression,
};
use crate::signal::Signal;

pub type ProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProviderSendResult, DeliveryError>> + Send + 'a>>;

pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn provider_type(&self) -> &str;
    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeliveryReport {
    pub matched_routes: usize,
    pub attempted: usize,
    pub succeeded: usize,
    pub suppressed: usize,
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

#[derive(Debug)]
pub enum RouterError {
    ProviderFailures(Vec<ProviderFailure>),
}

impl fmt::Display for RouterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderFailures(failures) => {
                write!(
                    f,
                    "{} provider failure{}",
                    failures.len(),
                    if failures.len() == 1 { "" } else { "s" }
                )?;
                for (index, failure) in failures.iter().take(3).enumerate() {
                    write!(
                        f,
                        "{}{} `{}` ({}) failed for signal `{}` from `{}`: {}",
                        if index == 0 { ": " } else { "; " },
                        failure.provider_type,
                        failure.provider_id,
                        failure.kind.as_str(),
                        failure.signal_id,
                        failure.source_id,
                        failure.message
                    )?;
                    if let Some(http_status) = failure.http_status {
                        write!(f, " http_status={http_status}")?;
                    }
                    if let Some(provider_code) = failure.provider_code.as_deref() {
                        write!(f, " provider_code={provider_code}")?;
                    }
                    if failure.retriable {
                        write!(f, " retriable=true")?;
                    }
                }
                if failures.len() > 3 {
                    write!(f, "; {} more provider failures omitted", failures.len() - 3)?;
                }
                Ok(())
            }
        }
    }
}

impl StdError for RouterError {}

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
        self.route_with_safety(signal, providers, None).await
    }

    pub async fn route_with_safety(
        &self,
        signal: &Signal,
        providers: &[&dyn Provider],
        delivery_safety: Option<&DeliverySafetyGuard>,
    ) -> Result<DeliveryReport, RouterError> {
        let providers_by_id: HashMap<&str, &dyn Provider> = providers
            .iter()
            .map(|provider| (provider.id(), *provider))
            .collect();
        let candidate_routes: Vec<_> = self
            .config
            .routes
            .iter()
            .enumerate()
            .filter(|(_, route)| route.matches_source(signal.source_id()))
            .collect();
        let candidate_route_count = candidate_routes.len();
        let mut matching_routes = Vec::new();
        let mut filtered_routes = 0;

        for (route_index, route) in candidate_routes {
            if let Some(reason) = route_filter_miss(signal, route) {
                filtered_routes += 1;
                debug!(
                    signal.id = %signal.id,
                    source.id = %signal.source_id(),
                    route.index = route_index,
                    reason = %reason.as_str(),
                    event = "route.filtered",
                );
                continue;
            }

            matching_routes.push(route);
        }

        info!(
            signal.id = %signal.id,
            source.id = %signal.source_id(),
            candidate_routes = candidate_route_count,
            matched_routes = matching_routes.len(),
            filtered_routes,
            event = "route.matched",
        );

        let mut attempted = 0;
        let mut succeeded = 0;
        let mut suppressed = 0;
        let mut failures = Vec::new();

        for route in &matching_routes {
            for provider_id in &route.providers {
                attempted += 1;
                let Some(provider) = providers_by_id.get(provider_id.as_str()) else {
                    failures.push(self.missing_provider_failure(signal, provider_id));
                    continue;
                };

                if let Some(delivery_safety) = delivery_safety {
                    match delivery_safety.check(DeliveryAttempt {
                        signal,
                        provider_id: provider.id(),
                    }) {
                        Ok(DeliverySafetyDecision::Allow { .. }) => {}
                        Ok(DeliverySafetyDecision::Suppress(suppression)) => {
                            suppressed += 1;
                            log_delivery_suppressed(signal, *provider, &suppression);
                            continue;
                        }
                        Err(error) => {
                            failures.push(ProviderFailure {
                                signal_id: signal.id.clone(),
                                source_id: signal.source_id().to_string(),
                                provider_id: provider.id().to_string(),
                                provider_type: provider.provider_type().to_string(),
                                kind: DeliveryErrorKind::Internal,
                                message: format!("delivery safety guard failed: {error}"),
                                http_status: None,
                                provider_code: None,
                                retriable: false,
                            });
                            continue;
                        }
                    }
                }

                info!(
                    signal.id = %signal.id,
                    source.id = %signal.source_id(),
                    provider.id = %provider.id(),
                    provider.type = %provider.provider_type(),
                    event = "provider.send.started",
                );

                match provider.send(signal).await {
                    Ok(result) => {
                        succeeded += 1;
                        info!(
                            signal.id = %signal.id,
                            source.id = %signal.source_id(),
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
                            source.id = %signal.source_id(),
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
                suppressed,
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
            source_id: signal.source_id().to_string(),
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

fn log_delivery_suppressed(
    signal: &Signal,
    provider: &dyn Provider,
    suppression: &DeliverySuppression,
) {
    warn!(
        signal.id = %signal.id,
        source.id = %signal.source_id(),
        provider.id = %provider.id(),
        provider.type = %provider.provider_type(),
        reason = %suppression.reason.as_str(),
        message.fingerprint = %suppression.message_fingerprint_hash,
        delivery.key = %suppression.delivery_key_hash,
        window.seconds = suppression.window_seconds,
        count = suppression.count,
        global.paused = suppression.global_paused,
        event = "delivery.suppressed",
    );

    if let Some(pause_started) = &suppression.pause_started {
        warn!(
            signal.id = %signal.id,
            source.id = %signal.source_id(),
            provider.id = %provider.id(),
            provider.type = %provider.provider_type(),
            message.fingerprint = %suppression.message_fingerprint_hash,
            message.fingerprint_count = pause_started.message_fingerprint_count,
            window.seconds = pause_started.window_seconds,
            event = "delivery.pause.started",
        );
    }
}

impl RouteConfig {
    fn matches_source(&self, source_id: &str) -> bool {
        self.sources.iter().any(|source| source == source_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RouteFilterMissReason {
    DurationTooShort,
    MissingProjectPath,
    ProjectPathNotAllowed,
}

impl RouteFilterMissReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::DurationTooShort => "duration_too_short",
            Self::MissingProjectPath => "missing_project_path",
            Self::ProjectPathNotAllowed => "project_path_not_allowed",
        }
    }
}

fn route_filter_miss(signal: &Signal, route: &RouteConfig) -> Option<RouteFilterMissReason> {
    if let Some(minimum_duration_ms) = route.minimum_task_duration_ms()
        && let Some(duration_ms) = signal
            .lifecycle
            .as_ref()
            .and_then(|lifecycle| lifecycle.duration_ms)
        && duration_ms < minimum_duration_ms
    {
        return Some(RouteFilterMissReason::DurationTooShort);
    }

    if !route.only_forward_from_project_paths.is_empty() {
        let Some(project_path) = signal
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.project_path.as_deref())
            .filter(|path| is_clean_absolute_project_path(path))
        else {
            return Some(RouteFilterMissReason::MissingProjectPath);
        };

        let allowed = route
            .only_forward_from_project_paths
            .iter()
            .any(|allowed_path| Path::new(project_path).starts_with(Path::new(allowed_path)));

        if !allowed {
            return Some(RouteFilterMissReason::ProjectPathNotAllowed);
        }
    }

    None
}

#[cfg(test)]
mod tests;
