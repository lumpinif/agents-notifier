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

const TELEGRAM_TEXT_LIMIT: usize = 4096;

#[derive(Debug)]
pub struct TelegramProvider {
    id: String,
    endpoint: String,
    chat_id: String,
    client: reqwest::Client,
}

impl TelegramProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::Telegram {
            return Err(anyhow!(
                "provider `{}` is not a telegram provider",
                config.id
            ));
        }

        let bot_token = resolve_secret_value(
            config,
            "bot_token",
            config.bot_token.as_deref(),
            "bot_token_env",
            config.bot_token_env.as_deref(),
        )?;
        validate_bot_token(&config.id, &bot_token)?;

        let chat_id = present(config.chat_id.as_deref())
            .ok_or_else(|| anyhow!("telegram provider `{}` requires `chat_id`", config.id))?
            .to_string();
        validate_chat_id(&config.id, &chat_id)?;

        Ok(Self {
            id: config.id.clone(),
            endpoint: format!("https://api.telegram.org/bot{bot_token}/sendMessage"),
            chat_id,
            client: provider_http_client()?,
        })
    }
}

impl Provider for TelegramProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::Telegram.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::Telegram.as_str();
            let text = format_telegram_text(signal);
            validate_text_size(signal, &self.id, provider_type, &text).map_err(|error| *error)?;

            let request = TelegramSendMessageRequest {
                chat_id: &self.chat_id,
                text: &text,
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
                        "telegram",
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
                    format!("telegram provider `{}` failed to read response", self.id),
                )
                .with_http_status(status_code)
                .with_retriable(true)
                .with_source(error.without_url())
            })?;

            let provider_response = parse_telegram_response(&response_body).ok();
            if !status.is_success() {
                let mut error = DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_telegram_rejection_message(
                        &self.id,
                        Some(status.as_str()),
                        provider_response.as_ref(),
                        &response_body,
                    ),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_http_status(status_code));
                if let Some(code) = provider_response.and_then(|response| response.error_code) {
                    error = error.with_provider_code(code.to_string());
                }

                return Err(error);
            }

            let provider_response = parse_telegram_response(&response_body).map_err(|error| {
                DeliveryError::new(
                    DeliveryErrorKind::ProviderResponse,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!(
                        "telegram provider `{}` returned invalid response JSON",
                        self.id
                    ),
                )
                .with_http_status(status_code)
                .with_source(error)
            })?;

            if !provider_response.ok {
                let mut error = DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_telegram_rejection_message(
                        &self.id,
                        None,
                        Some(&provider_response),
                        &response_body,
                    ),
                )
                .with_http_status(status_code);
                if let Some(code) = provider_response.error_code {
                    error = error.with_provider_code(code.to_string());
                }

                return Err(error);
            }

            let mut result = ProviderSendResult::sent(&self.id, provider_type, signal)
                .with_http_status(status_code);
            if let Some(message_id) = provider_response
                .result
                .and_then(|message| message.message_id)
            {
                result = result.with_provider_message_id(message_id.to_string());
            }

            Ok(result)
        })
    }
}

#[derive(Debug, Serialize)]
struct TelegramSendMessageRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
}

#[derive(Debug, Deserialize)]
struct TelegramResponse {
    ok: bool,
    result: Option<TelegramMessage>,
    error_code: Option<i64>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: Option<i64>,
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
                "telegram provider `{}` env var `{}` from `{}` is not set",
                config.id, env_name, env_field
            )
        }),
        _ => Err(anyhow!(
            "telegram provider `{}` must set exactly one of `{}` or `{}`",
            config.id,
            value_field,
            env_field
        )),
    }
}

fn validate_bot_token(provider_id: &str, token: &str) -> anyhow::Result<()> {
    let Some((bot_id, secret)) = token.split_once(':') else {
        return Err(anyhow!(
            "telegram provider `{provider_id}` `bot_token` must look like `123456:ABC...`"
        ));
    };

    let valid = !bot_id.is_empty()
        && bot_id.bytes().all(|byte| byte.is_ascii_digit())
        && !secret.is_empty()
        && secret.bytes().all(|byte| !byte.is_ascii_whitespace());
    if valid {
        Ok(())
    } else {
        Err(anyhow!(
            "telegram provider `{provider_id}` `bot_token` must look like `123456:ABC...`"
        ))
    }
}

