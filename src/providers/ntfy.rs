use anyhow::anyhow;
use chrono::{DateTime, Local, Utc};

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::providers::formatting::body_with_local_time;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

const DEFAULT_PRIORITY: &str = "high";

#[derive(Debug)]
pub struct NtfyProvider {
    id: String,
    endpoint: String,
    client: reqwest::Client,
}

impl NtfyProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::Ntfy {
            return Err(anyhow!("provider `{}` is not an ntfy provider", config.id));
        }

        let server = required_field(config, "server", config.server.as_deref())?;
        let topic = required_field(config, "topic", config.topic.as_deref())?;

        Ok(Self {
            id: config.id.clone(),
            endpoint: format!(
                "{}/{}",
                server.trim_end_matches('/'),
                topic.trim_start_matches('/')
            ),
            client: provider_http_client()?,
        })
    }
}

impl Provider for NtfyProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::Ntfy.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::Ntfy.as_str();
            let response = self
                .client
                .post(&self.endpoint)
                .header("Title", &signal.title)
                .header("Priority", DEFAULT_PRIORITY)
                .body(format_ntfy_body(signal))
                .send()
                .await
                .map_err(|error| {
                    let is_timeout = error.is_timeout();
                    provider_request_error(
                        signal,
                        &self.id,
                        provider_type,
                        "ntfy",
                        is_timeout,
                        error.without_url(),
                    )
                })?;

            let status = response.status();
            if !status.is_success() {
                let status_code = status.as_u16();
                return Err(DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!(
                        "ntfy provider `{}` returned HTTP status {}",
                        self.id, status
                    ),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_http_status(status_code)));
            }

            Ok(ProviderSendResult::sent(&self.id, provider_type, signal)
                .with_http_status(status.as_u16()))
        })
    }
}

fn required_field<'a>(
    config: &ProviderConfig,
    field: &'static str,
    value: Option<&'a str>,
) -> anyhow::Result<&'a str> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("provider `{}` requires `{}`", config.id, field))
}

fn format_ntfy_body(signal: &Signal) -> String {
    body_with_local_time(&signal.body, &format_local_timestamp(signal.timestamp))
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::config::ProviderType;
    use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};

    #[tokio::test]
    async fn sends_ntfy_request_with_title_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/topic"))
            .and(header("Title", "Codex"))
            .and(header("Priority", "high"))
            .and(body_string(format!(
                "Ready for review.\nTime: {}",
                format_local_timestamp(test_timestamp())
            )))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let provider = NtfyProvider::from_config(&ProviderConfig {
            id: "phone".to_string(),
            provider_type: ProviderType::Ntfy,
            server: Some(server.uri()),
            topic: Some("topic".to_string()),
            url: None,
            url_env: None,
            secret: None,
            secret_env: None,
        })
        .expect("provider config should be valid");

        let result = provider
            .send(&test_signal())
            .await
            .expect("ntfy send should succeed");

        assert_eq!(result.provider_id, "phone");
        assert_eq!(result.provider_type, "ntfy");
        assert_eq!(result.signal_id, "signal-1");
        assert_eq!(result.status, ProviderSendStatus::Sent);
        assert_eq!(result.http_status, Some(200));
    }

    #[tokio::test]
    async fn returns_error_for_non_success_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/topic"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let provider = NtfyProvider::from_config(&ProviderConfig {
            id: "phone".to_string(),
            provider_type: ProviderType::Ntfy,
            server: Some(server.uri()),
            topic: Some("topic".to_string()),
            url: None,
            url_env: None,
            secret: None,
            secret_env: None,
        })
        .expect("provider config should be valid");

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("HTTP 500 should fail");

        assert!(err.to_string().contains("HTTP status 500"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.context.signal_id, "signal-1");
        assert_eq!(err.context.provider_id.as_deref(), Some("phone"));
        assert_eq!(err.context.provider_type.as_deref(), Some("ntfy"));
        assert_eq!(err.http_status, Some(500));
        assert_eq!(err.provider_code, None);
        assert!(err.retriable);
    }

    fn test_signal() -> Signal {
        Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            "Ready for review.",
            test_timestamp(),
            BTreeMap::new(),
        )
    }

    fn test_timestamp() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
}
