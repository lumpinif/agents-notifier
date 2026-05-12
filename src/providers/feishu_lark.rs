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
use crate::providers::formatting::format_duration_ms;
use crate::providers::http::provider_http_client;
use crate::router::{Provider, ProviderFuture};
use crate::signal::{Signal, SignalAnswerKind, SignalLink};

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
        let card_body = FeishuLarkCardBody::from_signal(signal, &detail_time);

        Self {
            config: FeishuLarkCardConfig {
                wide_screen_mode: true,
            },
            header: FeishuLarkCardHeader {
                template: CODEX_CARD_TEMPLATE,
                title: FeishuLarkPlainText {
                    tag: "plain_text",
                    content: format_header_title(computer_name, signal.title(), &card_body),
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

    if let Some(open_link) = card_body.open_link {
        elements.push(FeishuLarkCardElement::Action {
            actions: vec![FeishuLarkCardAction {
                tag: "button",
                text: FeishuLarkPlainText {
                    tag: "plain_text",
                    content: open_link.label,
                },
                url: open_link.url,
                button_type: "primary",
            }],
        });
    }

    let has_prompt = card_body.prompt.is_some();
    let has_answer = card_body.answer.is_some();
    if has_prompt || has_answer {
        elements.push(FeishuLarkCardElement::Divider);
    }

    if let Some(prompt) = card_body.prompt {
        elements.push(FeishuLarkCardElement::Markdown {
            content: format!("**Prompt**\n{prompt}"),
        });
    }

    if let Some(answer) = card_body.answer {
        if has_prompt {
            elements.push(FeishuLarkCardElement::Divider);
        }
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
    format_signal_card_elements(FeishuLarkCardBody::from_signal(signal, formatted_time))
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
    model: Option<String>,
    duration: Option<String>,
    branch: Option<String>,
    time: Option<String>,
    other_details: Vec<String>,
    open_link: Option<FeishuLarkActionLink>,
    prompt: Option<String>,
    answer: Option<FeishuLarkAnswerBlock>,
}

#[derive(Debug, PartialEq, Eq)]
struct FeishuLarkActionLink {
    label: String,
    url: String,
}

#[derive(Debug, PartialEq, Eq)]
struct FeishuLarkAnswerBlock {
    label: &'static str,
    content: String,
}

impl FeishuLarkCardBody {
    fn from_signal(signal: &Signal, formatted_time: &str) -> Self {
        let mut other_details = Vec::new();

        if !has_structured_card_content(signal) {
            push_markdown_detail(&mut other_details, signal.summary());
        }

        let workspace = signal.workspace.as_ref();
        let conversation = signal.conversation.as_ref();
        let lifecycle = signal.lifecycle.as_ref();

        Self {
            project: workspace.and_then(|workspace| workspace.project_name.clone()),
            project_path: workspace.and_then(|workspace| workspace.project_path.clone()),
            session: conversation.and_then(|conversation| {
                present(conversation.session_title.as_deref())
                    .or_else(|| present(conversation.session_id.as_deref()))
                    .map(ToOwned::to_owned)
            }),
            model: conversation.and_then(|conversation| conversation.model.clone()),
            duration: lifecycle
                .and_then(|lifecycle| lifecycle.duration_ms)
                .map(format_duration_ms),
            branch: workspace.and_then(|workspace| workspace.branch.clone()),
            time: Some(formatted_time.to_string()),
            other_details,
            open_link: signal.links.iter().find_map(feishu_action_link),
            prompt: conversation.and_then(|conversation| conversation.prompt.clone()),
            answer: conversation
                .and_then(|conversation| conversation.answer.as_ref())
                .map(|answer| FeishuLarkAnswerBlock {
                    label: answer_label(answer.kind),
                    content: answer.content.trim().to_string(),
                }),
        }
    }

    fn display_title(&self) -> Option<&str> {
        self.session.as_deref().or(self.project.as_deref())
    }

    fn project_row(&self) -> Option<FeishuLarkCardElement> {
        let mut columns = Vec::new();

        if let Some(project) = self.project.as_deref() {
            columns.push(metric_column("Project Name", project, None));
        }
        if let Some(branch) = self.branch.as_deref() {
            columns.push(metric_column("Branch", branch, None));
        }
        if let Some(model) = self.model.as_deref() {
            columns.push(metric_column("Model", model, None));
        }

        if columns.is_empty() {
            None
        } else {
            Some(column_set(columns))
        }
    }

    fn timing_row(&self) -> Option<FeishuLarkCardElement> {
        let mut columns = Vec::new();

        if let Some(duration) = self.duration.as_deref() {
            columns.push(metric_column("Duration", duration, None));
        }
        if let Some(time) = self.time.as_deref() {
            columns.push(metric_column("Time", strip_numeric_timezone(time), None));
        }

        if columns.is_empty() {
            None
        } else {
            Some(column_set(columns))
        }
    }
}

fn has_structured_card_content(signal: &Signal) -> bool {
    signal.workspace.is_some()
        || signal.conversation.as_ref().is_some_and(|conversation| {
            conversation.session_title.is_some()
                || conversation.session_id.is_some()
                || conversation.prompt.is_some()
                || conversation.answer.is_some()
                || conversation.model.is_some()
        })
        || signal.lifecycle.is_some()
        || !signal.links.is_empty()
}

fn push_markdown_detail(details: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        details.push(format!("**{value}**"));
    }
}

fn answer_label(kind: SignalAnswerKind) -> &'static str {
    kind.display_label()
}

fn feishu_action_link(link: &SignalLink) -> Option<FeishuLarkActionLink> {
    let label = link.label.trim();
    if label.is_empty() {
        return None;
    }

    let url = feishu_action_url(link)?;
    Some(FeishuLarkActionLink {
        label: label.to_string(),
        url,
    })
}

fn feishu_action_url(link: &SignalLink) -> Option<String> {
    let url = link.url.trim();
    if url.is_empty() {
        return None;
    }

    if let Some(session_id) = url.strip_prefix("codex://threads/") {
        return codex_thread_bridge_url(session_id);
    }

    Some(url.to_string())
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
mod tests;
