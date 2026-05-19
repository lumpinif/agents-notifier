use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::config::{SecretSource, WechatProviderConfig};
use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};
use crate::provider_catalog::{MessageSurface, provider_local_preflight_message_limit};

#[tokio::test]
async fn sends_text_message_to_wechat_ilink() {
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

    assert_eq!(result.provider_id, "wechat");
    assert_eq!(result.provider_type, "wechat");
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
            .is_some_and(|value| value.starts_with("agents-router-"))
    );
    assert_eq!(
        body["msg"]["item_list"][0]["text_item"]["text"],
        format!(
            "Codex\n\nReady for review.\nTime: {}",
            format_local_timestamp(test_timestamp())
        )
    );
    assert_eq!(body["base_info"]["channel_version"], WECHAT_CHANNEL_VERSION);
}

#[tokio::test]
async fn accepts_wechat_success_with_non_json_response_body() {
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
async fn returns_provider_rejected_for_wechat_error_response() {
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
async fn rejects_wechat_text_that_exceeds_limit_before_sending() {
    let server = MockServer::start().await;
    let provider = test_provider(format!("{}/ilink/bot/sendmessage", server.uri()));
    let signal = Signal::new_with_timestamp(
        "signal-1",
        "codex_cli",
        "codex_cli",
        "Codex",
        "a".repeat(
            provider_local_preflight_message_limit(ProviderType::Wechat, MessageSurface::TextBody)
                + 100,
        ),
        test_timestamp(),
        BTreeMap::new(),
    );

    let err = provider
        .send(&signal)
        .await
        .expect_err("oversized WeChat message should fail before network send");

    assert_eq!(err.kind, DeliveryErrorKind::Validation);
    assert!(err.to_string().contains(&format!(
        "text body exceeds {}",
        provider_local_preflight_message_limit(ProviderType::Wechat, MessageSurface::TextBody)
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
fn validates_wechat_config_values() {
    let provider = WechatProvider::from_config(&ProviderConfig {
        id: "wechat".to_string(),
        detail: ProviderConfigDetail::Wechat(WechatProviderConfig {
            base_url: "https://ilinkai.weixin.qq.com".to_string(),
            token: SecretSource::Inline("test-token".to_string()),
            recipient_user_id: "user@im.wechat".to_string(),
            context_token: SecretSource::Inline("test-context-token".to_string()),
            route_tag: Some("test-route".to_string()),
        }),
    })
    .expect("WeChat provider config should be valid");

    assert_eq!(provider.id, "wechat");
    assert_eq!(provider.recipient_user_id, "user@im.wechat");
    assert_eq!(provider.route_tag.as_deref(), Some("test-route"));
}

fn test_provider(endpoint: String) -> WechatProvider {
    WechatProvider {
        id: "wechat".to_string(),
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
