use anyhow::{Context, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Local, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::config::{ProviderConfig, ProviderType};
use crate::router::{Provider, ProviderFuture};
use crate::signal::Signal;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug)]
pub struct FeishuLarkProvider {
    id: String,
    url: String,
    secret: Option<String>,
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

        Ok(Self {
            id: config.id.clone(),
            url,
            secret,
            client: reqwest::Client::new(),
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
            let request = FeishuLarkTextRequest::from_signal(signal, self.secret.as_deref());
            let response = self
                .client
                .post(&self.url)
                .json(&request)
                .send()
                .await
                .map_err(reqwest::Error::without_url)
                .with_context(|| format!("feishu_lark provider `{}` request failed", self.id))?;

            let status = response.status();
            if !status.is_success() {
                return Err(anyhow!(
                    "feishu_lark provider `{}` returned HTTP status {}",
                    self.id,
                    status
                ));
            }

            let response_body = response.text().await.with_context(|| {
                format!("feishu_lark provider `{}` failed to read response", self.id)
            })?;
            let provider_response: FeishuLarkResponse = serde_json::from_str(&response_body)
                .with_context(|| {
                    format!(
                        "feishu_lark provider `{}` returned invalid response JSON",
                        self.id
                    )
                })?;

            if provider_response.code != 0 {
                return Err(anyhow!(
                    "feishu_lark provider `{}` returned code {}: {}",
                    self.id,
                    provider_response.code,
                    provider_response
                        .msg
                        .unwrap_or_else(|| "unknown error".to_string())
                ));
            }

            Ok(())
        })
    }
}

#[derive(Debug, Serialize)]
struct FeishuLarkTextRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sign: Option<String>,
    msg_type: &'static str,
    content: FeishuLarkTextContent,
}

impl FeishuLarkTextRequest {
    fn from_signal(signal: &Signal, secret: Option<&str>) -> Self {
        let timestamp = secret.map(|_| Utc::now().timestamp().to_string());
        let sign = match (&timestamp, secret) {
            (Some(timestamp), Some(secret)) => Some(sign_request(timestamp, secret)),
            _ => None,
        };

        Self {
            timestamp,
            sign,
            msg_type: "text",
            content: FeishuLarkTextContent {
                text: format_signal_text(signal),
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct FeishuLarkTextContent {
    text: String,
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

fn format_signal_text(signal: &Signal) -> String {
    format_signal_text_with_time(signal, &format_local_timestamp(signal.timestamp))
}

fn format_signal_text_with_time(signal: &Signal, formatted_time: &str) -> String {
    let body = signal.body.trim_end();
    if body.is_empty() {
        format!("{}\n\nTime: {}", signal.title, formatted_time)
    } else {
        format!("{}\n\n{}\nTime: {}", signal.title, body, formatted_time)
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
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn sends_text_message_to_custom_bot_webhook() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/bot/v2/hook/test"))
            .and(body_partial_json(json!({
                "msg_type": "text"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 0,
                "msg": "success",
                "data": {}
            })))
            .mount(&server)
            .await;

        let provider = FeishuLarkProvider::from_config(&ProviderConfig {
            id: "work_chat".to_string(),
            provider_type: ProviderType::FeishuLark,
            server: None,
            topic: None,
            url: Some(format!("{}/open-apis/bot/v2/hook/test", server.uri())),
            url_env: None,
            secret: None,
            secret_env: None,
        })
        .expect("provider config should be valid");

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
        let text = body["content"]["text"]
            .as_str()
            .expect("text content should be present");
        assert!(text.contains("Codex\n\nCodex finished a job."));
        assert!(text.contains("Time: "));
        assert!(!text.contains("UTC"));
    }

    #[tokio::test]
    async fn sends_signature_fields_when_secret_is_configured() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/open-apis/bot/v2/hook/test"))
            .and(body_partial_json(json!({
                "msg_type": "text",
                "content": {
                    "text": format_signal_text(&test_signal())
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "code": 0,
                "msg": "success",
                "data": {}
            })))
            .mount(&server)
            .await;

        let provider = FeishuLarkProvider::from_config(&ProviderConfig {
            id: "work_chat".to_string(),
            provider_type: ProviderType::FeishuLark,
            server: None,
            topic: None,
            url: Some(format!("{}/open-apis/bot/v2/hook/test", server.uri())),
            url_env: None,
            secret: Some("demo".to_string()),
            secret_env: None,
        })
        .expect("provider config should be valid");

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
    fn formats_message_with_display_source_and_supplied_time() {
        assert_eq!(
            format_signal_text_with_time(&test_signal(), "2026-05-08 20:00:00 +08:00"),
            "Codex\n\nCodex finished a job.\nTime: 2026-05-08 20:00:00 +08:00"
        );
    }

    #[test]
    fn formats_codex_desktop_message_without_separate_source_line() {
        let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_desktop",
            "codex_desktop",
            "Codex Desktop finished a job.",
            "Project: agents-notifier\nSession: agents-notifier sync report\nPreview: Updated the Codex Desktop default notification text...\nDuration: 1m 32s\nBranch: main",
            timestamp,
            BTreeMap::new(),
        );

        assert_eq!(
            format_signal_text_with_time(&signal, "2026-05-10 01:35:42 +08:00"),
            "Codex Desktop finished a job.\n\nProject: agents-notifier\nSession: agents-notifier sync report\nPreview: Updated the Codex Desktop default notification text...\nDuration: 1m 32s\nBranch: main\nTime: 2026-05-10 01:35:42 +08:00"
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

        let provider = FeishuLarkProvider::from_config(&ProviderConfig {
            id: "work_chat".to_string(),
            provider_type: ProviderType::FeishuLark,
            server: None,
            topic: None,
            url: Some(format!("{}/open-apis/bot/v2/hook/test", server.uri())),
            url_env: None,
            secret: Some("demo".to_string()),
            secret_env: None,
        })
        .expect("provider config should be valid");

        let err = provider
            .send(&test_signal())
            .await
            .expect_err("nonzero provider code should fail");

        assert!(err.to_string().contains("returned code 19021"));
        assert!(err.to_string().contains("sign match fail"));
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
            "Codex finished a job.",
            timestamp,
            BTreeMap::new(),
        )
    }
}
