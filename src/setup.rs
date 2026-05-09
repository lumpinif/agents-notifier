use std::fs;
use std::path::Path;

use anyhow::Context;
use uuid::Uuid;

use crate::config::{
    CONFIG_SCHEMA_VERSION, Config, LogConfig, ProviderConfig, ProviderType, RouteConfig,
    SourceConfig, SourceType,
};

const DEFAULT_NTFY_SERVER: &str = "https://ntfy.sh";
const FEISHU_WEBHOOK_PREFIX: &str = "https://open.feishu.cn/open-apis/bot/v2/hook/";
const LARK_WEBHOOK_PREFIX: &str = "https://open.larksuite.com/open-apis/bot/v2/hook/";
pub const TEST_NOTIFICATION_SKIPPED_MESSAGE: &str =
    "Service is running. No test notification was sent.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfySubscription {
    pub provider_id: String,
    pub server: String,
    pub topic: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeishuLarkTarget {
    pub provider_id: String,
    pub webhook_host: String,
    pub signed: bool,
}

pub fn generated_ntfy_topic() -> String {
    let id = Uuid::new_v4().simple().to_string();
    format!("agents-notifier-{}", &id[..12])
}

pub fn resolve_ntfy_topic(input: &str, generated_topic: &str) -> anyhow::Result<String> {
    let topic = input.trim();
    let topic = if topic.is_empty() {
        generated_topic
    } else {
        topic
    };

    if !is_valid_ntfy_topic(topic) {
        anyhow::bail!("topic must only contain letters, numbers, hyphens, and underscores");
    }

    Ok(topic.to_string())
}

pub fn resolve_feishu_lark_webhook_url(input: &str) -> anyhow::Result<String> {
    let url = input.trim();
    if url.is_empty() {
        anyhow::bail!("webhook URL is required");
    }

    if !url.starts_with(FEISHU_WEBHOOK_PREFIX) && !url.starts_with(LARK_WEBHOOK_PREFIX) {
        anyhow::bail!(
            "webhook URL must start with `https://open.feishu.cn/open-apis/bot/v2/hook/` or `https://open.larksuite.com/open-apis/bot/v2/hook/`"
        );
    }

    Ok(url.to_string())
}

pub fn resolve_feishu_lark_secret(input: &str) -> Option<String> {
    let secret = input.trim();
    if secret.is_empty() {
        None
    } else {
        Some(secret.to_string())
    }
}

pub fn build_ntfy_config(topic: &str) -> Config {
    build_config(vec![ProviderConfig {
        id: "phone".to_string(),
        provider_type: ProviderType::Ntfy,
        server: Some(DEFAULT_NTFY_SERVER.to_string()),
        topic: Some(topic.to_string()),
        url: None,
        url_env: None,
        secret: None,
        secret_env: None,
    }])
}

pub fn build_feishu_lark_config(webhook_url: &str, secret: Option<String>) -> Config {
    build_config(vec![ProviderConfig {
        id: "work_chat".to_string(),
        provider_type: ProviderType::FeishuLark,
        server: None,
        topic: None,
        url: Some(webhook_url.to_string()),
        url_env: None,
        secret,
        secret_env: None,
    }])
}

fn build_config(providers: Vec<ProviderConfig>) -> Config {
    let provider_ids = providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect();

    Config {
        schema_version: CONFIG_SCHEMA_VERSION,
        log: LogConfig::default(),
        sources: vec![
            SourceConfig {
                id: "codex_desktop".to_string(),
                source_type: SourceType::CodexDesktop,
            },
            SourceConfig {
                id: "codex_cli".to_string(),
                source_type: SourceType::CodexCli,
            },
        ],
        providers,
        routes: vec![RouteConfig {
            sources: vec!["codex_desktop".to_string(), "codex_cli".to_string()],
            providers: provider_ids,
        }],
    }
}

pub fn write_config(path: &Path, config: &Config) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory `{}`", parent.display()))?;
    }

    let raw = toml::to_string_pretty(config).context("failed to serialize config")?;
    fs::write(path, raw).with_context(|| format!("failed to write config `{}`", path.display()))
}

pub fn ntfy_subscriptions(config: &Config) -> Vec<NtfySubscription> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::Ntfy)
        .filter_map(|provider| {
            Some(NtfySubscription {
                provider_id: provider.id.clone(),
                server: provider.server.as_ref()?.clone(),
                topic: provider.topic.as_ref()?.clone(),
            })
        })
        .collect()
}

