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
use crate::providers::formatting::body_with_local_time;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

const WEIXIN_CHANNEL_VERSION: &str = "agents-notifier-weixin/1.0";
const WEIXIN_TEXT_LIMIT: usize = 3800;

#[derive(Debug)]
pub struct WeixinProvider {
    id: String,
    endpoint: String,
    token: String,
    recipient_user_id: String,
    context_token: String,
    route_tag: Option<String>,
    client: reqwest::Client,
}

impl WeixinProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::Weixin {
            return Err(anyhow!("provider `{}` is not a weixin provider", config.id));
        }

        let base_url = config
            .base_url
            .as_deref()
            .ok_or_else(|| anyhow!("weixin provider `{}` requires `base_url`", config.id))?;
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
                    "weixin provider `{}` requires `recipient_user_id`",
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

impl Provider for WeixinProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::Weixin.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::Weixin.as_str();
            let text = format_weixin_text(signal);
            validate_text_size(signal, &self.id, provider_type, &text).map_err(|error| *error)?;

            let request = WeixinSendMessageRequest {
                msg: WeixinOutboundMessage {
                    from_user_id: "",
                    to_user_id: &self.recipient_user_id,
                    client_id: random_client_id(),
                    message_type: 2,
                    message_state: 2,
                    item_list: vec![WeixinMessageItem {
                        item_type: 1,
                        text_item: WeixinTextItem { text: &text },
                    }],
                    context_token: &self.context_token,
                },
                base_info: WeixinBaseInfo {
                    channel_version: WEIXIN_CHANNEL_VERSION,
                },
            };

            let mut builder = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.token)
                .header("AuthorizationType", "ilink_bot_token")
                .header("X-WECHAT-UIN", random_weixin_uin())
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
                    "weixin",
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
                    format!("weixin provider `{}` failed to read response", self.id),
                )
                .with_http_status(status_code)
                .with_retriable(true)
                .with_source(error.without_url())
            })?;

            if !status.is_success() {
                return Err(DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format_weixin_http_rejection_message(&self.id, status.as_str(), &response_body),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_http_status(status_code)));
            }

            if let Some(provider_response) = parse_weixin_send_response(&response_body)
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
                    format_weixin_api_rejection_message(&self.id, &provider_response),
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
struct WeixinSendMessageRequest<'a> {
    msg: WeixinOutboundMessage<'a>,
    base_info: WeixinBaseInfo<'a>,
}

#[derive(Debug, Serialize)]
struct WeixinBaseInfo<'a> {
    channel_version: &'a str,
}

#[derive(Debug, Serialize)]
struct WeixinOutboundMessage<'a> {
    from_user_id: &'a str,
    to_user_id: &'a str,
    client_id: String,
    message_type: i32,
    message_state: i32,
    item_list: Vec<WeixinMessageItem<'a>>,
    context_token: &'a str,
}

#[derive(Debug, Serialize)]
struct WeixinMessageItem<'a> {
    #[serde(rename = "type")]
    item_type: i32,
    text_item: WeixinTextItem<'a>,
}

#[derive(Debug, Serialize)]
struct WeixinTextItem<'a> {
    text: &'a str,
}

#[derive(Debug, Deserialize)]
struct WeixinSendMessageResponse {
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
                "weixin provider `{}` env var `{}` from `{}` is not set",
                config.id, env_name, env_field
            )
        }),
        _ => Err(anyhow!(
            "weixin provider `{}` must set exactly one of `{}` or `{}`",
            config.id,
            value_field,
            env_field
        )),
    }
}

fn validate_base_url(provider_id: &str, base_url: &str) -> anyhow::Result<String> {
    let base_url = base_url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(base_url).with_context(|| {
        format!("weixin provider `{provider_id}` `base_url` must be a valid HTTPS URL")
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
            "weixin provider `{provider_id}` `base_url` must be an HTTPS origin like `https://ilinkai.weixin.qq.com`"
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
            "weixin provider `{provider_id}` `{field}` must be non-empty and must not contain whitespace"
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
            "weixin provider `{provider_id}` `recipient_user_id` must be non-empty and must not contain whitespace"
        );
    }

    Ok(())
}

