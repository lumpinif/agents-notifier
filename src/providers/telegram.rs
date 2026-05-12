use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::providers::formatting::format_signal_message;
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
    format_signal_message(signal, &format_local_timestamp(signal.timestamp))
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
mod tests;
