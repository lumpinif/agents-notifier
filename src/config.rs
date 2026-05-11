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
    pub cli: CliConfig,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub notification: NotificationConfig,
    pub sources: Vec<SourceConfig>,
    pub providers: Vec<ProviderConfig>,
    pub routes: Vec<RouteConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CliConfig {
    #[serde(default)]
    pub language: CliLanguage,
}

impl CliConfig {
    pub fn new(language: CliLanguage) -> Self {
        Self { language }
    }
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            language: CliLanguage::English,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum CliLanguage {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-CN")]
    SimplifiedChinese,
}

impl CliLanguage {
    pub fn config_value(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        let normalized = input.trim().to_ascii_lowercase().replace('_', "-");
        match normalized.as_str() {
            "en" | "english" => Some(Self::English),
            "zh" | "zh-cn" | "zh-hans" | "cn" | "chinese" | "simplified-chinese" => {
                Some(Self::SimplifiedChinese)
            }
            _ if input.trim() == "简体中文" || input.trim() == "中文" => {
                Some(Self::SimplifiedChinese)
            }
            _ => None,
        }
    }
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
    #[serde(default)]
    pub prompt_detail: PromptDetail,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            answer_detail: AnswerDetail::Preview,
            prompt_detail: PromptDetail::Off,
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

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptDetail {
    #[default]
    Off,
    On,
}

impl PromptDetail {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::On => "On",
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
    AgentHook,
    CodexDesktop,
    CodexCli,
    ClaudeCode,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentsNotifier => "agents_notifier",
            Self::AgentHook => "agent_hook",
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
    pub base_url: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_token_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_key_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_token_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_phone_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<EmailSmtpSecurity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_tag: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmailSmtpSecurity {
    Starttls,
    ImplicitTls,
}

impl EmailSmtpSecurity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starttls => "starttls",
            Self::ImplicitTls => "implicit_tls",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Ntfy,
    Webhook,
    FeishuLark,
    Pushover,
    Slack,
    Discord,
    Telegram,
    Whatsapp,
    Weixin,
    MicrosoftTeams,
    EmailSmtp,
}

impl ProviderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ntfy => "ntfy",
            Self::Webhook => "webhook",
            Self::FeishuLark => "feishu_lark",
            Self::Pushover => "pushover",
            Self::Slack => "slack",
            Self::Discord => "discord",
            Self::Telegram => "telegram",
            Self::Whatsapp => "whatsapp",
            Self::Weixin => "weixin",
            Self::MicrosoftTeams => "microsoft_teams",
            Self::EmailSmtp => "email_smtp",
        }
    }

    pub fn has_message_size_limit(&self) -> bool {
        match self {
            Self::Ntfy
            | Self::Pushover
            | Self::Slack
            | Self::Discord
            | Self::Telegram
            | Self::Whatsapp
            | Self::Weixin
            | Self::MicrosoftTeams => true,
            Self::Webhook | Self::FeishuLark | Self::EmailSmtp => false,
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
    #[error(
        "pushover provider `{provider_id}` must set exactly one of `app_token` or `app_token_env`"
    )]
    InvalidPushoverAppTokenSource { provider_id: String },
    #[error(
        "pushover provider `{provider_id}` must set exactly one of `user_key` or `user_key_env`"
    )]
    InvalidPushoverUserKeySource { provider_id: String },
    #[error("slack provider `{provider_id}` must set exactly one of `url` or `url_env`")]
    InvalidSlackUrlSource { provider_id: String },
    #[error("discord provider `{provider_id}` must set exactly one of `url` or `url_env`")]
    InvalidDiscordUrlSource { provider_id: String },
    #[error(
        "telegram provider `{provider_id}` must set exactly one of `bot_token` or `bot_token_env`"
    )]
    InvalidTelegramBotTokenSource { provider_id: String },
    #[error(
        "whatsapp provider `{provider_id}` must set exactly one of `access_token` or `access_token_env`"
    )]
    InvalidWhatsappAccessTokenSource { provider_id: String },
    #[error("weixin provider `{provider_id}` must set exactly one of `token` or `token_env`")]
    InvalidWeixinTokenSource { provider_id: String },
    #[error(
        "weixin provider `{provider_id}` must set exactly one of `context_token` or `context_token_env`"
    )]
    InvalidWeixinContextTokenSource { provider_id: String },
    #[error("microsoft_teams provider `{provider_id}` must set exactly one of `url` or `url_env`")]
    InvalidMicrosoftTeamsUrlSource { provider_id: String },
    #[error(
        "email_smtp provider `{provider_id}` must set at most one of `username` or `username_env`"
    )]
    InvalidEmailSmtpUsernameSource { provider_id: String },
    #[error(
        "email_smtp provider `{provider_id}` must set at most one of `password` or `password_env`"
    )]
    InvalidEmailSmtpPasswordSource { provider_id: String },
    #[error("email_smtp provider `{provider_id}` must set both username and password, or neither")]
    InvalidEmailSmtpCredentials { provider_id: String },
    #[error(
        "`prompt_detail = \"on\"` is not supported with `{provider_type}` provider `{provider_id}` because that provider has a message size limit"
    )]
    PromptDetailNotSupportedForProvider {
        provider_id: String,
        provider_type: &'static str,
    },
    #[error(
        "`answer_detail = \"full\"` is not supported with `{provider_type}` provider `{provider_id}` because that provider has a message size limit"
    )]
    AnswerDetailNotSupportedForProvider {
        provider_id: String,
        provider_type: &'static str,
    },
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

        self.validate_notification_provider_compatibility()?;

        Ok(())
    }

    fn validate_notification_provider_compatibility(&self) -> Result<(), ConfigError> {
        if self.notification.answer_detail != AnswerDetail::Full
            && self.notification.prompt_detail != PromptDetail::On
        {
            return Ok(());
        }

        for route in &self.routes {
            for provider_id in &route.providers {
                let Some(provider) = self.provider(provider_id) else {
                    continue;
                };
                if provider.provider_type.has_message_size_limit() {
                    if self.notification.answer_detail == AnswerDetail::Full {
                        return Err(ConfigError::AnswerDetailNotSupportedForProvider {
                            provider_id: provider.id.clone(),
                            provider_type: provider.provider_type.as_str(),
                        });
                    }

                    return Err(ConfigError::PromptDetailNotSupportedForProvider {
                        provider_id: provider.id.clone(),
                        provider_type: provider.provider_type.as_str(),
                    });
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
            ProviderType::Pushover => {
                let has_app_token = is_present(self.app_token.as_deref());
                let has_app_token_env = is_present(self.app_token_env.as_deref());
                if has_app_token == has_app_token_env {
                    return Err(ConfigError::InvalidPushoverAppTokenSource {
                        provider_id: self.id.clone(),
                    });
                }

                let has_user_key = is_present(self.user_key.as_deref());
                let has_user_key_env = is_present(self.user_key_env.as_deref());
                if has_user_key == has_user_key_env {
                    return Err(ConfigError::InvalidPushoverUserKeySource {
                        provider_id: self.id.clone(),
                    });
                }
            }
            ProviderType::Slack => {
                let has_url = is_present(self.url.as_deref());
                let has_url_env = is_present(self.url_env.as_deref());
                if has_url == has_url_env {
                    return Err(ConfigError::InvalidSlackUrlSource {
                        provider_id: self.id.clone(),
                    });
                }
            }
            ProviderType::Discord => {
                let has_url = is_present(self.url.as_deref());
                let has_url_env = is_present(self.url_env.as_deref());
                if has_url == has_url_env {
                    return Err(ConfigError::InvalidDiscordUrlSource {
                        provider_id: self.id.clone(),
                    });
                }
            }
            ProviderType::Telegram => {
                let has_bot_token = is_present(self.bot_token.as_deref());
                let has_bot_token_env = is_present(self.bot_token_env.as_deref());
                if has_bot_token == has_bot_token_env {
                    return Err(ConfigError::InvalidTelegramBotTokenSource {
                        provider_id: self.id.clone(),
                    });
                }
                self.require_present("chat_id", self.chat_id.as_deref())?;
            }
            ProviderType::Whatsapp => {
                let has_access_token = is_present(self.access_token.as_deref());
                let has_access_token_env = is_present(self.access_token_env.as_deref());
                if has_access_token == has_access_token_env {
                    return Err(ConfigError::InvalidWhatsappAccessTokenSource {
                        provider_id: self.id.clone(),
                    });
                }
                self.require_present("phone_number_id", self.phone_number_id.as_deref())?;
                self.require_present(
                    "recipient_phone_number",
                    self.recipient_phone_number.as_deref(),
                )?;
            }
            ProviderType::Weixin => {
                self.require_present("base_url", self.base_url.as_deref())?;
                let has_token = is_present(self.token.as_deref());
                let has_token_env = is_present(self.token_env.as_deref());
                if has_token == has_token_env {
                    return Err(ConfigError::InvalidWeixinTokenSource {
                        provider_id: self.id.clone(),
                    });
                }
                self.require_present("recipient_user_id", self.recipient_user_id.as_deref())?;
                let has_context_token = is_present(self.context_token.as_deref());
                let has_context_token_env = is_present(self.context_token_env.as_deref());
                if has_context_token == has_context_token_env {
                    return Err(ConfigError::InvalidWeixinContextTokenSource {
                        provider_id: self.id.clone(),
                    });
                }
            }
            ProviderType::MicrosoftTeams => {
                let has_url = is_present(self.url.as_deref());
                let has_url_env = is_present(self.url_env.as_deref());
                if has_url == has_url_env {
                    return Err(ConfigError::InvalidMicrosoftTeamsUrlSource {
                        provider_id: self.id.clone(),
                    });
                }
            }
            ProviderType::EmailSmtp => {
                self.require_present("host", self.host.as_deref())?;
                if self.port.is_none() {
                    return Err(ConfigError::MissingProviderField {
                        provider_id: self.id.clone(),
                        field: "port",
                    });
                }
                if self.security.is_none() {
                    return Err(ConfigError::MissingProviderField {
                        provider_id: self.id.clone(),
                        field: "security",
                    });
                }
                self.require_present("from", self.from.as_deref())?;
                if self
                    .to
                    .as_ref()
                    .is_none_or(|recipients| recipients.is_empty())
                {
                    return Err(ConfigError::MissingProviderField {
                        provider_id: self.id.clone(),
                        field: "to",
                    });
                }

                let has_username = is_present(self.username.as_deref());
                let has_username_env = is_present(self.username_env.as_deref());
                if has_username && has_username_env {
                    return Err(ConfigError::InvalidEmailSmtpUsernameSource {
                        provider_id: self.id.clone(),
                    });
                }

                let has_password = is_present(self.password.as_deref());
                let has_password_env = is_present(self.password_env.as_deref());
                if has_password && has_password_env {
                    return Err(ConfigError::InvalidEmailSmtpPasswordSource {
                        provider_id: self.id.clone(),
                    });
                }

                if (has_username || has_username_env) != (has_password || has_password_env) {
                    return Err(ConfigError::InvalidEmailSmtpCredentials {
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

[[providers]]
id = "pushover"
type = "pushover"
app_token_env = "AGENTS_NOTIFIER_PUSHOVER_APP_TOKEN"
user_key_env = "AGENTS_NOTIFIER_PUSHOVER_USER_KEY"

[[providers]]
id = "slack"
type = "slack"
url_env = "AGENTS_NOTIFIER_SLACK_WEBHOOK_URL"

[[providers]]
id = "discord"
type = "discord"
url_env = "AGENTS_NOTIFIER_DISCORD_WEBHOOK_URL"

[[providers]]
id = "telegram"
type = "telegram"
bot_token_env = "AGENTS_NOTIFIER_TELEGRAM_BOT_TOKEN"
chat_id = "123456789"

[[providers]]
id = "whatsapp"
type = "whatsapp"
access_token_env = "AGENTS_NOTIFIER_WHATSAPP_ACCESS_TOKEN"
phone_number_id = "123456789"
recipient_phone_number = "15551234567"

[[providers]]
id = "weixin"
type = "weixin"
base_url = "https://ilinkai.weixin.qq.com"
token_env = "AGENTS_NOTIFIER_WEIXIN_TOKEN"
recipient_user_id = "user@im.wechat"
context_token_env = "AGENTS_NOTIFIER_WEIXIN_CONTEXT_TOKEN"

[[providers]]
id = "microsoft_teams"
type = "microsoft_teams"
url_env = "AGENTS_NOTIFIER_MICROSOFT_TEAMS_WEBHOOK_URL"

[[providers]]
id = "email"
type = "email_smtp"
host = "smtp.example.com"
port = 587
security = "starttls"
username_env = "AGENTS_NOTIFIER_EMAIL_SMTP_USERNAME"
password_env = "AGENTS_NOTIFIER_EMAIL_SMTP_PASSWORD"
from = "Agents Notifier <alerts@example.com>"
to = ["felix@example.com"]

[[routes]]
sources = ["codex_desktop", "codex_cli"]
providers = ["phone", "debug_webhook", "work_chat", "pushover", "slack", "discord", "telegram", "whatsapp", "weixin", "microsoft_teams", "email"]
"#;

    #[test]
    fn parses_valid_config() {
        let config = Config::from_toml_str(VALID_CONFIG).expect("valid config should parse");

        assert_eq!(config.schema_version, 1);
        assert_eq!(config.sources.len(), 2);
        assert_eq!(config.providers.len(), 11);
        assert_eq!(config.routes.len(), 1);
        assert_eq!(config.cli.language, CliLanguage::English);
        assert_eq!(config.log.level, "info");
        assert_eq!(config.notification.answer_detail, AnswerDetail::Preview);
        assert_eq!(config.notification.prompt_detail, PromptDetail::Off);
    }

    #[test]
    fn parses_cli_language() {
        let raw = r#"
schema_version = 1

[cli]
language = "zh-CN"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "debug_webhook"
type = "webhook"
url = "https://example.com/hook"

[[routes]]
sources = ["codex_cli"]
providers = ["debug_webhook"]
"#;

        let config = Config::from_toml_str(raw).expect("valid config should parse");

        assert_eq!(config.cli.language, CliLanguage::SimplifiedChinese);
    }

    #[test]
    fn parses_cli_language_aliases() {
        assert_eq!(CliLanguage::parse("en"), Some(CliLanguage::English));
        assert_eq!(
            CliLanguage::parse("zh_CN"),
            Some(CliLanguage::SimplifiedChinese)
        );
        assert_eq!(
            CliLanguage::parse("简体中文"),
            Some(CliLanguage::SimplifiedChinese)
        );
        assert_eq!(CliLanguage::parse("fr"), None);
    }

    #[test]
    fn parses_full_answer_detail() {
        let raw = r#"
schema_version = 1

[notification]
answer_detail = "full"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "debug_webhook"
type = "webhook"
url = "https://example.com/hook"

[[routes]]
sources = ["codex_cli"]
providers = ["debug_webhook"]
"#;

        let config = Config::from_toml_str(raw).expect("valid config should parse");

        assert_eq!(config.notification.answer_detail, AnswerDetail::Full);
    }

    #[test]
    fn rejects_full_answer_detail_with_ntfy_provider() {
        let raw = r#"
schema_version = 1

[notification]
answer_detail = "full"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "phone"
type = "ntfy"
server = "https://ntfy.sh"
topic = "topic"

[[routes]]
sources = ["codex_cli"]
providers = ["phone"]
"#;

        let err = Config::from_toml_str(raw).expect_err("ntfy should reject full answer detail");

        assert!(matches!(
            err,
            ConfigError::AnswerDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "phone" && provider_type == "ntfy"
        ));
    }

    #[test]
    fn rejects_full_answer_detail_with_pushover_provider() {
        let raw = r#"
schema_version = 1

[notification]
answer_detail = "full"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "pushover"
type = "pushover"
app_token = "123456789012345678901234567890"
user_key = "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ"

[[routes]]
sources = ["codex_cli"]
providers = ["pushover"]
"#;

        let err =
            Config::from_toml_str(raw).expect_err("Pushover should reject full answer detail");

        assert!(matches!(
            err,
            ConfigError::AnswerDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "pushover" && provider_type == "pushover"
        ));
    }

    #[test]
    fn rejects_full_answer_detail_with_slack_provider() {
        let raw = r#"
schema_version = 1

[notification]
answer_detail = "full"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "slack"
type = "slack"
url = "https://example.com/slack"

[[routes]]
sources = ["codex_cli"]
providers = ["slack"]
"#;

        let err = Config::from_toml_str(raw).expect_err("Slack should reject full answer detail");

        assert!(matches!(
            err,
            ConfigError::AnswerDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "slack" && provider_type == "slack"
        ));
    }

    #[test]
    fn rejects_full_answer_detail_with_discord_provider() {
        let raw = r#"
schema_version = 1

[notification]
answer_detail = "full"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "discord"
type = "discord"
url = "https://discord.com/api/webhooks/123456789012345678/token"

[[routes]]
sources = ["codex_cli"]
providers = ["discord"]
"#;

        let err = Config::from_toml_str(raw).expect_err("Discord should reject full answer detail");

        assert!(matches!(
            err,
            ConfigError::AnswerDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "discord" && provider_type == "discord"
        ));
    }

    #[test]
    fn rejects_full_answer_detail_with_telegram_provider() {
        let raw = r#"
schema_version = 1

[notification]
answer_detail = "full"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "telegram"
type = "telegram"
bot_token = "123456:test-token"
chat_id = "123456789"

[[routes]]
sources = ["codex_cli"]
providers = ["telegram"]
"#;

        let err =
            Config::from_toml_str(raw).expect_err("Telegram should reject full answer detail");

        assert!(matches!(
            err,
            ConfigError::AnswerDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "telegram" && provider_type == "telegram"
        ));
    }

    #[test]
    fn rejects_full_answer_detail_with_weixin_provider() {
        let raw = r#"
schema_version = 1

[notification]
answer_detail = "full"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "weixin"
type = "weixin"
base_url = "https://ilinkai.weixin.qq.com"
token = "test-token"
recipient_user_id = "user@im.wechat"
context_token = "test-context-token"

[[routes]]
sources = ["codex_cli"]
providers = ["weixin"]
"#;

        let err = Config::from_toml_str(raw).expect_err("WeChat should reject full answer detail");

        assert!(matches!(
            err,
            ConfigError::AnswerDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "weixin" && provider_type == "weixin"
        ));
    }

    #[test]
    fn parses_on_prompt_detail() {
        let raw = r#"
schema_version = 1

[notification]
prompt_detail = "on"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "debug_webhook"
type = "webhook"
url = "https://example.com/hook"

[[routes]]
sources = ["codex_cli"]
providers = ["debug_webhook"]
"#;

        let config = Config::from_toml_str(raw).expect("valid config should parse");

        assert_eq!(config.notification.prompt_detail, PromptDetail::On);
    }

    #[test]
    fn rejects_prompt_detail_on_with_ntfy_provider() {
        let raw = r#"
schema_version = 1

[notification]
prompt_detail = "on"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "phone"
type = "ntfy"
server = "https://ntfy.sh"
topic = "topic"

[[routes]]
sources = ["codex_cli"]
providers = ["phone"]
"#;

        let err = Config::from_toml_str(raw).expect_err("ntfy should reject prompt detail");

        assert!(matches!(
            err,
            ConfigError::PromptDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "phone" && provider_type == "ntfy"
        ));
    }

    #[test]
    fn rejects_prompt_detail_on_with_pushover_provider() {
        let raw = r#"
schema_version = 1

[notification]
prompt_detail = "on"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "pushover"
type = "pushover"
app_token = "123456789012345678901234567890"
user_key = "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ"

[[routes]]
sources = ["codex_cli"]
providers = ["pushover"]
"#;

        let err = Config::from_toml_str(raw).expect_err("Pushover should reject prompt detail");

        assert!(matches!(
            err,
            ConfigError::PromptDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "pushover" && provider_type == "pushover"
        ));
    }

    #[test]
    fn rejects_prompt_detail_on_with_slack_provider() {
        let raw = r#"
schema_version = 1

[notification]
prompt_detail = "on"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "slack"
type = "slack"
url = "https://example.com/slack"

[[routes]]
sources = ["codex_cli"]
providers = ["slack"]
"#;

        let err = Config::from_toml_str(raw).expect_err("Slack should reject prompt detail");

        assert!(matches!(
            err,
            ConfigError::PromptDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "slack" && provider_type == "slack"
        ));
    }

    #[test]
    fn rejects_prompt_detail_on_with_discord_provider() {
        let raw = r#"
schema_version = 1

[notification]
prompt_detail = "on"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "discord"
type = "discord"
url = "https://discord.com/api/webhooks/123456789012345678/token"

[[routes]]
sources = ["codex_cli"]
providers = ["discord"]
"#;

        let err = Config::from_toml_str(raw).expect_err("Discord should reject prompt detail");

        assert!(matches!(
            err,
            ConfigError::PromptDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "discord" && provider_type == "discord"
        ));
    }

    #[test]
    fn rejects_prompt_detail_on_with_whatsapp_provider() {
        let raw = r#"
schema_version = 1

[notification]
prompt_detail = "on"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "whatsapp"
type = "whatsapp"
access_token = "test-access-token"
phone_number_id = "123456789"
recipient_phone_number = "15551234567"

[[routes]]
sources = ["codex_cli"]
providers = ["whatsapp"]
"#;

        let err = Config::from_toml_str(raw).expect_err("WhatsApp should reject prompt detail");

        assert!(matches!(
            err,
            ConfigError::PromptDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "whatsapp" && provider_type == "whatsapp"
        ));
    }

    #[test]
    fn rejects_prompt_detail_on_with_weixin_provider() {
        let raw = r#"
schema_version = 1

[notification]
prompt_detail = "on"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "weixin"
type = "weixin"
base_url = "https://ilinkai.weixin.qq.com"
token = "test-token"
recipient_user_id = "user@im.wechat"
context_token = "test-context-token"

[[routes]]
sources = ["codex_cli"]
providers = ["weixin"]
"#;

        let err = Config::from_toml_str(raw).expect_err("WeChat should reject prompt detail");

        assert!(matches!(
            err,
            ConfigError::PromptDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "weixin" && provider_type == "weixin"
        ));
    }

    #[test]
    fn rejects_prompt_detail_on_with_microsoft_teams_provider() {
        let raw = r#"
schema_version = 1

[notification]
prompt_detail = "on"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "microsoft_teams"
type = "microsoft_teams"
url = "https://example.com/workflow"

[[routes]]
sources = ["codex_cli"]
providers = ["microsoft_teams"]
"#;

        let err =
            Config::from_toml_str(raw).expect_err("Microsoft Teams should reject prompt detail");

        assert!(matches!(
            err,
            ConfigError::PromptDetailNotSupportedForProvider {
                provider_id,
                provider_type
            } if provider_id == "microsoft_teams" && provider_type == "microsoft_teams"
        ));
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

    #[test]
    fn rejects_pushover_without_app_token_source() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "pushover"
type = "pushover"
user_key = "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ"

[[routes]]
sources = ["codex_cli"]
providers = ["pushover"]
"#;

        let err = Config::from_toml_str(raw).expect_err("missing app token should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidPushoverAppTokenSource { provider_id } if provider_id == "pushover"
        ));
    }

    #[test]
    fn rejects_pushover_with_both_user_key_sources() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "pushover"
type = "pushover"
app_token = "123456789012345678901234567890"
user_key = "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ"
user_key_env = "AGENTS_NOTIFIER_PUSHOVER_USER_KEY"

[[routes]]
sources = ["codex_cli"]
providers = ["pushover"]
"#;

        let err = Config::from_toml_str(raw).expect_err("ambiguous user key should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidPushoverUserKeySource { provider_id } if provider_id == "pushover"
        ));
    }

    #[test]
    fn rejects_slack_without_webhook_url() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "slack"
type = "slack"

[[routes]]
sources = ["codex_cli"]
providers = ["slack"]
"#;

        let err = Config::from_toml_str(raw).expect_err("missing Slack webhook URL should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidSlackUrlSource { provider_id } if provider_id == "slack"
        ));
    }

    #[test]
    fn rejects_discord_with_both_url_sources() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "discord"
type = "discord"
url = "https://discord.com/api/webhooks/123456789012345678/token"
url_env = "AGENTS_NOTIFIER_DISCORD_WEBHOOK_URL"

[[routes]]
sources = ["codex_cli"]
providers = ["discord"]
"#;

        let err = Config::from_toml_str(raw).expect_err("ambiguous Discord URL should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidDiscordUrlSource { provider_id } if provider_id == "discord"
        ));
    }

    #[test]
    fn rejects_telegram_without_bot_token_source() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "telegram"
type = "telegram"
chat_id = "123456789"

[[routes]]
sources = ["codex_cli"]
providers = ["telegram"]
"#;

        let err = Config::from_toml_str(raw).expect_err("missing Telegram bot token should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidTelegramBotTokenSource { provider_id } if provider_id == "telegram"
        ));
    }

    #[test]
    fn rejects_whatsapp_without_access_token_source() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "whatsapp"