fn resolve_route_tag(provider_id: &str, route_tag: Option<&str>) -> anyhow::Result<Option<String>> {
    let Some(route_tag) = present(route_tag) else {
        return Ok(None);
    };
    if route_tag.bytes().any(|byte| byte.is_ascii_whitespace()) {
        anyhow::bail!("weixin provider `{provider_id}` `route_tag` must not contain whitespace");
    }

    Ok(Some(route_tag.to_string()))
}

fn validate_text_size(
    signal: &Signal,
    provider_id: &str,
    provider_type: &str,
    text: &str,
) -> Result<(), Box<DeliveryError>> {
    if text.chars().count() <= WEIXIN_TEXT_LIMIT {
        return Ok(());
    }

    Err(Box::new(DeliveryError::new(
        DeliveryErrorKind::Validation,
        DeliveryErrorContext::provider_send(signal, provider_id, provider_type),
        format!("weixin provider `{provider_id}` text body exceeds 3800 characters"),
    )))
}

fn format_weixin_text(signal: &Signal) -> String {
    let body = body_with_local_time(&signal.body, &format_local_timestamp(signal.timestamp));
    format!("{}\n\n{}", signal.title, body.trim_end())
        .trim_end()
        .to_string()
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    let local: DateTime<Local> = DateTime::from(timestamp);
    local.format("%Y-%m-%d %H:%M:%S %:z").to_string()
}

fn format_weixin_http_rejection_message(
    provider_id: &str,
    http_status: &str,
    response_body: &str,
) -> String {
    let summary = response_body.trim();
    if summary.is_empty() {
        return format!("weixin provider `{provider_id}` returned HTTP status {http_status}");
    }

    format!(
        "weixin provider `{provider_id}` returned HTTP status {http_status}: {}",
        truncate_for_error(summary, 512)
    )
}

fn format_weixin_api_rejection_message(
    provider_id: &str,
    response: &WeixinSendMessageResponse,
) -> String {
    let errmsg = response.errmsg.trim();
    if errmsg.is_empty() {
        return format!(
            "weixin provider `{provider_id}` returned ret={} errcode={}",
            response.ret, response.errcode
        );
    }

    format!(
        "weixin provider `{provider_id}` returned ret={} errcode={}: {}",
        response.ret, response.errcode, errmsg
    )
}

fn parse_weixin_send_response(response_body: &str) -> Option<WeixinSendMessageResponse> {
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
    format!("agents-notifier-{}", &id[..12])
}

