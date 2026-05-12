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
    Wechat,
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
            Self::Wechat => "wechat",
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
            | Self::Wechat
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
    #[error("wechat provider `{provider_id}` must set exactly one of `token` or `token_env`")]
    InvalidWechatTokenSource { provider_id: String },
    #[error(
        "wechat provider `{provider_id}` must set exactly one of `context_token` or `context_token_env`"
    )]
    InvalidWechatContextTokenSource { provider_id: String },
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
    pub fn new(id: impl Into<String>, provider_type: ProviderType) -> Self {
        Self {
            id: id.into(),
            provider_type,
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
            access_token: None,
            access_token_env: None,
            phone_number_id: None,
            recipient_phone_number: None,
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
        }
    }

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
            ProviderType::Wechat => {
                self.require_present("base_url", self.base_url.as_deref())?;
                let has_token = is_present(self.token.as_deref());
                let has_token_env = is_present(self.token_env.as_deref());
                if has_token == has_token_env {
                    return Err(ConfigError::InvalidWechatTokenSource {
                        provider_id: self.id.clone(),
                    });
                }
                self.require_present("recipient_user_id", self.recipient_user_id.as_deref())?;
                let has_context_token = is_present(self.context_token.as_deref());
                let has_context_token_env = is_present(self.context_token_env.as_deref());
                if has_context_token == has_context_token_env {
                    return Err(ConfigError::InvalidWechatContextTokenSource {
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
mod tests;
