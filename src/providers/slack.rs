use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, Utc};
use serde::Serialize;

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::provider_catalog::{MessageSurface, provider_message_limit};
use crate::provider_urls::validate_slack_webhook_url;
use crate::providers::formatting::format_signal_message;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

#[derive(Debug)]
pub struct SlackProvider {
    id: String,
    url: String,
    client: reqwest::Client,
}

impl SlackProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::Slack {
            return Err(anyhow!("provider `{}` is not a slack provider", config.id));
        }

        let url = resolve_url(config)?;
        let url = validate_slack_webhook_url(&url)?;

        Ok(Self {
            id: config.id.clone(),
            url,
            client: provider_http_client()?,
        })
    }
}

impl Provider for SlackProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::Slack.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::Slack.as_str();
            let text = format_slack_text(signal);
            validate_text_size(signal, &self.id, provider_type, &text).map_err(|error| *error)?;

            let request = SlackWebhookRequest {
                text: &text,
                mrkdwn: false,
            };
            let response = self
                .client
                .post(&self.url)
                .json(&request)
                .send()
                .await
                .map_err(|error| {
                    let is_timeout = error.is_timeout();
                    provider_request_error(
                        signal,
                        &self.id,
                        provider_type,
                        "slack",
                        is_timeout,
                        error.without_url(),
                    )
                })?;

            let status = response.status();
            let status_code = status.as_u16();
            let response_body = response.text().await.map_err(|error| {
                DeliveryError::new(
                    DeliveryErrorKind::Network,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!("slack provider `{}` failed to read response", self.id),
                )
                .with_http_status(status_code)
                .with_retriable(true)
                .with_source(error.without_url())
            })?;

            if !status.is_success() {
                let mut error = DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_slack_rejection_message(&self.id, status.as_str(), &response_body),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_http_status(status_code));
                let provider_code = response_summary(&response_body);
                if !provider_code.is_empty() {
                    error = error.with_provider_code(provider_code);
                }

                return Err(error);
            }

            if response_body.trim() != "ok" {
                return Err(DeliveryError::new(
                    DeliveryErrorKind::ProviderResponse,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!(
                        "slack provider `{}` returned unexpected response{}",
                        self.id,
                        response_summary_suffix(&response_body)
                    ),
                )
                .with_http_status(status_code));
            }

            Ok(ProviderSendResult::sent(&self.id, provider_type, signal)
                .with_http_status(status_code))
        })
    }
}

#[derive(Debug, Serialize)]
struct SlackWebhookRequest<'a> {
    text: &'a str,
    mrkdwn: bool,
}

fn resolve_url(config: &ProviderConfig) -> anyhow::Result<String> {
    match (
        present(config.url.as_deref()),
        present(config.url_env.as_deref()),
    ) {
        (Some(url), None) => Ok(url.to_string()),
        (None, Some(env_name)) => std::env::var(env_name)
            .with_context(|| format!("slack provider `{}` env var is not set", config.id)),
        _ => Err(anyhow!(
            "slack provider `{}` must set exactly one of `url` or `url_env`",
            config.id
        )),
    }
}

fn validate_text_size(
    signal: &Signal,
    provider_id: &str,
    provider_type: &str,
    text: &str,
) -> Result<(), Box<DeliveryError>> {
    let limit = provider_message_limit(ProviderType::Slack, MessageSurface::TextBody);
    if text.chars().count() > limit {
        return Err(Box::new(DeliveryError::new(
            DeliveryErrorKind::Validation,
            DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
            format!("slack provider `{provider_id}` text exceeds {limit} characters"),
        )));
    }

    Ok(())
}

