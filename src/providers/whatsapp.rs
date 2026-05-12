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
    format_signal_message(signal, &format_local_timestamp(signal.timestamp))
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
mod tests;
