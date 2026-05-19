use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};
use crate::provider_catalog::{MessageSurface, provider_message_limit};

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
        "a".repeat(
            provider_message_limit(ProviderType::Pushover, MessageSurface::MessageBody) + 100,
        ),
        test_timestamp(),
        BTreeMap::new(),
    );

    let err = provider
        .send(&signal)
        .await
        .expect_err("oversized Pushover message should fail before network send");

    assert_eq!(err.kind, DeliveryErrorKind::Validation);
    assert!(err.to_string().contains(&format!(
        "message exceeds {}",
        provider_message_limit(ProviderType::Pushover, MessageSurface::MessageBody)
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
fn resolves_inline_config_values() {
    let provider = PushoverProvider::from_config(&ProviderConfig {
        id: "pushover".to_string(),
        provider_type: ProviderType::Pushover,
        base_url: None,
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
        base_url: None,
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
