use anyhow::{Context, anyhow};

use crate::config::{ProviderConfig, ProviderType};
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
            client: reqwest::Client::new(),
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
            let response = self
                .client
                .post(&self.url)
                .json(signal)
                .send()
                .await
                .map_err(reqwest::Error::without_url)
                .with_context(|| format!("webhook provider `{}` request failed", self.id))?;

            let status = response.status();
            if !status.is_success() {
                return Err(anyhow!(
                    "webhook provider `{}` returned HTTP status {}",
                    self.id,
                    status
                ));
            }

            Ok(())
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
        })
        .expect("provider config should be valid");

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("HTTP 500 should fail");

        assert!(err.to_string().contains("HTTP status 500"));
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
            "Codex finished a job.",
            timestamp,
            BTreeMap::new(),
        )
    }
}
