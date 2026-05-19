use anyhow::{Context, anyhow};

use crate::config::{ProviderConfig, ProviderConfigDetail, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::provider_urls::validate_custom_webhook_url;
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
        let ProviderConfigDetail::Webhook(detail) = &config.detail else {
            return Err(anyhow!(
                "provider `{}` is not a webhook provider",
                config.id
            ));
        };

        let url = detail.url.resolve_runtime_value(
            &config.id,
            ProviderType::Webhook.as_str(),
            "url_env",
        )?;
        let url = validate_custom_webhook_url(&url)
            .with_context(|| format!("webhook provider `{}` URL is invalid", config.id))?;

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::config::{UrlSource, WebhookProviderConfig};
    use crate::delivery::DeliveryErrorKind;
    use crate::providers::contract_test::ProviderHttpSimulator;
    use chrono::{DateTime, Utc};

    #[tokio::test]
    async fn posts_complete_signal_json() {
        let simulator = ProviderHttpSimulator::start().await;
        let signal = test_signal();
        simulator.expect_post_json("/hook", &signal, 200).await;

        let provider = WebhookProvider::from_config(&webhook_config(simulator.url("/hook")))
            .expect("provider config should be valid");

        provider
            .send(&signal)
            .await
            .expect("webhook send should succeed");
    }

    #[test]
    fn rejects_invalid_url_from_env() {
        let _guard = EnvVarGuard::set("AGENTS_ROUTER_TEST_WEBHOOK_URL", "http://example.com/hook");
        let config = webhook_env_config("AGENTS_ROUTER_TEST_WEBHOOK_URL");

        let err = WebhookProvider::from_config(&config)
            .expect_err("invalid webhook URL env value should fail");

        assert!(err.to_string().contains("URL is invalid"));
    }

    #[tokio::test]
    async fn returns_error_for_non_success_status() {
        let simulator = ProviderHttpSimulator::start().await;
        simulator.expect_post_status("/hook", 500).await;

        let provider = WebhookProvider::from_config(&webhook_config(simulator.url("/hook")))
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

    fn webhook_config(url: String) -> ProviderConfig {
        ProviderConfig {
            id: "debug".to_string(),
            detail: ProviderConfigDetail::Webhook(WebhookProviderConfig {
                url: UrlSource::Inline(url),
            }),
        }
    }

    fn webhook_env_config(env_name: &'static str) -> ProviderConfig {
        ProviderConfig {
            id: "debug".to_string(),
            detail: ProviderConfigDetail::Webhook(WebhookProviderConfig {
                url: UrlSource::Env(env_name.to_string()),
            }),
        }
    }

    struct EnvVarGuard {
        name: &'static str,
    }

    impl EnvVarGuard {
        fn set(name: &'static str, value: &str) -> Self {
            // SAFETY: this test uses a unique env var name scoped to this module.
            unsafe { std::env::set_var(name, value) };
            Self { name }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            // SAFETY: this removes only the unique env var set by the test guard.
            unsafe { std::env::remove_var(self.name) };
        }
    }
}
