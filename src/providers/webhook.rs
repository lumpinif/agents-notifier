use anyhow::{Context, anyhow};

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

#[derive(Debug)]
pub struct WebhookProvider {
    id: String,
    url: String,
    client: reqwest::Client,
}

impl WebhookProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::Webhook {
            return Err(anyhow!(
                "provider `{}` is not a webhook provider",
                config.id
            ));
        }

        let url = match (
            present(config.url.as_deref()),
            present(config.url_env.as_deref()),
        ) {
            (Some(url), None) => url.to_string(),
            (None, Some(env_name)) => std::env::var(env_name)
                .with_context(|| format!("webhook provider `{}` env var is not set", config.id))?,
            _ => {
                return Err(anyhow!(
                    "webhook provider `{}` must set exactly one of `url` or `url_env`",
                    config.id
                ));
            }
        };

        if url.trim().is_empty() {
            return Err(anyhow!("webhook provider `{}` URL is empty", config.id));
        }

        Ok(Self {
            id: config.id.clone(),
            url,
            client: provider_http_client()?,
        })
    }
}

impl Provider for WebhookProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::Webhook.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::Webhook.as_str();
            let response = self
                .client
                .post(&self.url)
                .json(signal)
                .send()
                .await
                .map_err(|error| {
                    let is_timeout = error.is_timeout();
                    provider_request_error(
                        signal,
                        &self.id,
                        provider_type,
                        "webhook",
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
                        "webhook provider `{}` returned HTTP status {}",
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

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::config::ProviderType;
    use crate::delivery::DeliveryErrorKind;

    #[tokio::test]
    async fn posts_complete_signal_json() {
        let server = MockServer::start().await;
        let signal = test_signal();
        Mock::given(method("POST"))
            .and(path("/hook"))
            .and(body_json(&signal))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let provider = WebhookProvider::from_config(&ProviderConfig {
            id: "debug".to_string(),
            provider_type: ProviderType::Webhook,
            server: None,
            topic: None,
            url: Some(format!("{}/hook", server.uri())),
            url_env: None,
            secret: None,
            secret_env: None,
            app_token: None,
            app_token_env: None,
            user_key: None,
            user_key_env: None,
            device: None,
            sound: None,
        })
        .expect("provider config should be valid");

        provider
            .send(&signal)
            .await
            .expect("webhook send should succeed");
    }

    #[test]
    fn rejects_ambiguous_url_sources() {
        let err = WebhookProvider::from_config(&ProviderConfig {
            id: "debug".to_string(),
            provider_type: ProviderType::Webhook,
            server: None,
            topic: None,
            url: Some("https://example.com/hook".to_string()),
            url_env: Some("AGENTS_NOTIFIER_WEBHOOK_URL".to_string()),
            secret: None,
            secret_env: None,
            app_token: None,
            app_token_env: None,
            user_key: None,
            user_key_env: None,
            device: None,
            sound: None,
        })
        .expect_err("ambiguous URL config should fail");

        assert!(err.to_string().contains("exactly one"));
    }

    #[tokio::test]
    async fn returns_error_for_non_success_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let provider = WebhookProvider::from_config(&ProviderConfig {
            id: "debug".to_string(),
            provider_type: ProviderType::Webhook,
            server: None,
            topic: None,
            url: Some(format!("{}/hook", server.uri())),
            url_env: None,
            secret: None,
            secret_env: None,
            app_token: None,
            app_token_env: None,
            user_key: None,
            user_key_env: None,
            device: None,
            sound: None,
        })
        .expect("provider config should be valid");

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("HTTP 500 should fail");

        assert!(err.to_string().contains("HTTP status 500"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.context.signal_id, "signal-1");
        assert_eq!(err.context.provider_id.as_deref(), Some("debug"));
        assert_eq!(err.context.provider_type.as_deref(), Some("webhook"));
        assert_eq!(err.http_status, Some(500));
        assert_eq!(err.provider_code, None);
        assert!(err.retriable);
    }

    fn test_signal() -> Signal {
        let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            "Ready for review.",
            timestamp,
            BTreeMap::new(),
        )
    }
}
