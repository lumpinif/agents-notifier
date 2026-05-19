use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    provider_request_error,
};
use crate::provider_catalog::{MessageSurface, provider_message_limit};
use crate::providers::formatting::format_signal_body;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

const PUSHOVER_MESSAGES_ENDPOINT: &str = "https://api.pushover.net/1/messages.json";

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
                title: signal.title(),
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
    let title_limit = provider_message_limit(ProviderType::Pushover, MessageSurface::Title);
    if signal.title().chars().count() > title_limit {
        return Err(Box::new(DeliveryError::new(
            DeliveryErrorKind::Validation,
            DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
            format!("pushover provider `{provider_id}` title exceeds {title_limit} characters"),
        )));
    }

    let message_limit = provider_message_limit(ProviderType::Pushover, MessageSurface::MessageBody);
    if message.chars().count() > message_limit {
        return Err(Box::new(DeliveryError::new(
            DeliveryErrorKind::Validation,
            DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
            format!("pushover provider `{provider_id}` message exceeds {message_limit} characters"),
        )));
    }

    Ok(())
}

fn format_pushover_body(signal: &Signal) -> String {
    format_signal_body(signal, &format_local_timestamp(signal.timestamp))
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
mod tests;
