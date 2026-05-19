use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, Utc};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::provider_catalog::{MessageSurface, provider_message_limit};
use crate::provider_urls::validate_discord_webhook_url;
use crate::providers::formatting::format_signal_message;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

#[derive(Debug)]
pub struct DiscordProvider {
    id: String,
    endpoint: String,
    client: reqwest::Client,
}

impl DiscordProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::Discord {
            return Err(anyhow!(
                "provider `{}` is not a discord provider",
                config.id
            ));
        }

        let url = resolve_url(config)?;
        let url = validate_discord_webhook_url(&url)?;

        Ok(Self {
            id: config.id.clone(),
            endpoint: endpoint_with_wait(&url)?,
            client: provider_http_client()?,
        })
    }
}

impl Provider for DiscordProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::Discord.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::Discord.as_str();
            let content = format_discord_content(signal);
            validate_content_size(signal, &self.id, provider_type, &content)
                .map_err(|error| *error)?;

            let request = DiscordWebhookRequest {
                content: &content,
                allowed_mentions: DiscordAllowedMentions { parse: Vec::new() },
            };
            let response = self
                .client
                .post(&self.endpoint)
                .json(&request)
                .send()
                .await
                .map_err(|error| {
                    let is_timeout = error.is_timeout();
                    provider_request_error(
                        signal,
                        &self.id,
                        provider_type,
                        "discord",
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
                    format!("discord provider `{}` failed to read response", self.id),
                )
                .with_http_status(status_code)
                .with_retriable(true)
                .with_source(error.without_url())
            })?;

            if !status.is_success() {
                let provider_response = parse_discord_error(&response_body);
                let mut error = DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_discord_rejection_message(
                        &self.id,
                        status.as_str(),
                        provider_response.as_ref(),
                        &response_body,
                    ),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_http_status(status_code));
                if let Some(code) = provider_response.and_then(|response| response.code) {
                    error = error.with_provider_code(code.to_string());
                }

                return Err(error);
            }

            let provider_response: DiscordMessageResponse = serde_json::from_str(&response_body)
                .map_err(|error| {
                    DeliveryError::new(
                        DeliveryErrorKind::ProviderResponse,
                        DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                        format!(
                            "discord provider `{}` returned invalid response JSON",
                            self.id
                        ),
                    )
                    .with_http_status(status_code)
                    .with_source(error)
                })?;

            Ok(ProviderSendResult::sent(&self.id, provider_type, signal)
                .with_http_status(status_code)
                .with_provider_message_id(provider_response.id))
        })
    }
}

#[derive(Debug, Serialize)]
struct DiscordWebhookRequest<'a> {
    content: &'a str,
    allowed_mentions: DiscordAllowedMentions,
}

#[derive(Debug, Serialize)]
struct DiscordAllowedMentions {
    parse: Vec<&'static str>,
}

#[derive(Debug, Deserialize)]
struct DiscordMessageResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct DiscordErrorResponse {
    message: String,
    code: Option<i64>,
}

fn resolve_url(config: &ProviderConfig) -> anyhow::Result<String> {
    match (
        present(config.url.as_deref()),
        present(config.url_env.as_deref()),
    ) {
        (Some(url), None) => Ok(url.to_string()),
        (None, Some(env_name)) => std::env::var(env_name)
            .with_context(|| format!("discord provider `{}` env var is not set", config.id)),
        _ => Err(anyhow!(
            "discord provider `{}` must set exactly one of `url` or `url_env`",
            config.id
        )),
    }
}

fn endpoint_with_wait(url: &str) -> anyhow::Result<String> {
    let mut parsed = Url::parse(url).context("Discord webhook URL must be a valid URL")?;
    parsed.query_pairs_mut().append_pair("wait", "true");
    Ok(parsed.to_string())
}

fn validate_content_size(
    signal: &Signal,
    provider_id: &str,
    provider_type: &str,
    content: &str,
) -> Result<(), Box<DeliveryError>> {
    let limit = provider_message_limit(ProviderType::Discord, MessageSurface::WebhookContent);
    if content.chars().count() > limit {
        return Err(Box::new(DeliveryError::new(
            DeliveryErrorKind::Validation,
            DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
            format!("discord provider `{provider_id}` content exceeds {limit} characters"),
        )));
    }

    Ok(())
}

