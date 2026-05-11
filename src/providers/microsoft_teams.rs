use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, Utc};
use reqwest::header::CONTENT_TYPE;
use serde::Serialize;

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::provider_urls::validate_microsoft_teams_webhook_url;
use crate::providers::formatting::body_with_local_time;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

const MICROSOFT_TEAMS_MESSAGE_LIMIT_BYTES: usize = 28 * 1024;

#[derive(Debug)]
pub struct MicrosoftTeamsProvider {
    id: String,
    url: String,
    client: reqwest::Client,
}

impl MicrosoftTeamsProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::MicrosoftTeams {
            return Err(anyhow!(
                "provider `{}` is not a microsoft_teams provider",
                config.id
            ));
        }

        let url = resolve_url(config)?;
        let url = validate_microsoft_teams_webhook_url(&url)?;

        Ok(Self {
            id: config.id.clone(),
            url,
            client: provider_http_client()?,
        })
    }
}

impl Provider for MicrosoftTeamsProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::MicrosoftTeams.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::MicrosoftTeams.as_str();
            let title = signal.title.as_str();
            let body = format_microsoft_teams_body(signal);
            let request = MicrosoftTeamsWebhookRequest::new(title, &body);
            let request_body = serde_json::to_vec(&request).map_err(|error| {
                DeliveryError::new(
                    DeliveryErrorKind::Internal,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!(
                        "microsoft_teams provider `{}` failed to serialize request",
                        self.id
                    ),
                )
                .with_source(error)
            })?;
            validate_request_size(signal, &self.id, provider_type, request_body.len())
                .map_err(|error| *error)?;

            let response = self
                .client
                .post(&self.url)
                .header(CONTENT_TYPE, "application/json")
                .body(request_body)
                .send()
                .await
                .map_err(|error| {
                    let is_timeout = error.is_timeout();
                    provider_request_error(
                        signal,
                        &self.id,
                        provider_type,
                        "microsoft_teams",
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
                    format!(
                        "microsoft_teams provider `{}` failed to read response",
                        self.id
                    ),
                )
                .with_http_status(status_code)
                .with_retriable(true)
                .with_source(error.without_url())
            })?;

            if !status.is_success() {
                let mut error = DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_microsoft_teams_rejection_message(
                        &self.id,
                        status.as_str(),
                        &response_body,
                    ),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_http_status(status_code));
                let provider_code = response_summary(&response_body);
                if !provider_code.is_empty() {
                    error = error.with_provider_code(provider_code);
                }

                return Err(error);
            }

            Ok(ProviderSendResult::sent(&self.id, provider_type, signal)
                .with_http_status(status_code))
        })
    }
}

#[derive(Debug, Serialize)]
struct MicrosoftTeamsWebhookRequest<'a> {
    #[serde(rename = "type")]
    message_type: &'static str,
    attachments: Vec<MicrosoftTeamsAttachment<'a>>,
}

impl<'a> MicrosoftTeamsWebhookRequest<'a> {
    fn new(title: &'a str, body: &'a str) -> Self {
        Self {
            message_type: "message",
            attachments: vec![MicrosoftTeamsAttachment {
                content_type: "application/vnd.microsoft.card.adaptive",
                content_url: None,
                content: MicrosoftTeamsAdaptiveCard {
                    schema: "http://adaptivecards.io/schemas/adaptive-card.json",
                    card_type: "AdaptiveCard",
                    version: "1.0",
                    body: vec![
                        MicrosoftTeamsTextBlock {
                            block_type: "TextBlock",
                            text: title,
                            weight: Some("Bolder"),
                            wrap: true,
                        },
                        MicrosoftTeamsTextBlock {
                            block_type: "TextBlock",
                            text: body,
                            weight: None,
                            wrap: true,
                        },
                    ],
                },
            }],
        }
    }
}

#[derive(Debug, Serialize)]
struct MicrosoftTeamsAttachment<'a> {
    #[serde(rename = "contentType")]
    content_type: &'static str,
    #[serde(rename = "contentUrl")]
    content_url: Option<&'static str>,
    content: MicrosoftTeamsAdaptiveCard<'a>,
}

#[derive(Debug, Serialize)]
struct MicrosoftTeamsAdaptiveCard<'a> {
    #[serde(rename = "$schema")]
    schema: &'static str,
    #[serde(rename = "type")]
    card_type: &'static str,
    version: &'static str,
    body: Vec<MicrosoftTeamsTextBlock<'a>>,
}

