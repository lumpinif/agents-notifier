use anyhow::{Context, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::provider_catalog::{MessageSurface, provider_local_preflight_message_limit};
use crate::providers::formatting::format_signal_message;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

const WECHAT_CHANNEL_VERSION: &str = "agents-router-wechat/1.0";

#[derive(Debug)]
pub struct WechatProvider {
    id: String,
    endpoint: String,
    token: String,
    recipient_user_id: String,
    context_token: String,
    route_tag: Option<String>,
    client: reqwest::Client,
}

impl WechatProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::Wechat {
            return Err(anyhow!("provider `{}` is not a wechat provider", config.id));
        }

        let base_url = config
            .base_url
            .as_deref()
            .ok_or_else(|| anyhow!("wechat provider `{}` requires `base_url`", config.id))?;
        let base_url = validate_base_url(&config.id, base_url)?;

        let token = resolve_secret_value(
            config,
            "token",
            config.token.as_deref(),
            "token_env",
            config.token_env.as_deref(),
        )?;
        let token = validate_secret_token(&config.id, "token", &token)?;

        let recipient_user_id = present(config.recipient_user_id.as_deref())
            .ok_or_else(|| {
                anyhow!(
                    "wechat provider `{}` requires `recipient_user_id`",
                    config.id
                )
            })?
            .to_string();
        validate_recipient_user_id(&config.id, &recipient_user_id)?;

        let context_token = resolve_secret_value(
            config,
            "context_token",
            config.context_token.as_deref(),
            "context_token_env",
            config.context_token_env.as_deref(),
        )?;
        let context_token = validate_secret_token(&config.id, "context_token", &context_token)?;

        let route_tag = resolve_route_tag(&config.id, config.route_tag.as_deref())?;

        Ok(Self {
            id: config.id.clone(),
            endpoint: format!("{base_url}/ilink/bot/sendmessage"),
            token,
            recipient_user_id,
            context_token,
            route_tag,
            client: provider_http_client()?,
        })
    }
}

impl Provider for WechatProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::Wechat.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::Wechat.as_str();
            let text = format_wechat_text(signal);
            validate_text_size(signal, &self.id, provider_type, &text).map_err(|error| *error)?;

            let request = WechatSendMessageRequest {
                msg: WechatOutboundMessage {
                    from_user_id: "",
                    to_user_id: &self.recipient_user_id,
                    client_id: random_client_id(),
                    message_type: 2,
                    message_state: 2,
                    item_list: vec![WechatMessageItem {
                        item_type: 1,
                        text_item: WechatTextItem { text: &text },
                    }],
                    context_token: &self.context_token,
                },
                base_info: WechatBaseInfo {
                    channel_version: WECHAT_CHANNEL_VERSION,
                },
            };

            let mut builder = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.token)
                .header("AuthorizationType", "ilink_bot_token")
                .header("X-WECHAT-UIN", random_wechat_uin())
                .json(&request);
            if let Some(route_tag) = &self.route_tag {
                builder = builder.header("SKRouteTag", route_tag);
            }

            let response = builder.send().await.map_err(|error| {
                let is_timeout = error.is_timeout();
                provider_request_error(
                    signal,
                    &self.id,
                    provider_type,
                    "wechat",
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
                    format!("wechat provider `{}` failed to read response", self.id),
                )
                .with_http_status(status_code)
                .with_retriable(true)
                .with_source(error.without_url())
            })?;

            if !status.is_success() {
                return Err(DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_wechat_http_rejection_message(&self.id, status.as_str(), &response_body),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_http_status(status_code)));
            }

            if let Some(provider_response) = parse_wechat_send_response(&response_body)
                && (provider_response.ret != 0 || provider_response.errcode != 0)
            {
                let provider_code = if provider_response.ret != 0 {
                    format!("ret:{}", provider_response.ret)
                } else {
                    format!("errcode:{}", provider_response.errcode)
                };
                return Err(DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_wechat_api_rejection_message(&self.id, &provider_response),
                )
                .with_http_status(status_code)
                .with_provider_code(provider_code));
            }

            Ok(ProviderSendResult::sent(&self.id, provider_type, signal)
                .with_http_status(status_code))
        })
    }
}

#[derive(Debug, Serialize)]
struct WechatSendMessageRequest<'a> {
    msg: WechatOutboundMessage<'a>,
    base_info: WechatBaseInfo<'a>,
}

#[derive(Debug, Serialize)]
struct WechatBaseInfo<'a> {
    channel_version: &'a str,
}

#[derive(Debug, Serialize)]
struct WechatOutboundMessage<'a> {
    from_user_id: &'a str,
    to_user_id: &'a str,
    client_id: String,
    message_type: i32,
    message_state: i32,
    item_list: Vec<WechatMessageItem<'a>>,
    context_token: &'a str,
}

#[derive(Debug, Serialize)]
struct WechatMessageItem<'a> {
    #[serde(rename = "type")]
    item_type: i32,
    text_item: WechatTextItem<'a>,
}