pub fn feishu_lark_targets(config: &Config) -> Vec<FeishuLarkTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::FeishuLark)
        .filter_map(|provider| {
            let webhook_host = match (
                provider
                    .url
                    .as_deref()
                    .filter(|value| !value.trim().is_empty()),
                provider
                    .url_env
                    .as_deref()
                    .filter(|value| !value.trim().is_empty()),
            ) {
                (Some(url), _) => webhook_host(url),
                (None, Some(env_name)) => format!("env:{env_name}"),
                (None, None) => return None,
            };

            Some(FeishuLarkTarget {
                provider_id: provider.id.clone(),
                webhook_host,
                signed: provider
                    .secret
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                    || provider
                        .secret_env
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty()),
            })
        })
        .collect()
}

pub fn missing_config_message(path: &str) -> String {
    format!(
        r#"No agents-notifier config found at `{path}`.

Run `agents-notifier start` in an interactive terminal to set up a notification provider.

If you are running non-interactively, create the config file first or pass `--config <PATH>`."#
    )
}

fn is_valid_ntfy_topic(topic: &str) -> bool {
    !topic.is_empty()
        && topic
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn webhook_host(url: &str) -> String {
    let without_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    without_scheme
        .split('/')
        .next()
        .filter(|host| !host.trim().is_empty())
        .unwrap_or("configured")
        .to_string()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn empty_topic_input_uses_generated_topic() {
        let topic = resolve_ntfy_topic("  ", "agents-notifier-test")
            .expect("empty input should use generated topic");

        assert_eq!(topic, "agents-notifier-test");
    }

    #[test]
    fn rejects_topic_with_path_separator() {
        let err = resolve_ntfy_topic("bad/topic", "agents-notifier-test")
            .expect_err("path separators should be rejected");

        assert!(err.to_string().contains("letters, numbers"));
    }

    #[test]
    fn writes_parseable_ntfy_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_ntfy_config("agents-notifier-test");

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("phone")
                .and_then(|provider| provider.topic.as_deref()),
            Some("agents-notifier-test")
        );
        assert!(parsed.source("codex_desktop").is_some());
        assert!(parsed.source("codex_cli").is_some());
    }

    #[test]
    fn writes_parseable_feishu_lark_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_feishu_lark_config(
            "https://open.larksuite.com/open-apis/bot/v2/hook/test",
            Some("secret".to_string()),
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("work_chat")
                .and_then(|provider| provider.url.as_deref()),
            Some("https://open.larksuite.com/open-apis/bot/v2/hook/test")
        );
        assert_eq!(
            parsed
                .provider("work_chat")
                .and_then(|provider| provider.secret.as_deref()),
            Some("secret")
        );
        assert!(parsed.source("codex_desktop").is_some());
        assert!(parsed.source("codex_cli").is_some());
    }

    #[test]
    fn accepts_feishu_and_lark_webhook_urls() {
        assert_eq!(
            resolve_feishu_lark_webhook_url("https://open.feishu.cn/open-apis/bot/v2/hook/abc")
                .expect("Feishu webhook should be valid"),
            "https://open.feishu.cn/open-apis/bot/v2/hook/abc"
        );
        assert_eq!(
            resolve_feishu_lark_webhook_url("https://open.larksuite.com/open-apis/bot/v2/hook/abc")
                .expect("Lark webhook should be valid"),
            "https://open.larksuite.com/open-apis/bot/v2/hook/abc"
        );
    }

    #[test]
    fn rejects_non_custom_bot_webhook_url() {
        let err = resolve_feishu_lark_webhook_url("https://example.com/hook")
            .expect_err("wrong URL should fail");

        assert!(err.to_string().contains("webhook URL must start"));
    }

    #[test]
    fn extracts_ntfy_subscriptions_from_config() {
        let config = build_ntfy_config("agents-notifier-test");

        let subscriptions = ntfy_subscriptions(&config);

        assert_eq!(
            subscriptions,
            vec![NtfySubscription {
                provider_id: "phone".to_string(),
                server: "https://ntfy.sh".to_string(),
                topic: "agents-notifier-test".to_string(),
            }]
        );
    }

    #[test]
    fn extracts_feishu_lark_targets_without_printing_webhook_token() {
        let config = build_feishu_lark_config(
            "https://open.larksuite.com/open-apis/bot/v2/hook/secret-token",
            Some("secret".to_string()),
        );

        let targets = feishu_lark_targets(&config);

        assert_eq!(
            targets,
            vec![FeishuLarkTarget {
                provider_id: "work_chat".to_string(),
                webhook_host: "open.larksuite.com".to_string(),
                signed: true,
            }]
        );
    }

    #[test]
    fn skipped_test_notification_message_confirms_running_service() {
        assert!(TEST_NOTIFICATION_SKIPPED_MESSAGE.contains("Service is running"));
        assert!(TEST_NOTIFICATION_SKIPPED_MESSAGE.contains("No test notification was sent"));
    }
}
