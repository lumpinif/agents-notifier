use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::delivery::{DeliveryErrorKind, ProviderSendStatus};

#[tokio::test]
async fn sends_interactive_card_to_custom_bot_webhook() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/open-apis/bot/v2/hook/test"))
        .and(body_partial_json(json!({
            "msg_type": "interactive"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 0,
            "msg": "success",
            "data": {}
        })))
        .mount(&server)
        .await;

    let provider = test_provider(format!("{}/open-apis/bot/v2/hook/test", server.uri()), None);

    let result = provider
        .send(&test_signal())
        .await
        .expect("feishu_lark send should succeed");

    assert_eq!(result.provider_id, "work_chat");
    assert_eq!(result.provider_type, "feishu_lark");
    assert_eq!(result.signal_id, "signal-1");
    assert_eq!(result.status, ProviderSendStatus::Sent);
    assert_eq!(result.http_status, Some(200));

    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");
    let body: serde_json::Value = requests[0]
        .body_json()
        .expect("request body should be JSON");
    assert_eq!(
        body["card"]["header"]["title"]["content"],
        "Codex · Test Mac"
    );
    assert_eq!(body["card"]["header"]["template"], "purple");
    assert_eq!(body["card"]["config"]["wide_screen_mode"], true);
    let elements = body["card"]["elements"]
        .as_array()
        .expect("card elements should be present");
    assert_eq!(elements[0]["content"], "**Ready for review.**");
}

