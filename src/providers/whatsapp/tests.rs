use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};
use crate::provider_catalog::{MessageSurface, provider_local_preflight_message_limit};

#[tokio::test]
async fn sends_text_message_to_whatsapp_cloud_api() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v23.0/123456789/messages"))
        .and(header("authorization", "Bearer test-access-token"))
        .and(body_json(serde_json::json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "to": "15551234567",
            "type": "text",
            "text": {
                "body": format!(
                    "Codex\n\nReady for review.\nTime: {}",
                    format_local_timestamp(test_timestamp())
                )
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "messaging_product": "whatsapp",
            "contacts": [{ "input": "15551234567", "wa_id": "15551234567" }],
            "messages": [{ "id": "wamid.test" }]
        })))
        .mount(&server)
        .await;

    let provider = test_provider(format!("{}/v23.0/123456789/messages", server.uri()));

    let result = provider
        .send(&test_signal())
        .await
        .expect("WhatsApp send should succeed");

    assert_eq!(result.provider_id, "whatsapp");
    assert_eq!(result.provider_type, "whatsapp");
    assert_eq!(result.signal_id, "signal-1");
    assert_eq!(result.status, ProviderSendStatus::Sent);
    assert_eq!(result.http_status, Some(200));
    assert_eq!(result.provider_message_id.as_deref(), Some("wamid.test"));
}

#[tokio::test]
async fn returns_provider_rejected_for_whatsapp_error_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v23.0/123456789/messages"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": {
                "message": "Unsupported post request",
                "type": "GraphMethodException",
                "code": 100,
                "error_subcode": 33
            }
        })))
        .mount(&server)
        .await;

    let provider = test_provider(format!("{}/v23.0/123456789/messages", server.uri()));

    let err = provider
        .send(&test_signal())
        .await
        .expect_err("WhatsApp error response should fail");

    assert!(err.to_string().contains("Unsupported post request"));
    assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
    assert_eq!(err.http_status, Some(400));
    assert_eq!(err.provider_code.as_deref(), Some("100"));
    assert!(!err.retriable);
}

#[tokio::test]
async fn rejects_whatsapp_text_that_exceeds_limit_before_sending() {
    let server = MockServer::start().await;
    let provider = test_provider(format!("{}/v23.0/123456789/messages", server.uri()));
    let signal = Signal::new_with_timestamp(
        "signal-1",
        "codex_cli",
        "codex_cli",
        "Codex",
        "a".repeat(
            provider_local_preflight_message_limit(
                ProviderType::Whatsapp,
                MessageSurface::TextBody,
            ) + 100,
        ),
        test_timestamp(),
        BTreeMap::new(),
    );

    let err = provider
        .send(&signal)
        .await
        .expect_err("oversized WhatsApp message should fail before network send");

    assert_eq!(err.kind, DeliveryErrorKind::Validation);
    assert!(err.to_string().contains(&format!(
        "text body exceeds {}",
        provider_local_preflight_message_limit(ProviderType::Whatsapp, MessageSurface::TextBody)
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
fn validates_whatsapp_config_values() {
    let provider = WhatsappProvider::from_config(&ProviderConfig {
        id: "whatsapp".to_string(),
        provider_type: ProviderType::Whatsapp,
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
        bot_token: None,
        bot_token_env: None,
        chat_id: None,
        access_token: Some("test-access-token".to_string()),
        access_token_env: None,
        phone_number_id: Some("123456789".to_string()),
        recipient_phone_number: Some("15551234567".to_string()),
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
    .expect("WhatsApp provider config should be valid");

    assert_eq!(provider.id, "whatsapp");
    assert_eq!(provider.recipient_phone_number, "15551234567");
}

fn test_provider(endpoint: String) -> WhatsappProvider {
    WhatsappProvider {
        id: "whatsapp".to_string(),
        endpoint,
        access_token: "test-access-token".to_string(),
        recipient_phone_number: "15551234567".to_string(),
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
