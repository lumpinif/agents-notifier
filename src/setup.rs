use std::fs;
use std::path::Path;

use anyhow::Context;
use uuid::Uuid;

use crate::config::{
    CONFIG_SCHEMA_VERSION, Config, LogConfig, ProviderConfig, ProviderType, RouteConfig,
    SourceConfig, SourceType,
};

const DEFAULT_NTFY_SERVER: &str = "https://ntfy.sh";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfySubscription {
    pub provider_id: String,
    pub server: String,
    pub topic: String,
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

pub fn build_ntfy_config(topic: &str) -> Config {
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
        providers: vec![ProviderConfig {
            id: "phone".to_string(),
            provider_type: ProviderType::Ntfy,
            server: Some(DEFAULT_NTFY_SERVER.to_string()),
            topic: Some(topic.to_string()),
            url: None,
            url_env: None,
        }],
        routes: vec![RouteConfig {
            sources: vec!["codex_desktop".to_string(), "codex_cli".to_string()],
            providers: vec!["phone".to_string()],
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

pub fn missing_config_message(path: &str) -> String {
    format!(
        r#"No agents-notifier config found at `{path}`.

Run `agents-notifier start` in an interactive terminal to set up ntfy.

If you are running non-interactively, create the config file first or pass `--config <PATH>`."#
    )
}

fn is_valid_ntfy_topic(topic: &str) -> bool {
    !topic.is_empty()
        && topic
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
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
}
