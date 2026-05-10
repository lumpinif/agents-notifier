use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Config {
    pub schema_version: u32,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub notification: NotificationConfig,
    pub sources: Vec<SourceConfig>,
    pub providers: Vec<ProviderConfig>,
    pub routes: Vec<RouteConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
        }
    }
}

fn default_log_level() -> String {
    "info".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct NotificationConfig {
    #[serde(default)]
    pub answer_detail: AnswerDetail,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            answer_detail: AnswerDetail::Preview,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnswerDetail {
    #[default]
    Preview,
    Full,
}

impl AnswerDetail {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::Full => "Full Answer",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SourceConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub source_type: SourceType,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    AgentsNotifier,
    CodexDesktop,
    CodexCli,
    ClaudeCode,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentsNotifier => "agents_notifier",
            Self::CodexDesktop => "codex_desktop",
            Self::CodexCli => "codex_cli",
            Self::ClaudeCode => "claude_code",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProviderConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub provider_type: ProviderType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret_env: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Ntfy,
    Webhook,
    FeishuLark,
}

impl ProviderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ntfy => "ntfy",
            Self::Webhook => "webhook",
            Self::FeishuLark => "feishu_lark",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RouteConfig {
    pub sources: Vec<String>,
    pub providers: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found at {path}")]
    NotFound { path: String },
    #[error("failed to read config at {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("unsupported config schema_version {found}; expected {expected}")]
    UnsupportedSchema { found: u32, expected: u32 },
    #[error("duplicate source id `{0}`")]
    DuplicateSourceId(String),
    #[error("duplicate provider id `{0}`")]
    DuplicateProviderId(String),
    #[error("route references unknown source `{0}`")]
    UnknownRouteSource(String),
    #[error("route references unknown provider `{0}`")]
    UnknownRouteProvider(String),
    #[error("provider `{provider_id}` requires `{field}`")]
    MissingProviderField {
        provider_id: String,
        field: &'static str,
    },
    #[error("webhook provider `{provider_id}` must set exactly one of `url` or `url_env`")]
    InvalidWebhookUrlSource { provider_id: String },
    #[error("feishu_lark provider `{provider_id}` must set exactly one of `url` or `url_env`")]
    InvalidFeishuLarkUrlSource { provider_id: String },
    #[error(
        "feishu_lark provider `{provider_id}` must set at most one of `secret` or `secret_env`"
    )]
    InvalidFeishuLarkSecretSource { provider_id: String },
}

impl Config {
    pub fn from_path(path: &Path) -> Result<Self, ConfigError> {
        let raw = fs::read_to_string(path).map_err(|source| {
            if source.kind() == ErrorKind::NotFound {
                ConfigError::NotFound {
                    path: path.display().to_string(),
                }
            } else {
                ConfigError::Read {
                    path: path.display().to_string(),
                    source,
                }
            }
        })?;

        Self::from_toml_str(&raw)
    }

    pub fn from_toml_str(raw: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(raw)?;
        config.validate()?;
        Ok(config)
    }

    pub fn source(&self, id: &str) -> Option<&SourceConfig> {
        self.sources.iter().find(|source| source.id == id)
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|provider| provider.id == id)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchema {
                found: self.schema_version,
                expected: CONFIG_SCHEMA_VERSION,
            });
        }

        let mut source_ids = HashSet::new();
        for source in &self.sources {
            if !source_ids.insert(source.id.as_str()) {
                return Err(ConfigError::DuplicateSourceId(source.id.clone()));
            }
        }

        let mut provider_ids = HashSet::new();
        for provider in &self.providers {
            if !provider_ids.insert(provider.id.as_str()) {
                return Err(ConfigError::DuplicateProviderId(provider.id.clone()));
            }
            provider.validate()?;
        }

        for route in &self.routes {
            for source_id in &route.sources {
                if !source_ids.contains(source_id.as_str()) {
                    return Err(ConfigError::UnknownRouteSource(source_id.clone()));
                }
            }

            for provider_id in &route.providers {
                if !provider_ids.contains(provider_id.as_str()) {
                    return Err(ConfigError::UnknownRouteProvider(provider_id.clone()));
                }
            }
        }

        Ok(())
    }
}