type = "whatsapp"
phone_number_id = "123456789"
recipient_phone_number = "15551234567"

[[routes]]
sources = ["codex_cli"]
providers = ["whatsapp"]
"#;

        let err =
            Config::from_toml_str(raw).expect_err("missing WhatsApp access token should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidWhatsappAccessTokenSource { provider_id } if provider_id == "whatsapp"
        ));
    }

    #[test]
    fn rejects_weixin_without_token_source() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "weixin"
type = "weixin"
base_url = "https://ilinkai.weixin.qq.com"
recipient_user_id = "user@im.wechat"
context_token = "test-context-token"

[[routes]]
sources = ["codex_cli"]
providers = ["weixin"]
"#;

        let err = Config::from_toml_str(raw).expect_err("missing WeChat token should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidWeixinTokenSource { provider_id } if provider_id == "weixin"
        ));
    }

    #[test]
    fn rejects_weixin_without_context_token_source() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "weixin"
type = "weixin"
base_url = "https://ilinkai.weixin.qq.com"
token = "test-token"
recipient_user_id = "user@im.wechat"

[[routes]]
sources = ["codex_cli"]
providers = ["weixin"]
"#;

        let err = Config::from_toml_str(raw).expect_err("missing WeChat context token should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidWeixinContextTokenSource { provider_id } if provider_id == "weixin"
        ));
    }

    #[test]
    fn rejects_microsoft_teams_with_both_url_sources() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "microsoft_teams"
