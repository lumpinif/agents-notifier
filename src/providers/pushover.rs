use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    provider_request_error,
};
use crate::providers::formatting::body_with_local_time;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

const PUSHOVER_MESSAGES_ENDPOINT: &str = "https://api.pushover.net/1/messages.json";
const PUSHOVER_TITLE_LIMIT: usize = 250;
const PUSHOVER_MESSAGE_LIMIT: usize = 1024;

#[derive(Debug)]
pub struct PushoverProvider {
    id: String,
    endpoint: String,
    app_token: String,
    user_key: String,
    device: Option<String>,
    sound: Option<String>,
    client: reqwest::Client,
}

impl PushoverProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::Pushover {
            return Err(anyhow!(
                "provider `{}` is not a pushover provider",
                config.id
            ));
        }

        let app_token = resolve_secret_value(
            config,
            "app_token",
            config.app_token.as_deref(),
            "app_token_env",
            config.app_token_env.as_deref(),
        )?;
        let user_key = resolve_secret_value(
            config,
            "user_key",
            config.user_key.as_deref(),
            "user_key_env",
            config.user_key_env.as_deref(),
        )?;
        validate_pushover_identifier(&config.id, "app_token", &app_token)?;
        validate_pushover_identifier(&config.id, "user_key", &user_key)?;

        let device = present(config.device.as_deref()).map(ToOwned::to_owned);
        if let Some(device) = &device {
            validate_device(&config.id, device)?;
        }

        Ok(Self {
            id: config.id.clone(),
            endpoint: PUSHOVER_MESSAGES_ENDPOINT.to_string(),
            app_token,
            user_key,
            device,
            sound: present(config.sound.as_deref()).map(ToOwned::to_owned),
            client: provider_http_client()?,
        })
    }
}

impl Provider for PushoverProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::Pushover.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::Pushover.as_str();
            let message = format_pushover_body(signal);
            validate_signal_size(signal, &self.id, provider_type, &message)
                .map_err(|error| *error)?;

            let request = PushoverMessageRequest {
                token: &self.app_token,
                user: &self.user_key,
                title: &signal.title,
                message: &message,
                device: self.device.as_deref(),
                sound: self.sound.as_deref(),
            };

            let response = self
                .client
                .post(&self.endpoint)
                .form(&request)
                .send()
                .await
                .map_err(|error| {
                    let is_timeout = error.is_timeout();
                    provider_request_error(
                        signal,
                        &self.id,
                        provider_type,
                        "pushover",
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
                    format!("pushover provider `{}` failed to read response", self.id),
                )
                .with_http_status(status_code)
                .with_retriable(true)
                .with_source(error.without_url())
            })?;

            if !status.is_success() {
                let provider_response = parse_pushover_response(&response_body).ok();
                return Err(DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_pushover_rejection_message(
                        &self.id,
                        Some(status.as_str()),
                        provider_response.as_ref(),
                    ),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_pushover_http_status(status_code)));
            }

            let provider_response = parse_pushover_response(&response_body).map_err(|error| {
                DeliveryError::new(
                    DeliveryErrorKind::ProviderResponse,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!(
                        "pushover provider `{}` returned invalid response JSON",
                        self.id
                    ),
                )
                .with_http_status(status_code)
                .with_source(error)
            })?;

            if provider_response.status != 1 {
                return Err(DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_pushover_rejection_message(&self.id, None, Some(&provider_response)),
                )
                .with_http_status(status_code)
                .with_provider_code(provider_response.status.to_string()));
            }

            let mut result = ProviderSendResult::sent(&self.id, provider_type, signal)
                .with_http_status(status_code);
            if let Some(receipt) = provider_response.receipt {
                result = result.with_provider_message_id(receipt);
            }

            Ok(result)
        })
    }
}

