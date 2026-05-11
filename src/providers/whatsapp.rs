use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::providers::formatting::body_with_local_time;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

const WHATSAPP_GRAPH_API_VERSION: &str = "v23.0";
const WHATSAPP_TEXT_BODY_LIMIT: usize = 4096;

#[derive(Debug)]
pub struct WhatsappProvider {
    id: String,
    endpoint: String,
    access_token: String,
    recipient_phone_number: String,
    client: reqwest::Client,
}

impl WhatsappProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::Whatsapp {
            return Err(anyhow!(
                "provider `{}` is not a whatsapp provider",
                config.id
            ));
        }

        let access_token = resolve_secret_value(
            config,
            "access_token",
            config.access_token.as_deref(),
            "access_token_env",
            config.access_token_env.as_deref(),
        )?;
        validate_access_token(&config.id, &access_token)?;

        let phone_number_id = present(config.phone_number_id.as_deref())
            .ok_or_else(|| {
                anyhow!(
                    "whatsapp provider `{}` requires `phone_number_id`",
                    config.id
                )
            })?
            .to_string();
        validate_phone_number_id(&config.id, &phone_number_id)?;

        let recipient_phone_number = present(config.recipient_phone_number.as_deref())
            .ok_or_else(|| {
                anyhow!(
                    "whatsapp provider `{}` requires `recipient_phone_number`",
                    config.id
                )
            })?
            .to_string();
        validate_recipient_phone_number(&config.id, &recipient_phone_number)?;

        Ok(Self {
            id: config.id.clone(),
            endpoint: format!(
                "https://graph.facebook.com/{WHATSAPP_GRAPH_API_VERSION}/{phone_number_id}/messages"
            ),
            access_token,
            recipient_phone_number,
            client: provider_http_client()?,
        })
    }
}

impl Provider for WhatsappProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::Whatsapp.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::Whatsapp.as_str();
            let body = format_whatsapp_text(signal);
            validate_text_size(signal, &self.id, provider_type, &body).map_err(|error| *error)?;

            let request = WhatsappMessageRequest {
                messaging_product: "whatsapp",
                recipient_type: "individual",
                to: &self.recipient_phone_number,
                message_type: "text",
                text: WhatsappText { body: &body },
            };
            let response = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.access_token)
                .json(&request)
                .send()
                .await
                .map_err(|error| {
                    let is_timeout = error.is_timeout();
                    provider_request_error(
                        signal,
                        &self.id,
                        provider_type,
                        "whatsapp",
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
                    format!("whatsapp provider `{}` failed to read response", self.id),
                )
                .with_http_status(status_code)
                .with_retriable(true)
                .with_source(error.without_url())
            })?;

            if !status.is_success() {
                let provider_response = parse_whatsapp_error(&response_body);
                let mut error = DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_whatsapp_rejection_message(
                        &self.id,
                        status.as_str(),
                        provider_response.as_ref(),
                        &response_body,
                    ),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_http_status(status_code));
                if let Some(code) = provider_response
                    .as_ref()
                    .and_then(|response| response.error.code)
                {
                    error = error.with_provider_code(code.to_string());
                }

                return Err(error);
            }

            let provider_response: WhatsappMessageResponse = serde_json::from_str(&response_body)
                .map_err(|error| {
                DeliveryError::new(
                    DeliveryErrorKind::ProviderResponse,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!(
                        "whatsapp provider `{}` returned invalid response JSON",
                        self.id
                    ),
                )
                .with_http_status(status_code)
                .with_source(error)
            })?;

            let mut result = ProviderSendResult::sent(&self.id, provider_type, signal)
                .with_http_status(status_code);
            if let Some(message_id) = provider_response
                .messages
                .and_then(|messages| messages.into_iter().next())
                .map(|message| message.id)
            {
                result = result.with_provider_message_id(message_id);
            }

            Ok(result)
        })
    }
}

#[derive(Debug, Serialize)]
struct WhatsappMessageRequest<'a> {
    messaging_product: &'static str,
    recipient_type: &'static str,
    to: &'a str,
    #[serde(rename = "type")]
    message_type: &'static str,
    text: WhatsappText<'a>,
}

#[derive(Debug, Serialize)]
struct WhatsappText<'a> {
    body: &'a str,
}

#[derive(Debug, Deserialize)]
struct WhatsappMessageResponse {
    messages: Option<Vec<WhatsappSentMessage>>,
}

#[derive(Debug, Deserialize)]
struct WhatsappSentMessage {
    id: String,
}

#[derive(Debug, Deserialize)]
struct WhatsappErrorResponse {
    error: WhatsappError,
}

#[derive(Debug, Deserialize)]
struct WhatsappError {
    message: String,
    code: Option<i64>,
    error_subcode: Option<i64>,
    #[serde(rename = "type")]
    error_type: Option<String>,
}

fn resolve_secret_value(
    config: &ProviderConfig,
    value_field: &'static str,
    value: Option<&str>,
    env_field: &'static str,
    env_name: Option<&str>,
) -> anyhow::Result<String> {
    match (present(value), present(env_name)) {
        (Some(value), None) => Ok(value.to_string()),
        (None, Some(env_name)) => std::env::var(env_name).with_context(|| {
            format!(
                "whatsapp provider `{}` env var `{}` from `{}` is not set",
                config.id, env_name, env_field
            )
        }),
        _ => Err(anyhow!(
            "whatsapp provider `{}` must set exactly one of `{}` or `{}`",
            config.id,
            value_field,
            env_field
        )),
    }
}

fn validate_access_token(provider_id: &str, access_token: &str) -> anyhow::Result<()> {
    if !access_token.is_empty() && access_token.bytes().all(|byte| !byte.is_ascii_whitespace()) {
        Ok(())
    } else {
        Err(anyhow!(
            "whatsapp provider `{provider_id}` `access_token` must be non-empty and must not contain whitespace"
        ))
    }
}