fn validate_chat_id(provider_id: &str, chat_id: &str) -> anyhow::Result<()> {
    let valid = if let Some(username) = chat_id.strip_prefix('@') {
        !username.is_empty()
            && username
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    } else {
        let digits = chat_id.strip_prefix('-').unwrap_or(chat_id);
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    };

    if valid {
        Ok(())
    } else {
        Err(anyhow!(
            "telegram provider `{provider_id}` `chat_id` must be a numeric chat id or an `@channelusername`"
        ))
    }
}

fn validate_text_size(
    signal: &Signal,
    provider_id: &str,
    provider_type: &str,
    text: &str,
) -> Result<(), Box<DeliveryError>> {
    if text.chars().count() > TELEGRAM_TEXT_LIMIT {
        return Err(Box::new(DeliveryError::new(
            DeliveryErrorKind::Validation,
            DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
            format!("telegram provider `{provider_id}` text exceeds 4096 characters"),
        )));
    }

    Ok(())
}

fn format_telegram_text(signal: &Signal) -> String {
    let body = body_with_local_time(&signal.body, &format_local_timestamp(signal.timestamp));
    format!("{}\n\n{}", signal.title, body)
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

fn parse_telegram_response(body: &str) -> Result<TelegramResponse, serde_json::Error> {
    serde_json::from_str(body)
}

fn format_telegram_rejection_message(
    provider_id: &str,
    http_status: Option<&str>,
    response: Option<&TelegramResponse>,
    response_body: &str,
) -> String {
    let mut message = match http_status {
        Some(http_status) => {
            format!("telegram provider `{provider_id}` returned HTTP status {http_status}")
        }
        None => format!("telegram provider `{provider_id}` returned `ok: false`"),
    };

    if let Some(description) = response
        .and_then(|response| response.description.as_deref())
        .filter(|description| !description.trim().is_empty())
    {
        message.push_str(": ");
        message.push_str(description.trim());
    } else {
        message.push_str(&response_summary_suffix(response_body));
    }

    message
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
    async fn sends_message_to_telegram_bot_api() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/bot123456:test-token/sendMessage"))
            .and(body_json(serde_json::json!({
                "chat_id": "12345",
                "text": format!(
                    "Codex\n\nReady for review.\nTime: {}",
                    format_local_timestamp(test_timestamp())
                )
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "result": { "message_id": 42 }
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/bot123456:test-token/sendMessage", server.uri()));

        let result = provider
            .send(&test_signal())
            .await
            .expect("Telegram send should succeed");

        assert_eq!(result.provider_id, "telegram");
        assert_eq!(result.provider_type, "telegram");
        assert_eq!(result.signal_id, "signal-1");
        assert_eq!(result.status, ProviderSendStatus::Sent);
        assert_eq!(result.http_status, Some(200));
        assert_eq!(result.provider_message_id.as_deref(), Some("42"));
    }

    #[tokio::test]
    async fn returns_provider_rejected_for_telegram_error_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/bot123456:test-token/sendMessage"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "ok": false,
                "error_code": 400,
                "description": "Bad Request: chat not found"
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/bot123456:test-token/sendMessage", server.uri()));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("Telegram error response should fail");

        assert!(err.to_string().contains("chat not found"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.http_status, Some(400));
        assert_eq!(err.provider_code.as_deref(), Some("400"));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn rejects_telegram_text_that_exceeds_limit_before_sending() {
        let server = MockServer::start().await;
        let provider = test_provider(format!("{}/bot123456:test-token/sendMessage", server.uri()));
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            "a".repeat(4100),
            test_timestamp(),
            BTreeMap::new(),
        );

        let err = provider
            .send(&signal)
            .await
            .expect_err("oversized Telegram message should fail before network send");

        assert_eq!(err.kind, DeliveryErrorKind::Validation);
        assert!(err.to_string().contains("text exceeds 4096"));
        assert!(
            server
                .received_requests()
                .await
                .expect("requests should be available")
                .is_empty()
        );
    }

    #[test]
    fn validates_telegram_config_values() {
        let provider = TelegramProvider::from_config(&ProviderConfig {
            id: "telegram".to_string(),
            provider_type: ProviderType::Telegram,
            base_url: None,
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
            bot_token: Some("123456:test-token".to_string()),
            bot_token_env: None,
            chat_id: Some("@agents_notifier".to_string()),
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
        .expect("Telegram provider config should be valid");

        assert_eq!(provider.id, "telegram");
        assert_eq!(provider.chat_id, "@agents_notifier");
    }

    fn test_provider(endpoint: String) -> TelegramProvider {
        TelegramProvider {
            id: "telegram".to_string(),
            endpoint,
            chat_id: "12345".to_string(),
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