#[derive(Debug, Serialize)]
struct PushoverMessageRequest<'a> {
    token: &'a str,
    user: &'a str,
    title: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sound: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct PushoverResponse {
    status: i64,
    receipt: Option<String>,
    errors: Option<Vec<String>>,
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
                "pushover provider `{}` env var `{}` from `{}` is not set",
                config.id, env_name, env_field
            )
        }),
        _ => Err(anyhow!(
            "pushover provider `{}` must set exactly one of `{}` or `{}`",
            config.id,
            value_field,
            env_field
        )),
    }
}

fn validate_pushover_identifier(
    provider_id: &str,
    field: &'static str,
    value: &str,
) -> anyhow::Result<()> {
    if value.len() == 30 && value.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(anyhow!(
            "pushover provider `{provider_id}` `{field}` must be 30 letters or numbers"
        ))
    }
}

fn validate_device(provider_id: &str, device: &str) -> anyhow::Result<()> {
    let valid = device.split(',').all(|name| {
        !name.is_empty()
            && name.len() <= 25
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    });
    if valid {
        Ok(())
    } else {
        Err(anyhow!(
            "pushover provider `{provider_id}` `device` must contain comma-separated device names of 25 characters or fewer"
        ))
    }
}

fn validate_signal_size(
    signal: &Signal,
    provider_id: &str,
    provider_type: &str,
    message: &str,
) -> Result<(), Box<DeliveryError>> {
    if signal.title.chars().count() > PUSHOVER_TITLE_LIMIT {
        return Err(Box::new(DeliveryError::new(
            DeliveryErrorKind::Validation,
            DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
            format!("pushover provider `{provider_id}` title exceeds 250 characters"),
        )));
    }

    if message.chars().count() > PUSHOVER_MESSAGE_LIMIT {
        return Err(Box::new(DeliveryError::new(
            DeliveryErrorKind::Validation,
            DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
            format!("pushover provider `{provider_id}` message exceeds 1024 characters"),
        )));
    }

    Ok(())
}