fn format_slack_text(signal: &Signal) -> String {
    format_signal_message(signal, &format_local_timestamp(signal.timestamp))
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

fn format_slack_rejection_message(
    provider_id: &str,
    http_status: &str,
    response_body: &str,
) -> String {
    format!(
        "slack provider `{provider_id}` returned HTTP status {http_status}{}",
        response_summary_suffix(response_body)
    )
}

fn response_summary_suffix(response_body: &str) -> String {
    let summary = response_summary(response_body);
    if summary.is_empty() {
        String::new()
    } else {
        format!(": {summary}")
    }
}

fn response_summary(response_body: &str) -> String {
    response_body
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(160)
        .collect()
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
    use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};
    use crate::provider_catalog::{MessageSurface, provider_message_limit};

    #[tokio::test]
    async fn sends_text_payload_to_slack_webhook() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/services/T00000000/B00000000/token"))
            .and(body_json(serde_json::json!({
                "text": format!(
                    "Codex\n\nReady for review.\nTime: {}",
                    format_local_timestamp(test_timestamp())
                ),
                "mrkdwn": false
            })))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let provider = test_provider(format!(
            "{}/services/T00000000/B00000000/token",
            server.uri()
        ));

        let result = provider
            .send(&test_signal())
            .await
            .expect("Slack send should succeed");

        assert_eq!(result.provider_id, "slack");
        assert_eq!(result.provider_type, "slack");
        assert_eq!(result.signal_id, "signal-1");
        assert_eq!(result.status, ProviderSendStatus::Sent);
        assert_eq!(result.http_status, Some(200));
    }

    #[tokio::test]
    async fn returns_provider_rejected_for_slack_error_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/services/T00000000/B00000000/token"))
            .respond_with(ResponseTemplate::new(403).set_body_string("action_prohibited"))
            .mount(&server)
            .await;

        let provider = test_provider(format!(
            "{}/services/T00000000/B00000000/token",
            server.uri()
        ));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("Slack error should fail");

        assert!(err.to_string().contains("action_prohibited"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.context.signal_id, "signal-1");
        assert_eq!(err.context.provider_id.as_deref(), Some("slack"));
        assert_eq!(err.context.provider_type.as_deref(), Some("slack"));
        assert_eq!(err.http_status, Some(403));
        assert_eq!(err.provider_code.as_deref(), Some("action_prohibited"));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn returns_retriable_error_for_slack_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/services/T00000000/B00000000/token"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate_limited"))
            .mount(&server)
            .await;

        let provider = test_provider(format!(
            "{}/services/T00000000/B00000000/token",
            server.uri()
        ));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("Slack rate limit should fail");

        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.http_status, Some(429));
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn rejects_text_that_exceeds_slack_limit_before_sending() {
        let server = MockServer::start().await;
        let provider = test_provider(format!(
            "{}/services/T00000000/B00000000/token",
            server.uri()
        ));
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            "a".repeat(provider_message_limit(ProviderType::Slack, MessageSurface::TextBody) + 100),
            test_timestamp(),
            BTreeMap::new(),
        );

        let err = provider
            .send(&signal)
            .await
            .expect_err("oversized Slack message should fail before network send");

        assert_eq!(err.kind, DeliveryErrorKind::Validation);
        assert!(err.to_string().contains(&format!(
            "text exceeds {}",
            provider_message_limit(ProviderType::Slack, MessageSurface::TextBody)
        )));
        assert!(
            server
                .received_requests()
                .await
                .expect("requests should be available")
                .is_empty()
        );
    }

    #[test]
    fn rejects_non_slack_provider_config() {
        let err = SlackProvider::from_config(&ProviderConfig {
            id: "slack".to_string(),
            provider_type: ProviderType::Webhook,
            base_url: None,
            server: None,
            topic: None,
            url: Some("https://example.com/slack".to_string()),
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
        })
        .expect_err("wrong provider type should fail");

        assert!(err.to_string().contains("not a slack provider"));
    }

    fn test_provider(url: String) -> SlackProvider {
        SlackProvider {
            id: "slack".to_string(),
            url,
            client: provider_http_client().expect("HTTP client should build"),
        }
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
