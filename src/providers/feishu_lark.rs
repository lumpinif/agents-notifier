use anyhow::{Context, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Local, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::config::{ProviderConfig, ProviderType};
use crate::delivery::{
    DeliveryError, DeliveryErrorContext, DeliveryErrorKind, ProviderSendResult,
    is_retriable_http_status, provider_request_error,
};
use crate::local_machine;
use crate::local_open_bridge::codex_thread_bridge_url;
use crate::providers::formatting::{body_with_local_time, split_trailing_answer_block};
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

type HmacSha256 = Hmac<Sha256>;
const CODEX_CARD_TEMPLATE: &str = "purple";

#[derive(Debug)]
pub struct FeishuLarkProvider {
    id: String,
    url: String,
    secret: Option<String>,
    computer_name: String,
    client: reqwest::Client,
}

impl FeishuLarkProvider {
    pub fn from_config(config: &ProviderConfig) -> anyhow::Result<Self> {
        if config.provider_type != ProviderType::FeishuLark {
            return Err(anyhow!(
                "provider `{}` is not a feishu_lark provider",
                config.id
            ));
        }

        let url = resolve_url(config)?;
        let secret = resolve_secret(config)?;
        let computer_name = local_machine::computer_name()?;

        Ok(Self {
            id: config.id.clone(),
            url,
            secret,
            computer_name,
            client: provider_http_client()?,
        })
    }
}

impl Provider for FeishuLarkProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider_type(&self) -> &str {
        ProviderType::FeishuLark.as_str()
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            let provider_type = ProviderType::FeishuLark.as_str();
            let request = FeishuLarkInteractiveRequest::from_signal(
                signal,
                self.secret.as_deref(),
                &self.computer_name,
            );
            let response = self
                .client
                .post(&self.url)
                .json(&request)
                .send()
                .await
                .map_err(|error| {
                    let is_timeout = error.is_timeout();
                    provider_request_error(
                        signal,
                        &self.id,
                        provider_type,
                        "feishu_lark",
                        is_timeout,
                        error.without_url(),
                    )
                })?;

            let status = response.status();
            let status_code = status.as_u16();
            if !status.is_success() {
                return Err(DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!(
                        "feishu_lark provider `{}` returned HTTP status {}",
                        self.id, status
                    ),
                )
                .with_http_status(status_code)
                .with_retriable(is_retriable_http_status(status_code)));
            }

            let response_body = response.text().await.map_err(|error| {
                DeliveryError::new(
                    DeliveryErrorKind::Network,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!("feishu_lark provider `{}` failed to read response", self.id),
                )
                .with_http_status(status_code)
                .with_retriable(true)
                .with_source(error.without_url())
            })?;
            let provider_response: FeishuLarkResponse = serde_json::from_str(&response_body)
                .map_err(|error| {
                    DeliveryError::new(
                        DeliveryErrorKind::ProviderResponse,
                        DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                        format!(
                            "feishu_lark provider `{}` returned invalid response JSON",
                            self.id
                        ),
                    )
                    .with_http_status(status_code)
                    .with_source(error)
                })?;

            if provider_response.code != 0 {
                let provider_code = provider_response.code.to_string();
                return Err(DeliveryError::new(
                    DeliveryErrorKind::ProviderRejected,
                    DeliveryErrorContext::provider_send(signal, &self.id, provider_type),
                    format!(
                        "feishu_lark provider `{}` returned code {}: {}",
                        self.id,
                        provider_response.code,
                        provider_response
                            .msg
                            .unwrap_or_else(|| "unknown error".to_string())
                    ),
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
struct FeishuLarkInteractiveRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sign: Option<String>,
    msg_type: &'static str,
    card: FeishuLarkCard,
}

impl FeishuLarkInteractiveRequest {
    fn from_signal(signal: &Signal, secret: Option<&str>, computer_name: &str) -> Self {
        let timestamp = secret.map(|_| Utc::now().timestamp().to_string());
        let sign = match (&timestamp, secret) {
            (Some(timestamp), Some(secret)) => Some(sign_request(timestamp, secret)),
            _ => None,
        };

        Self {
            timestamp,
            sign,
            msg_type: "interactive",
            card: FeishuLarkCard::from_signal(signal, computer_name),
        }
    }
}

#[derive(Debug, Serialize)]
struct FeishuLarkCard {
    config: FeishuLarkCardConfig,
    header: FeishuLarkCardHeader,
    elements: Vec<FeishuLarkCardElement>,
}

impl FeishuLarkCard {
    fn from_signal(signal: &Signal, computer_name: &str) -> Self {
        let detail_time = format_local_timestamp(signal.timestamp);
        let body = body_with_local_time(&signal.body, &detail_time);
        let card_body = FeishuLarkCardBody::from_body(&body);

        Self {
            config: FeishuLarkCardConfig {
                wide_screen_mode: true,
            },
            header: FeishuLarkCardHeader {
                template: CODEX_CARD_TEMPLATE,
                title: FeishuLarkPlainText {
                    tag: "plain_text",
                    content: format_header_title(computer_name, &signal.title, &card_body),
                },
            },
            elements: format_signal_card_elements(card_body),
        }
    }
}

#[derive(Debug, Serialize)]
struct FeishuLarkCardConfig {
    wide_screen_mode: bool,
}

#[derive(Debug, Serialize)]
struct FeishuLarkCardHeader {
    title: FeishuLarkPlainText,
    template: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct FeishuLarkPlainText {
    tag: &'static str,
    content: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct FeishuLarkLarkMarkdown {
    tag: &'static str,
    content: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "tag")]
enum FeishuLarkCardElement {
    #[serde(rename = "markdown")]
    Markdown { content: String },
    #[serde(rename = "div")]
    Div { text: FeishuLarkLarkMarkdown },
    #[serde(rename = "column_set")]
    ColumnSet {
        flex_mode: &'static str,
        background_style: &'static str,
        columns: Vec<FeishuLarkCardColumn>,
    },
    #[serde(rename = "hr")]
    Divider,
    #[serde(rename = "action")]
    Action { actions: Vec<FeishuLarkCardAction> },
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct FeishuLarkCardColumn {
    tag: &'static str,
    width: &'static str,
    weight: u8,
    vertical_align: &'static str,
    elements: Vec<FeishuLarkCardElement>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct FeishuLarkCardAction {
    tag: &'static str,
    text: FeishuLarkPlainText,
    url: String,
    #[serde(rename = "type")]
    button_type: &'static str,
}

#[derive(Debug, Deserialize)]
struct FeishuLarkResponse {
    code: i64,
    msg: Option<String>,
}

fn resolve_url(config: &ProviderConfig) -> anyhow::Result<String> {
    let url = match (
        present(config.url.as_deref()),
        present(config.url_env.as_deref()),
    ) {
        (Some(url), None) => url.to_string(),
        (None, Some(env_name)) => std::env::var(env_name).with_context(|| {
            format!(
                "feishu_lark provider `{}` webhook URL env var is not set",
                config.id
            )
        })?,
        _ => {
            return Err(anyhow!(
                "feishu_lark provider `{}` must set exactly one of `url` or `url_env`",
                config.id
            ));
        }
    };

    if url.trim().is_empty() {
        return Err(anyhow!(
            "feishu_lark provider `{}` webhook URL is empty",
            config.id
        ));
    }

    Ok(url)
}

fn resolve_secret(config: &ProviderConfig) -> anyhow::Result<Option<String>> {
    let secret = match (
        present(config.secret.as_deref()),
        present(config.secret_env.as_deref()),
    ) {
        (Some(secret), None) => Some(secret.to_string()),
        (None, Some(env_name)) => Some(std::env::var(env_name).with_context(|| {
            format!(
                "feishu_lark provider `{}` secret env var is not set",
                config.id
            )
        })?),
        (None, None) => None,
        (Some(_), Some(_)) => {
            return Err(anyhow!(
                "feishu_lark provider `{}` must set at most one of `secret` or `secret_env`",
                config.id
            ));
        }
    };

    if secret
        .as_ref()
        .is_some_and(|secret| secret.trim().is_empty())
    {
        return Err(anyhow!(
            "feishu_lark provider `{}` secret is empty",
            config.id
        ));
    }

    Ok(secret)
}

fn sign_request(timestamp: &str, secret: &str) -> String {
    let string_to_sign = format!("{timestamp}\n{secret}");
    let mac =
        HmacSha256::new_from_slice(string_to_sign.as_bytes()).expect("HMAC accepts any key size");
    STANDARD.encode(mac.finalize().into_bytes())
}

fn format_signal_card_elements(card_body: FeishuLarkCardBody) -> Vec<FeishuLarkCardElement> {
    let mut elements = Vec::new();

    if let Some(title) = card_body.display_title() {
        elements.push(FeishuLarkCardElement::Div {
            text: FeishuLarkLarkMarkdown {
                tag: "lark_md",
                content: format!("**{title}**"),
            },
        });
    }
    if let Some(project_row) = card_body.project_row() {
        elements.push(project_row);
    }
    if let Some(timing_row) = card_body.timing_row() {
        elements.push(timing_row);
    }
    if !card_body.other_details.is_empty() {
        elements.push(FeishuLarkCardElement::Markdown {
            content: card_body.other_details.join("\n"),
        });
    }

    if let Some(open_url) = card_body.open_url {
        elements.push(FeishuLarkCardElement::Action {
            actions: vec![FeishuLarkCardAction {
                tag: "button",
                text: FeishuLarkPlainText {
                    tag: "plain_text",
                    content: "Open in Codex".to_string(),
                },
                url: open_url,
                button_type: "primary",
            }],
        });
    }

    if let Some(answer) = card_body.answer {
        elements.push(FeishuLarkCardElement::Divider);
        elements.push(FeishuLarkCardElement::Markdown {
            content: format!("**{}**\n{}", answer.label, answer.content),
        });
    }

    elements
}

#[cfg(test)]
fn format_signal_card_elements_with_time(
    signal: &Signal,
    formatted_time: &str,
) -> Vec<FeishuLarkCardElement> {
    let body = body_with_local_time(&signal.body, formatted_time);
    format_signal_card_elements(FeishuLarkCardBody::from_body(&body))
}

fn format_header_title(computer_name: &str, title: &str, card_body: &FeishuLarkCardBody) -> String {
    let computer_name = computer_name.trim();
    let mut parts = Vec::new();
    parts.push(title.to_string());

    if let Some(session) = &card_body.session {
        parts.push(session.clone());
    }
    if !computer_name.is_empty() {
        parts.push(computer_name.to_string());
    }

    parts.join(" · ")
}

#[derive(Debug, PartialEq, Eq)]
struct FeishuLarkCardBody {
    project: Option<String>,
    project_path: Option<String>,
    session: Option<String>,
    duration: Option<String>,
    branch: Option<String>,
    time: Option<String>,
    other_details: Vec<String>,
    open_url: Option<String>,
    answer: Option<FeishuLarkAnswerBlock>,
}

#[derive(Debug, PartialEq, Eq)]
struct FeishuLarkAnswerBlock {
    label: &'static str,
    content: String,
}

impl FeishuLarkCardBody {
    fn from_body(body: &str) -> Self {
        let (body, answer) = split_trailing_answer_block(body)
            .map(|(details, block)| {
                (
                    details,
                    Some(FeishuLarkAnswerBlock {
                        label: block.label,
                        content: block.content.trim().to_string(),
                    }),
                )
            })
            .unwrap_or((body, None));
        let mut project = None;
        let mut project_path = None;
        let mut session = None;
        let mut duration = None;
        let mut branch = None;
        let mut time = None;
        let mut other_details = Vec::new();
        let mut open_url = None;
        let mut answer = answer;

        for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if let Some(session_id) = codex_thread_line(line) {
                open_url = codex_thread_bridge_url(session_id);
            } else if let Some(preview_text) = line.strip_prefix("Preview:") {
                answer = Some(FeishuLarkAnswerBlock {
                    label: "Preview",
                    content: preview_text.trim().to_string(),
                });
            } else if let Some(answer_text) = line.strip_prefix("Answer:") {
                answer = Some(FeishuLarkAnswerBlock {
                    label: "Answer",
                    content: answer_text.trim().to_string(),
                });
            } else if let Some((label, value)) = split_detail_line(line) {
                match label {
                    "Project" => project = Some(value.to_string()),
                    "Project Path" => project_path = Some(value.to_string()),
                    "Session" => session = Some(value.to_string()),
                    "Duration" => duration = Some(value.to_string()),
                    "Branch" => branch = Some(value.to_string()),
                    "Time" => time = Some(value.to_string()),
                    _ => other_details.push(format_detail_line(label, value)),
                }
            } else {
                other_details.push(format!("**{line}**"));
            }
        }

        Self {
            project,
            project_path,
            session,
            duration,
            branch,
            time,
            other_details,
            open_url,
            answer,
        }
    }

    fn display_title(&self) -> Option<&str> {
        self.session.as_deref().or(self.project.as_deref())
    }

    fn project_row(&self) -> Option<FeishuLarkCardElement> {
        let project = self.project.as_deref()?;
        let branch = self.branch.as_deref()?;

        Some(column_set(vec![
            metric_column("Project Name", project, None),
            metric_column("Branch", branch, None),
        ]))
    }

    fn timing_row(&self) -> Option<FeishuLarkCardElement> {
        let duration = self.duration.as_deref()?;
        let time = self.time.as_deref()?;

        Some(column_set(vec![
            metric_column("Duration", duration, None),
            metric_column("Time", strip_numeric_timezone(time), None),
        ]))
    }
}

fn column_set(columns: Vec<FeishuLarkCardColumn>) -> FeishuLarkCardElement {
    FeishuLarkCardElement::ColumnSet {
        flex_mode: "bisect",
        background_style: "default",
        columns,
    }
}

fn metric_column(label: &str, value: &str, secondary_value: Option<&str>) -> FeishuLarkCardColumn {
    let mut content = format!("**{label}**\n{}", value.trim());
    if let Some(secondary_value) = present(secondary_value) {
        content.push('\n');
        content.push('`');
        content.push_str(secondary_value);
        content.push('`');
    }

    FeishuLarkCardColumn {
        tag: "column",
        width: "weighted",
        weight: 1,
        vertical_align: "top",
        elements: vec![FeishuLarkCardElement::Div {
            text: FeishuLarkLarkMarkdown {
                tag: "lark_md",
                content,
            },
        }],
    }
}

fn strip_numeric_timezone(value: &str) -> &str {
    let value = value.trim();
    let Some((timestamp, offset)) = value.rsplit_once(' ') else {
        return value;
    };

    let offset_bytes = offset.as_bytes();
    if offset_bytes.len() == 6
        && matches!(offset_bytes[0], b'+' | b'-')
        && offset_bytes[3] == b':'
        && offset_bytes[1..3].iter().all(u8::is_ascii_digit)
        && offset_bytes[4..6].iter().all(u8::is_ascii_digit)
    {
        timestamp
    } else {
        value
    }
}

fn split_detail_line(line: &str) -> Option<(&str, &str)> {
    let (label, value) = line.split_once(':')?;
    let label = label.trim();
    let value = value.trim();
    if label.is_empty() || value.is_empty() {
        return None;
    }

    Some((label, value))
}

fn format_detail_line(label: &str, value: &str) -> String {
    format!("**{}**\n{}", label.trim(), value.trim())
}

fn codex_thread_line(line: &str) -> Option<&str> {
    let (_, session_id) = line.split_once("codex://threads/")?;
    Some(session_id.trim())
}

fn format_local_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %:z")
        .to_string()
}

fn present(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
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
                answer: Some(FeishuLarkAnswerBlock {
                    label: "Answer",
                    content: "Fixed the route.\n\nRun tests next.".to_string(),
                }),
            }
        );
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

        let request =
            FeishuLarkInteractiveRequest::from_signal(&signal, None, "Felix’s MacBook Pro");
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
}