fn format_pushover_body(signal: &Signal) -> String {
    body_with_local_time(&signal.body, &format_local_timestamp(signal.timestamp))
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

fn parse_pushover_response(body: &str) -> Result<PushoverResponse, serde_json::Error> {
    serde_json::from_str(body)
}

fn format_pushover_rejection_message(
    provider_id: &str,
    http_status: Option<&str>,
    response: Option<&PushoverResponse>,
) -> String {
    let mut message = match http_status {
        Some(http_status) => {
            format!("pushover provider `{provider_id}` returned HTTP status {http_status}")
        }
        None => format!(
            "pushover provider `{provider_id}` returned status {}",
            response
                .map(|response| response.status.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
    };

    if let Some(errors) = response.and_then(|response| response.errors.as_ref()) {
        let errors = errors
            .iter()
            .map(|error| error.trim())
            .filter(|error| !error.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
        if !errors.is_empty() {
            message.push_str(": ");
            message.push_str(&errors);
        }
    }

    message
}

fn is_retriable_pushover_http_status(status: u16) -> bool {
    status == 408 || status >= 500
}

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use wiremock::matchers::{body_string_contains, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};

    #[tokio::test]
    async fn sends_form_request_to_pushover_message_api() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/1/messages.json"))
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string_contains("token=123456789012345678901234567890"))
            .and(body_string_contains("user=ABCDEFGHIJABCDEFGHIJABCDEFGHIJ"))
            .and(body_string_contains("title=Codex"))
            .and(body_string_contains("message=Ready+for+review."))
            .and(body_string_contains("device=iphone"))
            .and(body_string_contains("sound=pushover"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": 1,
                "request": "647d2300-702c-4b38-8b2f-d56326ae460b"
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/1/messages.json", server.uri()));

        let result = provider
            .send(&test_signal())
            .await
            .expect("pushover send should succeed");

        assert_eq!(result.provider_id, "pushover");
        assert_eq!(result.provider_type, "pushover");
        assert_eq!(result.signal_id, "signal-1");
        assert_eq!(result.status, ProviderSendStatus::Sent);
        assert_eq!(result.http_status, Some(200));
        assert_eq!(result.provider_message_id, None);
    }

    #[tokio::test]
    async fn returns_provider_rejected_when_response_status_is_not_successful() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/1/messages.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": 0,
                "request": "5042853c-402d-4a18-abcb-168734a801de",
                "errors": ["user identifier is invalid"]
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/1/messages.json", server.uri()));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("Pushover status 0 should fail");

        assert!(err.to_string().contains("user identifier is invalid"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.context.signal_id, "signal-1");
        assert_eq!(err.context.provider_id.as_deref(), Some("pushover"));
        assert_eq!(err.context.provider_type.as_deref(), Some("pushover"));
        assert_eq!(err.http_status, Some(200));
        assert_eq!(err.provider_code.as_deref(), Some("0"));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn returns_non_retriable_error_for_quota_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/1/messages.json"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "status": 0,
                "request": "5042853c-402d-4a18-abcb-168734a801de",
                "errors": ["monthly message limit reached"]
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/1/messages.json", server.uri()));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("HTTP 429 should fail");

        assert!(err.to_string().contains("HTTP status 429"));
        assert!(err.to_string().contains("monthly message limit reached"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.http_status, Some(429));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn returns_retriable_error_for_server_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/1/messages.json"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/1/messages.json", server.uri()));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("HTTP 500 should fail");

        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.http_status, Some(500));
        assert!(err.retriable);
    }

    #[tokio::test]
    async fn rejects_message_that_exceeds_pushover_limit_before_sending() {
        let server = MockServer::start().await;
        let provider = test_provider(format!("{}/1/messages.json", server.uri()));
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            &"a".repeat(1100),
            test_timestamp(),
            BTreeMap::new(),
        );

        let err = provider
            .send(&signal)
            .await
            .expect_err("oversized Pushover message should fail before network send");

        assert_eq!(err.kind, DeliveryErrorKind::Validation);
        assert!(err.to_string().contains("message exceeds 1024"));
        assert!(
            server
                .received_requests()
                .await
                .expect("requests should be available")
                .is_empty()
        );
    }

    #[test]
    fn resolves_inline_config_values() {
        let provider = PushoverProvider::from_config(&ProviderConfig {
            id: "pushover".to_string(),
            provider_type: ProviderType::Pushover,
            server: None,
            topic: None,
            url: None,
            url_env: None,
            secret: None,
            secret_env: None,
            app_token: Some("123456789012345678901234567890".to_string()),
            app_token_env: None,
            user_key: Some("ABCDEFGHIJABCDEFGHIJABCDEFGHIJ".to_string()),
            user_key_env: None,
            device: Some("iphone,work_mac".to_string()),
            sound: Some("pushover".to_string()),
            bot_token: None,
            bot_token_env: None,
            chat_id: None,
            access_token: None,
            access_token_env: None,
            phone_number_id: None,
            recipient_phone_number: None,
        })
        .expect("pushover provider config should be valid");

        assert_eq!(provider.id, "pushover");
        assert_eq!(provider.device.as_deref(), Some("iphone,work_mac"));
        assert_eq!(provider.sound.as_deref(), Some("pushover"));
    }

    #[test]
    fn rejects_invalid_inline_app_token() {
        let err = PushoverProvider::from_config(&ProviderConfig {
            id: "pushover".to_string(),
            provider_type: ProviderType::Pushover,
            server: None,
            topic: None,
            url: None,
            url_env: None,
            secret: None,
            secret_env: None,
            app_token: Some("too-short".to_string()),
            app_token_env: None,
            user_key: Some("ABCDEFGHIJABCDEFGHIJABCDEFGHIJ".to_string()),
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
        })
        .expect_err("invalid token should fail");

        assert!(err.to_string().contains("app_token"));
        assert!(err.to_string().contains("30 letters or numbers"));
    }

    fn test_provider(endpoint: String) -> PushoverProvider {
        PushoverProvider {
            id: "pushover".to_string(),
            endpoint,
            app_token: "123456789012345678901234567890".to_string(),
            user_key: "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ".to_string(),
            device: Some("iphone".to_string()),
            sound: Some("pushover".to_string()),
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
