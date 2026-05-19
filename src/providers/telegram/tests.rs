use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::config::{SecretSource, TelegramProviderConfig};
use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};
use crate::provider_catalog::{MessageSurface, provider_local_preflight_message_limit};

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
        "a".repeat(
            provider_local_preflight_message_limit(
                ProviderType::Telegram,
                MessageSurface::TextBody,
            ) + 100,
        ),
        test_timestamp(),
        BTreeMap::new(),
    );

    let err = provider
        .send(&signal)
        .await
        .expect_err("oversized Telegram message should fail before network send");

    assert_eq!(err.kind, DeliveryErrorKind::Validation);
    assert!(err.to_string().contains(&format!(
        "text exceeds {}",
        provider_local_preflight_message_limit(ProviderType::Telegram, MessageSurface::TextBody)
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
fn validates_telegram_config_values() {
    let provider = TelegramProvider::from_config(&ProviderConfig {
        id: "telegram".to_string(),
        detail: ProviderConfigDetail::Telegram(TelegramProviderConfig {
            bot_token: SecretSource::Inline("123456:test-token".to_string()),
            chat_id: "@agents_router".to_string(),
        }),
    })
    .expect("Telegram provider config should be valid");

    assert_eq!(provider.id, "telegram");
    assert_eq!(provider.chat_id, "@agents_router");
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