fn random_weixin_uin() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    let value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    BASE64_STANDARD.encode(value.to_string())
}

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};

    #[tokio::test]
    async fn sends_text_message_to_weixin_ilink() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ilink/bot/sendmessage"))
            .and(header("authorization", "Bearer test-token"))
            .and(header("authorizationtype", "ilink_bot_token"))
            .and(header("skroutetag", "test-route"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ret": 0,
                "errcode": 0,
                "errmsg": ""
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/ilink/bot/sendmessage", server.uri()));

        let result = provider
            .send(&test_signal())
            .await
            .expect("WeChat send should succeed");

        assert_eq!(result.provider_id, "weixin");
        assert_eq!(result.provider_type, "weixin");
        assert_eq!(result.signal_id, "signal-1");
        assert_eq!(result.status, ProviderSendStatus::Sent);
        assert_eq!(result.http_status, Some(200));

        let requests = server
            .received_requests()
            .await
            .expect("requests should be available");
        assert_eq!(requests.len(), 1);
        let body: serde_json::Value =
            serde_json::from_slice(&requests[0].body).expect("request body should be JSON");
        assert_eq!(body["msg"]["from_user_id"], "");
        assert_eq!(body["msg"]["to_user_id"], "user@im.wechat");
        assert_eq!(body["msg"]["message_type"], 2);
        assert_eq!(body["msg"]["message_state"], 2);
        assert_eq!(body["msg"]["context_token"], "test-context-token");
        assert!(
            body["msg"]["client_id"]
                .as_str()
                .is_some_and(|value| value.starts_with("agents-notifier-"))
        );
        assert_eq!(
            body["msg"]["item_list"][0]["text_item"]["text"],
            format!(
                "Codex\n\nReady for review.\nTime: {}",
                format_local_timestamp(test_timestamp())
            )
        );
        assert_eq!(body["base_info"]["channel_version"], WEIXIN_CHANNEL_VERSION);
    }

    #[tokio::test]
    async fn accepts_weixin_success_with_non_json_response_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ilink/bot/sendmessage"))
            .respond_with(ResponseTemplate::new(200).set_body_string("OK"))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/ilink/bot/sendmessage", server.uri()));

        let result = provider
            .send(&test_signal())
            .await
            .expect("WeChat 2xx non-JSON response should succeed");

        assert_eq!(result.status, ProviderSendStatus::Sent);
        assert_eq!(result.http_status, Some(200));
    }

    #[tokio::test]
    async fn returns_provider_rejected_for_weixin_error_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ilink/bot/sendmessage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ret": -2,
                "errcode": -14,
                "errmsg": "context token expired"
            })))
            .mount(&server)
            .await;

        let provider = test_provider(format!("{}/ilink/bot/sendmessage", server.uri()));

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("WeChat error response should fail");

        assert!(err.to_string().contains("context token expired"));
        assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
        assert_eq!(err.http_status, Some(200));
        assert_eq!(err.provider_code.as_deref(), Some("ret:-2"));
        assert!(!err.retriable);
    }

    #[tokio::test]
    async fn rejects_weixin_text_that_exceeds_limit_before_sending() {
        let server = MockServer::start().await;
        let provider = test_provider(format!("{}/ilink/bot/sendmessage", server.uri()));
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            "a".repeat(3900),
            test_timestamp(),
            BTreeMap::new(),
        );

        let err = provider
            .send(&signal)
            .await
            .expect_err("oversized WeChat message should fail before network send");

        assert_eq!(err.kind, DeliveryErrorKind::Validation);
        assert!(err.to_string().contains("text body exceeds 3800"));
        assert!(
            server
                .received_requests()
                .await
                .expect("requests should be available")
                .is_empty()
        );
    }

    #[test]
    fn validates_weixin_config_values() {
        let provider = WeixinProvider::from_config(&ProviderConfig {
            id: "weixin".to_string(),
            provider_type: ProviderType::Weixin,
            base_url: Some("https://ilinkai.weixin.qq.com".to_string()),
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
            token: Some("test-token".to_string()),
            token_env: None,
            recipient_user_id: Some("user@im.wechat".to_string()),
            context_token: Some("test-context-token".to_string()),
            context_token_env: None,
            route_tag: Some("test-route".to_string()),
        })
        .expect("WeChat provider config should be valid");

        assert_eq!(provider.id, "weixin");
        assert_eq!(provider.recipient_user_id, "user@im.wechat");
        assert_eq!(provider.route_tag.as_deref(), Some("test-route"));
    }

    fn test_provider(endpoint: String) -> WeixinProvider {
        WeixinProvider {
            id: "weixin".to_string(),
            endpoint,
            token: "test-token".to_string(),
            recipient_user_id: "user@im.wechat".to_string(),
            context_token: "test-context-token".to_string(),
            route_tag: Some("test-route".to_string()),
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
        DateTime::parse_from_rfc3339("2026-05-10T02:31:43Z")
            .expect("timestamp should parse")
            .with_timezone(&Utc)
    }
}