type = "microsoft_teams"
url = "https://example.com/workflow"
url_env = "AGENTS_NOTIFIER_MICROSOFT_TEAMS_WEBHOOK_URL"

[[routes]]
sources = ["codex_cli"]
providers = ["microsoft_teams"]
"#;

        let err =
            Config::from_toml_str(raw).expect_err("ambiguous Microsoft Teams URL should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidMicrosoftTeamsUrlSource { provider_id } if provider_id == "microsoft_teams"
        ));
    }

    #[test]
    fn rejects_email_smtp_without_required_fields() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "email"
type = "email_smtp"
host = "smtp.example.com"
port = 587
security = "starttls"
from = "alerts@example.com"

[[routes]]
sources = ["codex_cli"]
providers = ["email"]
"#;

        let err = Config::from_toml_str(raw).expect_err("missing recipients should fail");

        assert!(matches!(
            err,
            ConfigError::MissingProviderField { provider_id, field }
                if provider_id == "email" && field == "to"
        ));
    }

    #[test]
    fn rejects_email_smtp_partial_credentials() {
        let raw = r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "email"
type = "email_smtp"
host = "smtp.example.com"
port = 587
security = "starttls"
username = "alerts@example.com"
from = "alerts@example.com"
to = ["felix@example.com"]

[[routes]]
sources = ["codex_cli"]
providers = ["email"]
"#;

        let err = Config::from_toml_str(raw).expect_err("partial credentials should fail");

        assert!(matches!(
            err,
            ConfigError::InvalidEmailSmtpCredentials { provider_id } if provider_id == "email"
        ));
    }
}