#[derive(Debug, Serialize)]
struct MicrosoftTeamsTextBlock<'a> {
    #[serde(rename = "type")]
    block_type: &'static str,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    weight: Option<&'static str>,
    wrap: bool,
}

fn resolve_url(config: &ProviderConfig) -> anyhow::Result<String> {
    match (
        present(config.url.as_deref()),
        present(config.url_env.as_deref()),
    ) {
        (Some(url), None) => Ok(url.to_string()),
        (None, Some(env_name)) => std::env::var(env_name).with_context(|| {
            format!(
                "microsoft_teams provider `{}` env var is not set",
                config.id
            )
        }),
        _ => Err(anyhow!(
            "microsoft_teams provider `{}` must set exactly one of `url` or `url_env`",
            config.id
        )),
    }
}

fn validate_request_size(
    signal: &Signal,
    provider_id: &str,
    provider_type: &str,
    request_size: usize,
) -> Result<(), Box<DeliveryError>> {
    if request_size > MICROSOFT_TEAMS_MESSAGE_LIMIT_BYTES {
        return Err(Box::new(DeliveryError::new(
            DeliveryErrorKind::Validation,
            DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
            format!("microsoft_teams provider `{provider_id}` message exceeds 28 KB"),
        )));
    }

    Ok(())
}

fn format_microsoft_teams_body(signal: &Signal) -> String {
    body_with_local_time(&signal.body, &format_local_timestamp(signal.timestamp))
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

fn format_microsoft_teams_rejection_message(
    provider_id: &str,
    http_status: &str,
    response_body: &str,
) -> String {
    format!(
        "microsoft_teams provider `{provider_id}` returned HTTP status {http_status}{}",
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

    #[tokio::test]
    async fn sends_adaptive_card_to_microsoft_teams_webhook() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/workflow"))
            .and(body_json(serde_json::json!({
                "type": "message",
                "attachments": [
                    {
                        "contentType": "application/vnd.microsoft.card.adaptive",
                        "contentUrl": null,
                        "content": {
                            "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                            "type": "AdaptiveCard",
                            "version": "1.0",
                            "body": [
                                {
                                    "type": "TextBlock",
                                    "text": "Codex",
                                    "weight": "Bolder",
                                    "wrap": true
                                },
                                {
                                    "type": "TextBlock",
                                    "text": format!(
                                        "Ready for review.\nTime: {}",
                                        format_local_timestamp(test_timestamp())
                                    ),
                                    "wrap": true
                                }
                            ]
                        }
                    }
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_string("1"))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/workflow", server.uri()));

        let result = provider
            .send(&test_signal())
            .await
            .expect("Microsoft Teams send should succeed");

        assert_eq!(result.provider_id, "microsoft_teams");
        assert_eq!(result.provider_type, "microsoft_teams");
        assert_eq!(result.signal_id, "signal-1");
        assert_eq!(result.status, ProviderSendStatus::Sent);
        assert_eq!(result.http_status, Some(200));
    }

    #[tokio::test]
    async fn returns_provider_rejected_for_microsoft_teams_error_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/workflow"))
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/workflow", server.uri()));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("Microsoft Teams error response should fail");

        assert!(err.to_string().contains("rate limited"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.http_status, Some(429));
        assert_eq!(err.provider_code.as_deref(), Some("rate limited"));
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn rejects_microsoft_teams_payload_that_exceeds_limit_before_sending() {
        let server = MockServer::start().await;
        let provider = test_provider(format!("{}/workflow", server.uri()));
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            "a".repeat(MICROSOFT_TEAMS_MESSAGE_LIMIT_BYTES),
            test_timestamp(),
            BTreeMap::new(),
        );

        let err = provider
            .send(&signal)
            .await
            .expect_err("oversized Microsoft Teams message should fail before network send");

        assert_eq!(err.kind, DeliveryErrorKind::Validation);
        assert!(err.to_string().contains("message exceeds 28 KB"));
        assert!(
            server
                .received_requests()
                .await
                .expect("requests should be available")
                .is_empty()
        );
    }

    fn test_provider(url: String) -> MicrosoftTeamsProvider {
        MicrosoftTeamsProvider {
            id: "microsoft_teams".to_string(),
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