impl ProviderConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        match self.provider_type {
            ProviderType::Ntfy => {
                self.require_present("server", self.server.as_deref())?;
                self.require_present("topic", self.topic.as_deref())?;
            }
            ProviderType::Webhook => {
                let has_url = is_present(self.url.as_deref());
                let has_url_env = is_present(self.url_env.as_deref());
                if has_url == has_url_env {
                    return Err(ConfigError::InvalidWebhookUrlSource {
                        provider_id: self.id.clone(),
                    });
                }
            }
            ProviderType::FeishuLark => {
                let has_url = is_present(self.url.as_deref());
                let has_url_env = is_present(self.url_env.as_deref());
                if has_url == has_url_env {
                    return Err(ConfigError::InvalidFeishuLarkUrlSource {
                        provider_id: self.id.clone(),
                    });
                }

                let has_secret = is_present(self.secret.as_deref());
                let has_secret_env = is_present(self.secret_env.as_deref());
                if has_secret && has_secret_env {
                    return Err(ConfigError::InvalidFeishuLarkSecretSource {
                        provider_id: self.id.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    fn require_present(&self, field: &'static str, value: Option<&str>) -> Result<(), ConfigError> {
        if is_present(value) {
            Ok(())
        } else {
            Err(ConfigError::MissingProviderField {
                provider_id: self.id.clone(),
                field,
            })
        }
    }
}

fn is_present(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CONFIG: &str = r#"
schema_version = 1

[[sources]]
id = "codex_desktop"
type = "codex_desktop"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "phone"
type = "ntfy"
server = "https://ntfy.sh"
topic = "my-codex-alerts"

[[providers]]
id = "debug_webhook"
type = "webhook"
url_env = "AGENTS_NOTIFIER_WEBHOOK_URL"

[[providers]]
id = "work_chat"
type = "feishu_lark"
url_env = "AGENTS_NOTIFIER_FEISHU_LARK_WEBHOOK_URL"
secret_env = "AGENTS_NOTIFIER_FEISHU_LARK_SECRET"

[[routes]]
sources = ["codex_desktop", "codex_cli"]
providers = ["phone", "debug_webhook", "work_chat"]
"#;

    #[test]
    fn parses_valid_config() {
        let config = Config::from_toml_str(VALID_CONFIG).expect("valid config should parse");

        assert_eq!(config.schema_version, 1);
        assert_eq!(config.sources.len(), 2);
        assert_eq!(config.providers.len(), 3);
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.log.level, "info");
        assert_eq!(config.notification.answer_detail, AnswerDetail::Preview);
    }

    #[test]
    fn parses_full_answer_detail() {
        let raw = VALID_CONFIG.replacen(
            "[[sources]]",
            "[notification]\nanswer_detail = \"full\"\n\n[[sources]]",
            1,
        );

        let config = Config::from_toml_str(&raw).expect("valid config should parse");

        assert_eq!(config.notification.answer_detail, AnswerDetail::Full);
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let err = Config::from_toml_str(
            &VALID_CONFIG.replace("schema_version = 1", "schema_version = 2"),
        )
        .expect_err("unsupported schema should fail");

        assert!(matches!(
            err,
            ConfigError::UnsupportedSchema { found: 2, .. }
        ));
    }

    #[test]
    fn rejects_duplicate_source_id() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[sources]]
id = "codex_cli"
type = "codex_desktop"

[[providers]]
id = "phone"
type = "ntfy"
server = "https://ntfy.sh"
topic = "topic"

[[routes]]
sources = ["codex_cli"]
providers = ["phone"]
"#;

        let err = Config::from_toml_str(raw).expect_err("duplicate source id should fail");

        assert!(matches!(err, ConfigError::DuplicateSourceId(id) if id == "codex_cli"));
    }

    #[test]
    fn rejects_unknown_route_source() {
        let err =
            Config::from_toml_str(&VALID_CONFIG.replace("codex_desktop\", \"codex_cli", "missing"))
                .expect_err("unknown source should fail");

        assert!(matches!(err, ConfigError::UnknownRouteSource(id) if id == "missing"));
    }

    #[test]
    fn rejects_unknown_route_provider() {
        let err =
            Config::from_toml_str(&VALID_CONFIG.replace("phone\", \"debug_webhook", "missing"))
                .expect_err("unknown provider should fail");

        assert!(matches!(err, ConfigError::UnknownRouteProvider(id) if id == "missing"));
    }

    #[test]
    fn rejects_webhook_with_both_url_sources() {
        let raw = VALID_CONFIG.replace(
            "url_env = \"AGENTS_NOTIFIER_WEBHOOK_URL\"",
            "url = \"https://example.com/hook\"\nurl_env = \"AGENTS_NOTIFIER_WEBHOOK_URL\"",
        );

        let err = Config::from_toml_str(&raw).expect_err("ambiguous webhook url should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidWebhookUrlSource { provider_id } if provider_id == "debug_webhook"
        ));
    }

    #[test]
    fn rejects_feishu_lark_without_webhook_url() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "work_chat"
type = "feishu_lark"

[[routes]]
sources = ["codex_cli"]
providers = ["work_chat"]
"#;

        let err = Config::from_toml_str(raw).expect_err("missing webhook URL should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidFeishuLarkUrlSource { provider_id } if provider_id == "work_chat"
        ));
    }

    #[test]
    fn rejects_feishu_lark_with_both_secret_sources() {
        let raw = VALID_CONFIG.replace(
            "secret_env = \"AGENTS_NOTIFIER_FEISHU_LARK_SECRET\"",
            "secret = \"inline-secret\"\nsecret_env = \"AGENTS_NOTIFIER_FEISHU_LARK_SECRET\"",
        );

        let err = Config::from_toml_str(&raw).expect_err("ambiguous secret source should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidFeishuLarkSecretSource { provider_id } if provider_id == "work_chat"
        ));
    }
}