fn validate_phone_number_id(provider_id: &str, phone_number_id: &str) -> anyhow::Result<()> {
    if !phone_number_id.is_empty() && phone_number_id.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(())
    } else {
        Err(anyhow!(
            "whatsapp provider `{provider_id}` `phone_number_id` must contain only digits"
        ))
    }
}

fn validate_recipient_phone_number(
    provider_id: &str,
    recipient_phone_number: &str,
) -> anyhow::Result<()> {
    let len = recipient_phone_number.len();
    if (7..=15).contains(&len)
        && recipient_phone_number
            .bytes()
            .all(|byte| byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err(anyhow!(
            "whatsapp provider `{provider_id}` `recipient_phone_number` must be 7 to 15 digits without spaces or punctuation"
        ))
    }
}

fn validate_text_size(
    signal: &Signal,
    provider_id: &str,
    provider_type: &str,
    text: &str,
) -> Result<(), Box<DeliveryError>> {
    if text.chars().count() > WHATSAPP_TEXT_BODY_LIMIT {
        return Err(Box::new(DeliveryError::new(
            DeliveryErrorKind::Validation,
            DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
            format!("whatsapp provider `{provider_id}` text body exceeds 4096 characters"),
        )));
    }

    Ok(())
}

fn format_whatsapp_text(signal: &Signal) -> String {
    let body = body_with_local_time(&signal.body, &format_local_timestamp(signal.timestamp));
    format!("{}\n\n{}", signal.title, body)
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

fn parse_whatsapp_error(body: &str) -> Option<WhatsappErrorResponse> {
    serde_json::from_str(body).ok()
}

fn format_whatsapp_rejection_message(
    provider_id: &str,
    http_status: &str,
    response: Option<&WhatsappErrorResponse>,
    response_body: &str,
) -> String {
    if let Some(response) = response {
        let mut message = format!(
            "whatsapp provider `{provider_id}` returned HTTP status {http_status}: {}",
            response.error.message
        );
        if let Some(error_type) = response.error.error_type.as_deref() {
            message.push_str(&format!(" ({error_type})"));
        }
        if let Some(error_subcode) = response.error.error_subcode {
            message.push_str(&format!(" subcode {error_subcode}"));
        }
        return message;
    }

    format!(
        "whatsapp provider `{provider_id}` returned HTTP status {http_status}{}",
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
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};

    #[tokio::test]
    async fn sends_text_message_to_whatsapp_cloud_api() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v23.0/123456789/messages"))
            .and(header("authorization", "Bearer test-access-token"))
            .and(body_json(serde_json::json!({
                "messaging_product": "whatsapp",
                "recipient_type": "individual",
                "to": "15551234567",
                "type": "text",
                "text": {
                    "body": format!(
                        "Codex\n\nReady for review.\nTime: {}",
                        format_local_timestamp(test_timestamp())
                    )
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messaging_product": "whatsapp",
                "contacts": [{ "input": "15551234567", "wa_id": "15551234567" }],
                "messages": [{ "id": "wamid.test" }]
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/v23.0/123456789/messages", server.uri()));

        let result = provider
            .send(&test_signal())
            .await
            .expect("WhatsApp send should succeed");

        assert_eq!(result.provider_id, "whatsapp");
        assert_eq!(result.provider_type, "whatsapp");
        assert_eq!(result.signal_id, "signal-1");
        assert_eq!(result.status, ProviderSendStatus::Sent);
        assert_eq!(result.http_status, Some(200));
        assert_eq!(result.provider_message_id.as_deref(), Some("wamid.test"));
    }

    #[tokio::test]
    async fn returns_provider_rejected_for_whatsapp_error_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v23.0/123456789/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": {
                    "message": "Unsupported post request",
                    "type": "GraphMethodException",
                    "code": 100,
                    "error_subcode": 33
                }
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/v23.0/123456789/messages", server.uri()));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("WhatsApp error response should fail");

        assert!(err.to_string().contains("Unsupported post request"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.http_status, Some(400));
        assert_eq!(err.provider_code.as_deref(), Some("100"));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn rejects_whatsapp_text_that_exceeds_limit_before_sending() {
        let server = MockServer::start().await;
        let provider = test_provider(format!("{}/v23.0/123456789/messages", server.uri()));
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            &"a".repeat(4100),
            test_timestamp(),
            BTreeMap::new(),
        );

        let err = provider
            .send(&signal)
            .await
            .expect_err("oversized WhatsApp message should fail before network send");

        assert_eq!(err.kind, DeliveryErrorKind::Validation);
        assert!(err.to_string().contains("text body exceeds 4096"));
        assert!(
            server
                .received_requests()
                .await
                .expect("requests should be available")
                .is_empty()
        );
    }

    #[test]
    fn validates_whatsapp_config_values() {
        let provider = WhatsappProvider::from_config(&ProviderConfig {
            id: "whatsapp".to_string(),
            provider_type: ProviderType::Whatsapp,
            server: None,
            topic: None,
            url: None,
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
            access_token: Some("test-access-token".to_string()),
            access_token_env: None,
            phone_number_id: Some("123456789".to_string()),
            recipient_phone_number: Some("15551234567".to_string()),
        })
        .expect("WhatsApp provider config should be valid");

        assert_eq!(provider.id, "whatsapp");
        assert_eq!(provider.recipient_phone_number, "15551234567");
    }

    fn test_provider(endpoint: String) -> WhatsappProvider {
        WhatsappProvider {
            id: "whatsapp".to_string(),
            endpoint,
            access_token: "test-access-token".to_string(),
            recipient_phone_number: "15551234567".to_string(),
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