#[derive(Debug, Serialize)]
struct WechatTextItem<'a> {
    text: &'a str,
}

#[derive(Debug, Deserialize)]
struct WechatSendMessageResponse {
    ret: i64,
    errcode: i64,
    #[serde(default)]
    errmsg: String,
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
                "wechat provider `{}` env var `{}` from `{}` is not set",
                config.id, env_name, env_field
            )
        }),
        _ => Err(anyhow!(
            "wechat provider `{}` must set exactly one of `{}` or `{}`",
            config.id,
            value_field,
            env_field
        )),
    }
}

fn validate_base_url(provider_id: &str, base_url: &str) -> anyhow::Result<String> {
    let base_url = base_url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(base_url).with_context(|| {
        format!("wechat provider `{provider_id}` `base_url` must be a valid HTTPS URL")
    })?;
    let has_root_path = parsed.path().is_empty() || parsed.path() == "/";
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !has_root_path
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        anyhow::bail!(
            "wechat provider `{provider_id}` `base_url` must be an HTTPS origin like `https://ilinkai.weixin.qq.com`"
        );
    }

    Ok(base_url.to_string())
}

fn validate_secret_token(
    provider_id: &str,
    field: &'static str,
    token: &str,
) -> anyhow::Result<String> {
    let token = token.trim();
    if token.is_empty() || token.bytes().any(|byte| byte.is_ascii_whitespace()) {
        anyhow::bail!(
            "wechat provider `{provider_id}` `{field}` must be non-empty and must not contain whitespace"
        );
    }

    Ok(token.to_string())
}

fn validate_recipient_user_id(provider_id: &str, recipient_user_id: &str) -> anyhow::Result<()> {
    if recipient_user_id.trim().is_empty()
        || recipient_user_id
            .bytes()
            .any(|byte| byte.is_ascii_whitespace())
    {
        anyhow::bail!(
            "wechat provider `{provider_id}` `recipient_user_id` must be non-empty and must not contain whitespace"
        );
    }

    Ok(())
}

fn resolve_route_tag(provider_id: &str, route_tag: Option<&str>) -> anyhow::Result<Option<String>> {
    let Some(route_tag) = present(route_tag) else {
        return Ok(None);
    };
    if route_tag.bytes().any(|byte| byte.is_ascii_whitespace()) {
        anyhow::bail!("wechat provider `{provider_id}` `route_tag` must not contain whitespace");
    }

    Ok(Some(route_tag.to_string()))
}

fn validate_text_size(
    signal: &Signal,
    provider_id: &str,
    provider_type: &str,
    text: &str,
) -> Result<(), Box<DeliveryError>> {
    let limit =
        provider_local_preflight_message_limit(ProviderType::Wechat, MessageSurface::TextBody);
    if text.chars().count() <= limit {
        return Ok(());
    }

    Err(Box::new(DeliveryError::new(
        DeliveryErrorKind::Validation,
        DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
        format!("wechat provider `{provider_id}` text body exceeds {limit} characters"),
    )))
}

fn format_wechat_text(signal: &Signal) -> String {
    format_signal_message(signal, &format_local_timestamp(signal.timestamp))
        .trim_end()
        .to_string()
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    let local: DateTime<Local> = DateTime::from(timestamp);
    local.format("%Y-%m-%d %H:%M:%S %:z").to_string()
}

fn format_wechat_http_rejection_message(
    provider_id: &str,
    http_status: &str,
    response_body: &str,
) -> String {
    let summary = response_body.trim();
    if summary.is_empty() {
        return format!("wechat provider `{provider_id}` returned HTTP status {http_status}");
    }

    format!(
        "wechat provider `{provider_id}` returned HTTP status {http_status}: {}",
        truncate_for_error(summary, 512)
    )
}

fn format_wechat_api_rejection_message(
    provider_id: &str,
    response: &WechatSendMessageResponse,
) -> String {
    let errmsg = response.errmsg.trim();
    if errmsg.is_empty() {
        return format!(
            "wechat provider `{provider_id}` returned ret={} errcode={}",
            response.ret, response.errcode
        );
    }

    format!(
        "wechat provider `{provider_id}` returned ret={} errcode={}: {}",
        response.ret, response.errcode, errmsg
    )
}

fn parse_wechat_send_response(response_body: &str) -> Option<WechatSendMessageResponse> {
    let response_body = response_body.trim();
    if response_body.is_empty() {
        return None;
    }

    serde_json::from_str(response_body).ok()
}

fn truncate_for_error(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }

    format!("{}...", value.chars().take(max).collect::<String>())
}

fn random_client_id() -> String {
    let id = Uuid::new_v4().simple().to_string();
    format!("agents-router-{}", &id[..12])
}

fn random_wechat_uin() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    let value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    BASE64_STANDARD.encode(value.to_string())
}

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests;