fn format_discord_content(signal: &Signal) -> String {
    format_signal_message(signal, &format_local_timestamp(signal.timestamp))
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

fn parse_discord_error(body: &str) -> Option<DiscordErrorResponse> {
    serde_json::from_str(body).ok()
}

fn format_discord_rejection_message(
    provider_id: &str,
    http_status: &str,
    response: Option<&DiscordErrorResponse>,
    response_body: &str,
) -> String {
    if let Some(response) = response {
        return format!(
            "discord provider `{provider_id}` returned HTTP status {http_status}: {}",
            response.message
        );
    }

    format!(
        "discord provider `{provider_id}` returned HTTP status {http_status}{}",
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
    use wiremock::matchers::{body_json, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};
    use crate::provider_catalog::{MessageSurface, provider_message_limit};

    #[tokio::test]
    async fn sends_content_payload_to_discord_webhook() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/webhooks/123456789012345678/token"))
            .and(query_param("wait", "true"))
            .and(body_json(serde_json::json!({
                "content": format!(
                    "Codex\n\nReady for review.\nTime: {}",
                    format_local_timestamp(test_timestamp())
                ),
                "allowed_mentions": {
                    "parse": []
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "987654321098765432"
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!(
            "{}/api/webhooks/123456789012345678/token?wait=true",
            server.uri()
        ));

        let result = provider
            .send(&test_signal())
            .await
            .expect("Discord send should succeed");

        assert_eq!(result.provider_id, "discord");
        assert_eq!(result.provider_type, "discord");
        assert_eq!(result.signal_id, "signal-1");
        assert_eq!(result.status, ProviderSendStatus::Sent);
        assert_eq!(result.http_status, Some(200));
        assert_eq!(
            result.provider_message_id.as_deref(),
            Some("987654321098765432")
        );
    }

    #[tokio::test]
    async fn returns_provider_rejected_for_discord_error_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/webhooks/123456789012345678/token"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "message": "Unknown Webhook",
                "code": 10015
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!(
            "{}/api/webhooks/123456789012345678/token",
            server.uri()
        ));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("Discord error should fail");

        assert!(err.to_string().contains("Unknown Webhook"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.context.signal_id, "signal-1");
        assert_eq!(err.context.provider_id.as_deref(), Some("discord"));
        assert_eq!(err.context.provider_type.as_deref(), Some("discord"));
        assert_eq!(err.http_status, Some(404));
        assert_eq!(err.provider_code.as_deref(), Some("10015"));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn returns_retriable_error_for_discord_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/webhooks/123456789012345678/token"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "message": "You are being rate limited.",
                "retry_after": 1.0,
                "global": false
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!(
            "{}/api/webhooks/123456789012345678/token",
            server.uri()
        ));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("Discord rate limit should fail");

        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.http_status, Some(429));
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn rejects_content_that_exceeds_discord_limit_before_sending() {
        let server = MockServer::start().await;
        let provider = test_provider(format!(
            "{}/api/webhooks/123456789012345678/token",
            server.uri()
        ));
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            "a".repeat(
                provider_message_limit(ProviderType::Discord, MessageSurface::WebhookContent) + 100,
            ),
            test_timestamp(),
            BTreeMap::new(),
        );

        let err = provider
            .send(&signal)
            .await
            .expect_err("oversized Discord message should fail before network send");

        assert_eq!(err.kind, DeliveryErrorKind::Validation);
        assert!(err.to_string().contains(&format!(
            "content exceeds {}",
            provider_message_limit(ProviderType::Discord, MessageSurface::WebhookContent)
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
    fn appends_wait_true_to_official_webhook_url() {
        assert_eq!(
            endpoint_with_wait("https://discord.com/api/webhooks/123456789012345678/token")
                .expect("endpoint should be valid"),
            "https://discord.com/api/webhooks/123456789012345678/token?wait=true"
        );
    }

    fn test_provider(endpoint: String) -> DiscordProvider {
        DiscordProvider {
            id: "discord".to_string(),
            endpoint,
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