#[tokio::test]
async fn sends_signature_fields_when_secret_is_configured() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/open-apis/bot/v2/hook/test"))
        .and(body_partial_json(json!({
            "msg_type": "interactive",
            "card": {
                "header": {
                    "title": {
                        "content": "Codex · Test Mac"
                    }
                }
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 0,
            "msg": "success",
            "data": {}
        })))
        .mount(&server)
        .await;

    let provider = test_provider(
        format!("{}/open-apis/bot/v2/hook/test", server.uri()),
        Some("demo".to_string()),
    );

    provider
        .send(&test_signal())
        .await
        .expect("feishu_lark send should succeed");

    let requests = server
        .received_requests()
        .await
        .expect("requests should be recorded");
    let body: serde_json::Value = requests[0]
        .body_json()
        .expect("request body should be JSON");
    assert!(
        body["timestamp"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(body["sign"].as_str().is_some_and(|value| !value.is_empty()));
}

#[test]
fn signs_request_with_feishu_lark_algorithm() {
    assert_eq!(
        sign_request("100", "demo"),
        "jquNHnVOwmDRfw+vqTIrY5dooJAgi5EcRtLsQE4wfXg="
    );
}

#[test]
fn formats_card_elements_with_supplied_time() {
    assert_eq!(
        format_signal_card_elements_with_time(&test_signal(), "2026-05-08 20:00:00 +08:00"),
        vec![FeishuLarkCardElement::Markdown {
            content: "**Ready for review.**".to_string()
        }]
    );
}

#[test]
fn formats_codex_desktop_message_with_clickable_open_link() {
    let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let signal = Signal::new_with_timestamp(
        "signal-1",
        "codex_desktop",
        "codex_desktop",
        "Codex Desktop",
        "Project: agents-notifier\nProject Path: /Users/tester/projects/agents-notifier\nSession: agents-notifier sync report\nOpen in Codex: codex://threads/session-1\nDuration: 1m 32s\nBranch: main\n\nPreview: Ready for review.",
        timestamp,
        BTreeMap::new(),
    );

    assert_eq!(
        format_signal_card_elements_with_time(&signal, "2026-05-10 01:35:42 +08:00"),
        vec![
            FeishuLarkCardElement::Div {
                text: FeishuLarkLarkMarkdown {
                    tag: "lark_md",
                    content: "**agents-notifier sync report**".to_string()
                }
            },
            FeishuLarkCardElement::ColumnSet {
                flex_mode: "bisect",
                background_style: "default",
                columns: vec![
                    metric_column("Project Name", "agents-notifier", None),
                    metric_column("Branch", "main", None)
                ]
            },
            FeishuLarkCardElement::ColumnSet {
                flex_mode: "bisect",
                background_style: "default",
                columns: vec![
                    metric_column("Duration", "1m 32s", None),
                    metric_column("Time", "2026-05-10 01:35:42", None)
                ]
            },
            FeishuLarkCardElement::Action {
                actions: vec![FeishuLarkCardAction {
                    tag: "button",
                    text: FeishuLarkPlainText {
                        tag: "plain_text",
                        content: "Open in Codex".to_string()
                    },
                    url: "http://127.0.0.1:17674/open/codex/thread/session-1".to_string(),
                    button_type: "primary"
                }]
            },
            FeishuLarkCardElement::Divider,
            FeishuLarkCardElement::Markdown {
                content: "**Preview**\nReady for review.".to_string()
            },
        ]
    );
}

#[test]
fn builds_card_body_from_signal_body() {
    assert_eq!(
        FeishuLarkCardBody::from_body(
            "Project: agents-notifier\nOpen in Codex: codex://threads/session-1\nPreview: done"
        ),
        FeishuLarkCardBody {
            project: Some("agents-notifier".to_string()),
            project_path: None,
            session: None,
            duration: None,
            branch: None,
            time: None,
            other_details: Vec::new(),
            open_url: Some("http://127.0.0.1:17674/open/codex/thread/session-1".to_string()),
            prompt: None,
            answer: Some(FeishuLarkAnswerBlock {
                label: "Preview",
                content: "done".to_string(),
            }),
        }
    );
}

#[test]
fn builds_card_body_with_multiline_answer_block() {
    assert_eq!(
        FeishuLarkCardBody::from_body(
            "Project: agents-notifier\nOpen in Codex: codex://threads/session-1\n\nAnswer: Fixed the route.\n\nRun tests next."
        ),
        FeishuLarkCardBody {
            project: Some("agents-notifier".to_string()),
            project_path: None,
            session: None,
            duration: None,
            branch: None,
            time: None,
            other_details: Vec::new(),
            open_url: Some("http://127.0.0.1:17674/open/codex/thread/session-1".to_string()),
            prompt: None,
            answer: Some(FeishuLarkAnswerBlock {
                label: "Answer",
                content: "Fixed the route.\n\nRun tests next.".to_string(),
            }),
        }
    );
}

#[test]
fn builds_card_body_with_prompt_before_answer() {
    assert_eq!(
        FeishuLarkCardBody::from_body(
            "Project: agents-notifier\nOpen in Codex: codex://threads/session-1\n\nPrompt: Fix the route.\n\nAnswer: Fixed the route."
        ),
        FeishuLarkCardBody {
            project: Some("agents-notifier".to_string()),
            project_path: None,
            session: None,
            duration: None,
            branch: None,
            time: None,
            other_details: Vec::new(),
            open_url: Some("http://127.0.0.1:17674/open/codex/thread/session-1".to_string()),
            prompt: Some("Fix the route.".to_string()),
            answer: Some(FeishuLarkAnswerBlock {
                label: "Answer",
                content: "Fixed the route.".to_string(),
            }),
        }
    );
}

#[test]
fn formats_prompt_before_answer() {
    let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let signal = Signal::new_with_timestamp(
        "signal-1",
        "codex_desktop",
        "codex_desktop",
        "Codex Desktop",
        "Project: agents-notifier\nOpen in Codex: codex://threads/session-1\n\nPrompt: Fix the route.\n\nAnswer: Fixed the route.",
        timestamp,
        BTreeMap::new(),
    );

    let elements = format_signal_card_elements_with_time(&signal, "2026-05-10 01:35:42 +08:00");

    let prompt_index = elements
        .iter()
        .position(|element| {
            matches!(
                element,
                FeishuLarkCardElement::Markdown { content }
                    if content == "**Prompt**\nFix the route."
            )
        })
        .expect("prompt should be rendered");
    let answer_index = elements
        .iter()
        .position(|element| {
            matches!(
                element,
                FeishuLarkCardElement::Markdown { content }
                    if content == "**Answer**\nFixed the route."
            )
        })
        .expect("answer should be rendered");

    assert!(prompt_index < answer_index);
}

#[test]
fn serializes_card_with_header_action_and_preview() {
    let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let signal = Signal::new_with_timestamp(
        "signal-1",
        "codex_desktop",
        "codex_desktop",
        "Codex Desktop",
        "Project: agents-notifier\nOpen in Codex: codex://threads/session-1\nPreview: done",
        timestamp,
        BTreeMap::new(),
    );

    let request = FeishuLarkInteractiveRequest::from_signal(&signal, None, "Test Mac");
    let body = serde_json::to_value(request).expect("request should serialize");

    assert_eq!(body["msg_type"], "interactive");
    assert_eq!(
        body["card"]["header"]["title"]["content"],
        "Codex Desktop · Test Mac"
    );
    assert_eq!(body["card"]["header"]["template"], "purple");
    assert_eq!(
        body["card"]["elements"][1]["actions"][0]["url"],
        "http://127.0.0.1:17674/open/codex/thread/session-1"
    );
    assert_eq!(
        body["card"]["elements"][2],
        json!({
            "tag": "hr"
        })
    );
    assert_eq!(
        body["card"]["elements"][3],
        json!({
            "tag": "markdown",
            "content": "**Preview**\ndone"
        })
    );
}

#[test]
fn formats_codex_desktop_header_with_machine_source_and_session() {
    let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let signal = Signal::new_with_timestamp(
        "signal-1",
        "codex_desktop",
        "codex_desktop",
        "Codex Desktop",
        "Project: agents-notifier\nSession: 为 Codex 桌面转发加 deeplink\nDuration: 7m 12s\nBranch: main",
        timestamp,
        BTreeMap::new(),
    );

    let request = FeishuLarkInteractiveRequest::from_signal(&signal, None, "Felix’s MacBook Pro");
    let body = serde_json::to_value(request).expect("request should serialize");

    assert_eq!(
        body["card"]["header"]["title"]["content"],
        "Codex Desktop · 为 Codex 桌面转发加 deeplink · Felix’s MacBook Pro"
    );
}

#[test]
fn formats_local_timestamp_with_numeric_offset() {
    let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    let formatted = format_local_timestamp(timestamp);

    assert_eq!(formatted.len(), "2026-05-08 20:00:00 +08:00".len());
    assert!(matches!(formatted.as_bytes()[20], b'+' | b'-'));
    assert!(!formatted.contains("UTC"));
}

#[tokio::test]
async fn returns_error_for_non_success_response_code() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/open-apis/bot/v2/hook/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "code": 19021,
            "msg": "sign match fail or timestamp is not within one hour from current time"
        })))
        .mount(&server)
        .await;

    let provider = test_provider(
        format!("{}/open-apis/bot/v2/hook/test", server.uri()),
        Some("demo".to_string()),
    );

    let err = provider
        .send(&test_signal())
        .await
        .expect_err("nonzero provider code should fail");

    assert!(err.to_string().contains("returned code 19021"));
    assert!(err.to_string().contains("sign match fail"));
    assert_eq!(err.kind, DeliveryErrorKind::ProviderRejected);
    assert_eq!(err.context.signal_id, "signal-1");
    assert_eq!(err.context.provider_id.as_deref(), Some("work_chat"));
    assert_eq!(err.context.provider_type.as_deref(), Some("feishu_lark"));
    assert_eq!(err.http_status, Some(200));
    assert_eq!(err.provider_code.as_deref(), Some("19021"));
    assert!(!err.retriable);
}

fn test_signal() -> Signal {
    let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    Signal::new_with_timestamp(
        "signal-1",
        "codex_desktop",
        "codex_desktop",
        "Codex",
        "Ready for review.",
        timestamp,
        BTreeMap::new(),
    )
}

fn test_provider(url: String, secret: Option<String>) -> FeishuLarkProvider {
    FeishuLarkProvider {
        id: "work_chat".to_string(),
        url,
        secret,
        computer_name: "Test Mac".to_string(),
        client: reqwest::Client::new(),
    }
}
