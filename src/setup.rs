use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Context;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::Deserialize;
use serde_json::json;
use tokio::time::sleep;
use uuid::Uuid;

use crate::config::{
    AnswerDetail, CONFIG_SCHEMA_VERSION, CliConfig, Config, EmailSmtpSecurity, LogConfig,
    NotificationConfig, PromptDetail, ProviderConfig, ProviderType, RouteConfig, SourceConfig,
    SourceType,
};
use crate::provider_urls::{
    host_label, validate_custom_webhook_url, validate_discord_webhook_url,
    validate_microsoft_teams_webhook_url, validate_slack_webhook_url,
};

const DEFAULT_NTFY_SERVER: &str = "https://ntfy.sh";
const FEISHU_WEBHOOK_PREFIX: &str = "https://open.feishu.cn/open-apis/bot/v2/hook/";
const LARK_WEBHOOK_PREFIX: &str = "https://open.larksuite.com/open-apis/bot/v2/hook/";
pub const DEFAULT_WEIXIN_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
pub const DEFAULT_WEIXIN_BOT_TYPE: &str = "3";
pub const DEFAULT_WEIXIN_QR_TIMEOUT: Duration = Duration::from_secs(480);
pub const DEFAULT_WEIXIN_LINK_TIMEOUT: Duration = Duration::from_secs(180);
const WEIXIN_SETUP_CHANNEL_VERSION: &str = "agents-notifier-weixin-setup/1.0";
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookTarget {
    pub provider_id: String,
    pub webhook_host: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushoverTarget {
    pub provider_id: String,
    pub device: Option<String>,
    pub sound: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackTarget {
    pub provider_id: String,
    pub webhook_host: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordTarget {
    pub provider_id: String,
    pub webhook_host: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramTarget {
    pub provider_id: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhatsappTarget {
    pub provider_id: String,
    pub recipient_phone_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeixinTarget {
    pub provider_id: String,
    pub base_url_host: String,
    pub recipient_user_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrosoftTeamsTarget {
    pub provider_id: String,
    pub webhook_host: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeixinQrCode {
    pub qr_key: String,
    pub qr_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeixinQrStatus {
    pub status: String,
    pub token: Option<String>,
    pub account_id: Option<String>,
    pub base_url: Option<String>,
    pub scanned_user_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeixinRecipientLink {
    pub recipient_user_id: String,
    pub context_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailSmtpTarget {
    pub provider_id: String,
    pub host: String,
    pub port: u16,
    pub from: String,
    pub to: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSelection {
    CodexDesktop,
    CodexCli,
    ClaudeCode,
    CursorCli,
    OpenCodeCli,
    OpenClaw,
    HermesAgentCli,
    GithubCopilotCli,
    GeminiCli,
    Aider,
}

impl AgentSelection {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::CodexDesktop => "Codex Desktop",
            Self::CodexCli => "Codex CLI",
            Self::ClaudeCode => "Claude Code",
            Self::CursorCli => "Cursor CLI",
            Self::OpenCodeCli => "OpenCode CLI",
            Self::OpenClaw => "OpenClaw",
            Self::HermesAgentCli => "Hermes Agent CLI",
            Self::GithubCopilotCli => "GitHub Copilot CLI",
            Self::GeminiCli => "Gemini CLI",
            Self::Aider => "Aider",
        }
    }

    pub fn source_id(self) -> &'static str {
        match self {
            Self::CodexDesktop => "codex_desktop",
            Self::CodexCli => "codex_cli",
            Self::ClaudeCode => "claude_code",
            Self::CursorCli => "cursor_cli",
            Self::OpenCodeCli => "opencode_cli",
            Self::OpenClaw => "openclaw",
            Self::HermesAgentCli => "hermes_agent_cli",
            Self::GithubCopilotCli => "github_copilot_cli",
            Self::GeminiCli => "gemini_cli",
            Self::Aider => "aider",
        }
    }

    pub fn from_hook_source_id(source_id: &str) -> Option<Self> {
        match source_id {
            "cursor_cli" => Some(Self::CursorCli),
            "opencode_cli" => Some(Self::OpenCodeCli),
            "openclaw" => Some(Self::OpenClaw),
            "hermes_agent_cli" => Some(Self::HermesAgentCli),
            "github_copilot_cli" => Some(Self::GithubCopilotCli),
            "gemini_cli" => Some(Self::GeminiCli),
            "aider" => Some(Self::Aider),
            _ => None,
        }
    }

    fn source_config(self) -> SourceConfig {
        match self {
            Self::CodexDesktop => SourceConfig {
                id: self.source_id().to_string(),
                source_type: SourceType::CodexDesktop,
            },
            Self::CodexCli => SourceConfig {
                id: self.source_id().to_string(),
                source_type: SourceType::CodexCli,
            },
            Self::ClaudeCode => SourceConfig {
                id: self.source_id().to_string(),
                source_type: SourceType::ClaudeCode,
            },
            Self::CursorCli
            | Self::OpenCodeCli
            | Self::OpenClaw
            | Self::HermesAgentCli
            | Self::GithubCopilotCli
            | Self::GeminiCli
            | Self::Aider => SourceConfig {
                id: self.source_id().to_string(),
                source_type: SourceType::AgentHook,
            },
        }
    }
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

pub fn resolve_webhook_url(input: &str) -> anyhow::Result<String> {
    validate_custom_webhook_url(input)
}

pub fn resolve_slack_webhook_url(input: &str) -> anyhow::Result<String> {
    validate_slack_webhook_url(input)
}

pub fn resolve_discord_webhook_url(input: &str) -> anyhow::Result<String> {
    validate_discord_webhook_url(input)
}

pub fn resolve_telegram_bot_token(input: &str) -> anyhow::Result<String> {
    let token = input.trim();
    let Some((bot_id, secret)) = token.split_once(':') else {
        anyhow::bail!("Telegram bot token must look like `123456:ABC...`");
    };

    let valid = !bot_id.is_empty()
        && bot_id.bytes().all(|byte| byte.is_ascii_digit())
        && !secret.is_empty()
        && secret.bytes().all(|byte| !byte.is_ascii_whitespace());
    if !valid {
        anyhow::bail!("Telegram bot token must look like `123456:ABC...`");
    }

    Ok(token.to_string())
}

pub fn resolve_telegram_chat_id(input: &str) -> anyhow::Result<String> {
    let chat_id = input.trim();
    let valid = if let Some(username) = chat_id.strip_prefix('@') {
        !username.is_empty()
            && username
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    } else {
        let digits = chat_id.strip_prefix('-').unwrap_or(chat_id);
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    };

    if !valid {
        anyhow::bail!("Telegram chat id must be a numeric chat id or an `@channelusername`");
    }

    Ok(chat_id.to_string())
}

pub fn resolve_whatsapp_access_token(input: &str) -> anyhow::Result<String> {
    let access_token = input.trim();
    if access_token.is_empty() || access_token.bytes().any(|byte| byte.is_ascii_whitespace()) {
        anyhow::bail!("WhatsApp access token must be non-empty and must not contain whitespace");
    }

    Ok(access_token.to_string())
}

pub fn resolve_whatsapp_phone_number_id(input: &str) -> anyhow::Result<String> {
    let phone_number_id = input.trim();
    if phone_number_id.is_empty() || !phone_number_id.bytes().all(|byte| byte.is_ascii_digit()) {
        anyhow::bail!("WhatsApp phone number ID must contain only digits");
    }

    Ok(phone_number_id.to_string())
}

pub fn resolve_whatsapp_recipient_phone_number(input: &str) -> anyhow::Result<String> {
    let phone_number = input.trim();
    if !(7..=15).contains(&phone_number.len())
        || !phone_number.bytes().all(|byte| byte.is_ascii_digit())
    {
        anyhow::bail!(
            "WhatsApp recipient phone number must be 7 to 15 digits without spaces or punctuation"
        );
    }

    Ok(phone_number.to_string())
}

pub fn resolve_weixin_base_url(input: &str) -> anyhow::Result<String> {
    let base_url = input.trim().trim_end_matches('/');
    let parsed =
        reqwest::Url::parse(base_url).context("Weixin iLink base URL must be a valid HTTPS URL")?;
    let has_root_path = parsed.path().is_empty() || parsed.path() == "/";
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !has_root_path
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        anyhow::bail!(
            "Weixin iLink base URL must be an HTTPS origin like `https://ilinkai.weixin.qq.com`"
        );
    }

    Ok(base_url.to_string())
}

pub fn resolve_weixin_token(input: &str) -> anyhow::Result<String> {
    resolve_weixin_secret("Weixin iLink token", input)
}

pub fn resolve_weixin_recipient_user_id(input: &str) -> anyhow::Result<String> {
    let recipient_user_id = input.trim();
    if recipient_user_id.is_empty()
        || recipient_user_id
            .bytes()
            .any(|byte| byte.is_ascii_whitespace())
    {
        anyhow::bail!("Weixin recipient user id must be non-empty and must not contain whitespace");
    }

    Ok(recipient_user_id.to_string())
}

pub fn resolve_weixin_context_token(input: &str) -> anyhow::Result<String> {
    resolve_weixin_secret("Weixin context token", input)
}

pub fn resolve_weixin_route_tag(input: &str) -> anyhow::Result<Option<String>> {
    let route_tag = input.trim();
    if route_tag.is_empty() || route_tag.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    if route_tag.bytes().any(|byte| byte.is_ascii_whitespace()) {
        anyhow::bail!("Weixin SKRouteTag must not contain whitespace");
    }

    Ok(Some(route_tag.to_string()))
}

fn resolve_weixin_secret(label: &'static str, input: &str) -> anyhow::Result<String> {
    let token = input.trim();
    if token.is_empty() || token.bytes().any(|byte| byte.is_ascii_whitespace()) {
        anyhow::bail!("{label} must be non-empty and must not contain whitespace");
    }

    Ok(token.to_string())
}

pub fn resolve_microsoft_teams_webhook_url(input: &str) -> anyhow::Result<String> {
    validate_microsoft_teams_webhook_url(input)
}

pub fn resolve_email_smtp_host(input: &str) -> anyhow::Result<String> {
    let host = input.trim();
    if host.is_empty() {
        anyhow::bail!("Email SMTP host is required");
    }
    if host.contains("://")
        || host.contains('/')
        || host.contains('@')
        || host.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        anyhow::bail!("Email SMTP host must be a hostname, not a URL");
    }

    Ok(host.to_string())
}

pub fn resolve_email_smtp_port(input: &str) -> anyhow::Result<u16> {
    let port = input
        .trim()
        .parse::<u16>()
        .context("Email SMTP port must be a number")?;
    if port == 0 {
        anyhow::bail!("Email SMTP port must be greater than 0");
    }

    Ok(port)
}

pub fn resolve_email_smtp_security(input: &str) -> anyhow::Result<EmailSmtpSecurity> {
    match input.trim() {
        "starttls" => Ok(EmailSmtpSecurity::Starttls),
        "implicit_tls" => Ok(EmailSmtpSecurity::ImplicitTls),
        _ => anyhow::bail!("Email SMTP security must be `starttls` or `implicit_tls`"),
    }
}

pub fn resolve_email_smtp_optional_username(input: &str) -> anyhow::Result<Option<String>> {
    let username = input.trim();
    if username.is_empty() || username.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    if username.bytes().any(|byte| byte.is_ascii_whitespace()) {
        anyhow::bail!("Email SMTP username must not contain whitespace");
    }

    Ok(Some(username.to_string()))
}

pub fn resolve_email_smtp_password(input: &str) -> anyhow::Result<String> {
    let password = input.trim();
    if password.is_empty() {
        anyhow::bail!("Email SMTP password is required when username is configured");
    }

    Ok(password.to_string())
}

pub fn resolve_email_smtp_mailbox(input: &str, field: &'static str) -> anyhow::Result<String> {
    let mailbox = input.trim();
    if mailbox.is_empty() {
        anyhow::bail!("Email SMTP {field} is required");
    }
    mailbox
        .parse::<lettre::message::Mailbox>()
        .with_context(|| format!("Email SMTP {field} must be a valid email mailbox"))?;

    Ok(mailbox.to_string())
}

pub fn resolve_email_smtp_recipients(input: &str) -> anyhow::Result<Vec<String>> {
    let recipients = input
        .split(',')
        .map(str::trim)
        .filter(|recipient| !recipient.is_empty())
        .map(|recipient| resolve_email_smtp_mailbox(recipient, "recipient"))
        .collect::<anyhow::Result<Vec<_>>>()?;

    if recipients.is_empty() {
        anyhow::bail!("Email SMTP recipients are required");
    }

    Ok(recipients)
}

pub fn resolve_pushover_app_token(input: &str) -> anyhow::Result<String> {
    let token = input.trim();
    if !is_pushover_identifier(token) {
        anyhow::bail!("Pushover app token must be 30 letters or numbers");
    }

    Ok(token.to_string())
}

pub fn resolve_pushover_user_key(input: &str) -> anyhow::Result<String> {
    let user_key = input.trim();
    if !is_pushover_identifier(user_key) {
        anyhow::bail!("Pushover user or group key must be 30 letters or numbers");
    }

    Ok(user_key.to_string())
}

pub fn resolve_pushover_device(input: &str) -> anyhow::Result<Option<String>> {
    let device = input.trim();
    if device.is_empty() {
        return Ok(None);
    }

    let valid = device.split(',').all(|name| {
        !name.is_empty()
            && name.len() <= 25
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    });
    if !valid {
        anyhow::bail!(
            "Pushover device must be one or more comma-separated device names using letters, numbers, hyphens, and underscores"
        );
    }

    Ok(Some(device.to_string()))
}

pub fn resolve_pushover_sound(input: &str) -> Option<String> {
    let sound = input.trim();
    if sound.is_empty() {
        None
    } else {
        Some(sound.to_string())
    }
}

pub fn build_ntfy_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    topic: &str,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "phone".to_string(),
            provider_type: ProviderType::Ntfy,
            base_url: None,
            server: Some(DEFAULT_NTFY_SERVER.to_string()),
            topic: Some(topic.to_string()),
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
        }],
    )
}

pub fn build_feishu_lark_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    webhook_url: &str,
    secret: Option<String>,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "work_chat".to_string(),
            provider_type: ProviderType::FeishuLark,
            base_url: None,
            server: None,
            topic: None,
            url: Some(webhook_url.to_string()),
            url_env: None,
            secret,
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
        }],
    )
}

pub fn build_webhook_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    webhook_url: &str,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "webhook".to_string(),
            provider_type: ProviderType::Webhook,
            base_url: None,
            server: None,
            topic: None,
            url: Some(webhook_url.to_string()),
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
        }],
    )
}

pub fn build_pushover_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    app_token: &str,
    user_key: &str,
    device: Option<String>,
    sound: Option<String>,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "pushover".to_string(),
            provider_type: ProviderType::Pushover,
            base_url: None,
            server: None,
            topic: None,
            url: None,
            url_env: None,
            secret: None,
            secret_env: None,
            app_token: Some(app_token.to_string()),
            app_token_env: None,
            user_key: Some(user_key.to_string()),
            user_key_env: None,
            device,
            sound,
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
        }],
    )
}

pub fn build_slack_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    webhook_url: &str,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "slack".to_string(),
            provider_type: ProviderType::Slack,
            base_url: None,
            server: None,
            topic: None,
            url: Some(webhook_url.to_string()),
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
        }],
    )
}

pub fn build_discord_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    webhook_url: &str,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "discord".to_string(),
            provider_type: ProviderType::Discord,
            base_url: None,
            server: None,
            topic: None,
            url: Some(webhook_url.to_string()),
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
        }],
    )
}

pub fn build_telegram_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    bot_token: &str,
    chat_id: &str,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "telegram".to_string(),
            provider_type: ProviderType::Telegram,
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
            bot_token: Some(bot_token.to_string()),
            bot_token_env: None,
            chat_id: Some(chat_id.to_string()),
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
        }],
    )
}

pub fn build_whatsapp_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    access_token: &str,
    phone_number_id: &str,
    recipient_phone_number: &str,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
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
            access_token: Some(access_token.to_string()),
            access_token_env: None,
            phone_number_id: Some(phone_number_id.to_string()),
            recipient_phone_number: Some(recipient_phone_number.to_string()),
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
        }],
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_weixin_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    base_url: &str,
    token: &str,
    recipient_user_id: &str,
    context_token: &str,
    route_tag: Option<String>,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "weixin".to_string(),
            provider_type: ProviderType::Weixin,
            base_url: Some(base_url.to_string()),
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
            token: Some(token.to_string()),
            token_env: None,
            recipient_user_id: Some(recipient_user_id.to_string()),
            context_token: Some(context_token.to_string()),
            context_token_env: None,
            route_tag,
        }],
    )
}

pub fn build_microsoft_teams_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    webhook_url: &str,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "microsoft_teams".to_string(),
            provider_type: ProviderType::MicrosoftTeams,
            base_url: None,
            server: None,
            topic: None,
            url: Some(webhook_url.to_string()),
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
        }],
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_email_smtp_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    host: &str,
    port: u16,
    security: EmailSmtpSecurity,
    username: Option<String>,
    password: Option<String>,
    from: &str,
    to: Vec<String>,
    reply_to: Option<String>,
) -> Config {
    build_config(
        agent,
        answer_detail,
        prompt_detail,
        vec![ProviderConfig {
            id: "email".to_string(),
            provider_type: ProviderType::EmailSmtp,
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
            host: Some(host.to_string()),
            port: Some(port),
            security: Some(security),
            username,
            username_env: None,
            password,
            password_env: None,
            from: Some(from.to_string()),
            to: Some(to),
            reply_to,
            token: None,
            token_env: None,
            recipient_user_id: None,
            context_token: None,
            context_token_env: None,
            route_tag: None,
        }],
    )
}

fn build_config(
    agent: AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    providers: Vec<ProviderConfig>,
) -> Config {
    let provider_ids = providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect();
    let agent_source = agent.source_config();
    let agent_source_id = agent_source.id.clone();

    Config {
        schema_version: CONFIG_SCHEMA_VERSION,
        cli: CliConfig::default(),
        log: LogConfig::default(),
        notification: NotificationConfig {
            answer_detail,
            prompt_detail,
        },
        sources: vec![
            agent_source,
            SourceConfig {
                id: "agents_notifier".to_string(),
                source_type: SourceType::AgentsNotifier,
            },
        ],
        providers,
        routes: vec![RouteConfig {
            sources: vec![agent_source_id, "agents_notifier".to_string()],
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

pub fn webhook_targets(config: &Config) -> Vec<WebhookTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::Webhook)
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

            Some(WebhookTarget {
                provider_id: provider.id.clone(),
                webhook_host,
            })
        })
        .collect()
}

pub fn pushover_targets(config: &Config) -> Vec<PushoverTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::Pushover)
        .map(|provider| PushoverTarget {
            provider_id: provider.id.clone(),
            device: provider.device.clone(),
            sound: provider.sound.clone(),
        })
        .collect()
}

pub fn slack_targets(config: &Config) -> Vec<SlackTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::Slack)
        .filter_map(|provider| {
            provider_url_host(provider).map(|webhook_host| SlackTarget {
                provider_id: provider.id.clone(),
                webhook_host,
            })
        })
        .collect()
}

pub fn discord_targets(config: &Config) -> Vec<DiscordTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::Discord)
        .filter_map(|provider| {
            provider_url_host(provider).map(|webhook_host| DiscordTarget {
                provider_id: provider.id.clone(),
                webhook_host,
            })
        })
        .collect()
}

pub fn telegram_targets(config: &Config) -> Vec<TelegramTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::Telegram)
        .filter_map(|provider| {
            Some(TelegramTarget {
                provider_id: provider.id.clone(),
                chat_id: provider.chat_id.as_ref()?.clone(),
            })
        })
        .collect()
}

pub fn whatsapp_targets(config: &Config) -> Vec<WhatsappTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::Whatsapp)
        .filter_map(|provider| {
            Some(WhatsappTarget {
                provider_id: provider.id.clone(),
                recipient_phone_number: provider.recipient_phone_number.as_ref()?.clone(),
            })
        })
        .collect()
}

pub fn weixin_targets(config: &Config) -> Vec<WeixinTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::Weixin)
        .filter_map(|provider| {
            let base_url = provider.base_url.as_ref()?;
            Some(WeixinTarget {
                provider_id: provider.id.clone(),
                base_url_host: host_label(base_url),
                recipient_user_id: provider.recipient_user_id.as_ref()?.clone(),
            })
        })
        .collect()
}

pub fn microsoft_teams_targets(config: &Config) -> Vec<MicrosoftTeamsTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::MicrosoftTeams)
        .filter_map(|provider| {
            provider_url_host(provider).map(|webhook_host| MicrosoftTeamsTarget {
                provider_id: provider.id.clone(),
                webhook_host,
            })
        })
        .collect()
}

pub fn email_smtp_targets(config: &Config) -> Vec<EmailSmtpTarget> {
    config
        .providers
        .iter()
        .filter(|provider| provider.provider_type == ProviderType::EmailSmtp)
        .filter_map(|provider| {
            Some(EmailSmtpTarget {
                provider_id: provider.id.clone(),
                host: provider.host.as_ref()?.clone(),
                port: provider.port?,
                from: provider.from.as_ref()?.clone(),
                to: provider.to.as_ref()?.clone(),
            })
        })
        .collect()
}

pub async fn fetch_weixin_qr_code(
    base_url: &str,
    route_tag: Option<&str>,
    bot_type: &str,
) -> anyhow::Result<WeixinQrCode> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(40))
        .build()
        .context("failed to build Weixin setup HTTP client")?;
    let mut url = weixin_url(base_url, &["ilink", "bot", "get_bot_qrcode"])?;
    url.query_pairs_mut().append_pair("bot_type", bot_type);

    let mut request = client.get(url);
    if let Some(route_tag) = route_tag.filter(|value| !value.trim().is_empty()) {
        request = request.header("SKRouteTag", route_tag);
    }

    let response = request
        .send()
        .await
        .context("failed to request Weixin QR code")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read Weixin QR code response")?;
    if !status.is_success() {
        anyhow::bail!(
            "Weixin QR code request returned HTTP {}: {}",
            status.as_u16(),
            short_response_body(&body)
        );
    }

    let payload: WeixinQrCodeResponse =
        serde_json::from_str(&body).context("failed to parse Weixin QR code response")?;
    let qr_key = payload.qrcode.trim();
    let qr_content = payload.qrcode_img_content.trim();
    if qr_key.is_empty() {
        anyhow::bail!("Weixin QR code response did not include `qrcode`");
    }
    if qr_content.is_empty() {
        anyhow::bail!("Weixin QR code response did not include `qrcode_img_content`");
    }

    Ok(WeixinQrCode {
        qr_key: qr_key.to_string(),
        qr_content: qr_content.to_string(),
    })
}

pub async fn poll_weixin_qr_status(
    base_url: &str,
    qr_key: &str,
    route_tag: Option<&str>,
) -> anyhow::Result<WeixinQrStatus> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(40))
        .build()
        .context("failed to build Weixin setup HTTP client")?;
    let mut url = weixin_url(base_url, &["ilink", "bot", "get_qrcode_status"])?;
    url.query_pairs_mut().append_pair("qrcode", qr_key);

    let mut request = client.get(url).header("iLink-App-ClientVersion", "1");
    if let Some(route_tag) = route_tag.filter(|value| !value.trim().is_empty()) {
        request = request.header("SKRouteTag", route_tag);
    }

    let response = request
        .send()
        .await
        .context("failed to request Weixin QR status")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read Weixin QR status response")?;
    if !status.is_success() {
        anyhow::bail!(
            "Weixin QR status request returned HTTP {}: {}",
            status.as_u16(),
            short_response_body(&body)
        );
    }

    let payload: WeixinQrStatusResponse =
        serde_json::from_str(&body).context("failed to parse Weixin QR status response")?;

    Ok(WeixinQrStatus {
        status: payload.status.trim().to_string(),
        token: present_str(payload.bot_token),
        account_id: present_str(payload.ilink_bot_id),
        base_url: present_str(payload.baseurl).map(|value| value.trim_end_matches('/').to_string()),
        scanned_user_id: present_str(payload.ilink_user_id),
    })
}

pub async fn verify_weixin_token(
    base_url: &str,
    token: &str,
    route_tag: Option<&str>,
) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("failed to build Weixin setup HTTP client")?;
    let url = weixin_url(base_url, &["ilink", "bot", "getupdates"])?;
    let body = json!({
        "get_updates_buf": "",
        "base_info": {
            "channel_version": WEIXIN_SETUP_CHANNEL_VERSION
        }
    });

    let mut request = client
        .post(url)
        .bearer_auth(token)
        .header("AuthorizationType", "ilink_bot_token")
        .header("X-WECHAT-UIN", random_weixin_uin())
        .json(&body);
    if let Some(route_tag) = route_tag.filter(|value| !value.trim().is_empty()) {
        request = request.header("SKRouteTag", route_tag);
    }

    let response = request
        .send()
        .await
        .context("failed to verify Weixin iLink token")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read Weixin token verification response")?;
    if !status.is_success() {
        anyhow::bail!(
            "Weixin token verification returned HTTP {}: {}",
            status.as_u16(),
            short_response_body(&body)
        );
    }
    let _: serde_json::Value =
        serde_json::from_str(&body).context("Weixin token verification returned invalid JSON")?;

    Ok(())
}

pub async fn poll_weixin_recipient_link(
    base_url: &str,
    token: &str,
    route_tag: Option<&str>,
    expected_user_id: Option<&str>,
    timeout: Duration,
) -> anyhow::Result<WeixinRecipientLink> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(40))
        .build()
        .context("failed to build Weixin setup HTTP client")?;
    let url = weixin_url(base_url, &["ilink", "bot", "getupdates"])?;
    let deadline = Instant::now() + timeout;
    let mut get_updates_buf = String::new();

    while Instant::now() < deadline {
        let body = json!({
            "get_updates_buf": get_updates_buf,
            "base_info": {
                "channel_version": WEIXIN_SETUP_CHANNEL_VERSION
            }
        });
        let mut request = client
            .post(url.clone())
            .bearer_auth(token)
            .header("AuthorizationType", "ilink_bot_token")
            .header("X-WECHAT-UIN", random_weixin_uin())
            .json(&body);
        if let Some(route_tag) = route_tag.filter(|value| !value.trim().is_empty()) {
            request = request.header("SKRouteTag", route_tag);
        }

        let response = request
            .send()
            .await
            .context("failed to poll Weixin recipient link message")?;
        let status = response.status();
        let response_body = response
            .text()
            .await
            .context("failed to read Weixin recipient link response")?;
        if !status.is_success() {
            anyhow::bail!(
                "Weixin recipient link poll returned HTTP {}: {}",
                status.as_u16(),
                short_response_body(&response_body)
            );
        }

        let payload: WeixinGetUpdatesResponse = serde_json::from_str(&response_body)
            .context("failed to parse Weixin recipient link response")?;
        if payload.ret != 0 || payload.errcode != 0 {
            anyhow::bail!(
                "Weixin recipient link poll returned ret={} errcode={}: {}",
                payload.ret,
                payload.errcode,
                payload.errmsg
            );
        }
        get_updates_buf = payload.get_updates_buf.unwrap_or_default();

        for message in payload.msgs {
            let from_user_id = message.from_user_id.trim();
            let context_token = message.context_token.trim();
            if message.message_type == Some(2) {
                continue;
            }
            if from_user_id.is_empty() || context_token.is_empty() {
                continue;
            }
            if expected_user_id.is_some_and(|expected| expected != from_user_id) {
                continue;
            }

            return Ok(WeixinRecipientLink {
                recipient_user_id: from_user_id.to_string(),
                context_token: context_token.to_string(),
            });
        }

        sleep(Duration::from_secs(1)).await;
    }

    anyhow::bail!("timed out waiting for a Weixin message with context_token")
}

#[derive(Debug, Deserialize)]
struct WeixinQrCodeResponse {
    qrcode: String,
    qrcode_img_content: String,
}

#[derive(Debug, Deserialize)]
struct WeixinQrStatusResponse {
    #[serde(default)]
    status: String,
    #[serde(default)]
    bot_token: String,
    #[serde(default)]
    ilink_bot_id: String,
    #[serde(default)]
    baseurl: String,
    #[serde(default)]
    ilink_user_id: String,
}

#[derive(Debug, Deserialize)]
struct WeixinGetUpdatesResponse {
    #[serde(default)]
    ret: i64,
    #[serde(default)]
    errcode: i64,
    #[serde(default)]
    errmsg: String,
    #[serde(default)]
    msgs: Vec<WeixinUpdateMessage>,
    #[serde(default)]
    get_updates_buf: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WeixinUpdateMessage {
    #[serde(default)]
    from_user_id: String,
    #[serde(default)]
    context_token: String,
    #[serde(default)]
    message_type: Option<i64>,
}

fn weixin_url(base_url: &str, path: &[&str]) -> anyhow::Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(&resolve_weixin_base_url(base_url)?)?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("Weixin iLink base URL cannot be a base URL"))?
        .extend(path);
    Ok(url)
}

fn short_response_body(body: &str) -> String {
    let body = body.trim();
    if body.chars().count() <= 256 {
        return body.to_string();
    }

    format!("{}...", body.chars().take(256).collect::<String>())
}

fn present_str(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn random_weixin_uin() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    let value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    BASE64_STANDARD.encode(value.to_string())
}

pub fn missing_config_message(path: &str) -> String {
    format!(
        r#"No agents-notifier config found at `{path}`.

Run `agents-notifier setup` in an interactive terminal to choose an agent and a notification provider.

If you are running non-interactively, create the config file first or pass `--config <PATH>`."#
    )
}

fn is_valid_ntfy_topic(topic: &str) -> bool {
    !topic.is_empty()
        && topic
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn is_pushover_identifier(value: &str) -> bool {
    value.len() == 30 && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn webhook_host(url: &str) -> String {
    host_label(url)
}

fn provider_url_host(provider: &ProviderConfig) -> Option<String> {
    match (
        provider
            .url
            .as_deref()
            .filter(|value| !value.trim().is_empty()),
        provider
            .url_env
            .as_deref()
            .filter(|value| !value.trim().is_empty()),
    ) {
        (Some(url), _) => Some(webhook_host(url)),
        (None, Some(env_name)) => Some(format!("env:{env_name}")),
        (None, None) => None,
    }
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
        let config = build_ntfy_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "agents-notifier-test",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(parsed.notification.answer_detail, AnswerDetail::Preview);
        assert_eq!(
            parsed
                .provider("phone")
                .and_then(|provider| provider.topic.as_deref()),
            Some("agents-notifier-test")
        );
        assert!(parsed.source("codex_desktop").is_some());
        assert!(parsed.source("agents_notifier").is_some());
        assert!(parsed.source("codex_cli").is_none());
        assert_eq!(
            parsed.routes[0].sources,
            vec!["codex_desktop".to_string(), "agents_notifier".to_string()]
        );
    }

    #[test]
    fn writes_parseable_full_answer_detail_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_webhook_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Full,
            PromptDetail::Off,
            "https://example.com/hook",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(parsed.notification.answer_detail, AnswerDetail::Full);
    }

    #[test]
    fn writes_parseable_on_prompt_detail_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_webhook_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::On,
            "https://example.com/hook",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(parsed.notification.prompt_detail, PromptDetail::On);
    }

    #[test]
    fn writes_parseable_codex_cli_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_ntfy_config(
            AgentSelection::CodexCli,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "agents-notifier-test",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert!(parsed.source("codex_cli").is_some());
        assert!(parsed.source("agents_notifier").is_some());
        assert!(parsed.source("codex_desktop").is_none());
        assert_eq!(
            parsed.routes[0].sources,
            vec!["codex_cli".to_string(), "agents_notifier".to_string()]
        );
    }

    #[test]
    fn writes_parseable_claude_code_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_ntfy_config(
            AgentSelection::ClaudeCode,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "agents-notifier-test",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert!(parsed.source("claude_code").is_some());
        assert!(parsed.source("agents_notifier").is_some());
        assert!(parsed.source("codex_desktop").is_none());
        assert!(parsed.source("codex_cli").is_none());
        assert_eq!(
            parsed.routes[0].sources,
            vec!["claude_code".to_string(), "agents_notifier".to_string()]
        );
    }

    #[test]
    fn writes_parseable_agent_hook_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_ntfy_config(
            AgentSelection::OpenCodeCli,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "agents-notifier-test",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        let source = parsed
            .source("opencode_cli")
            .expect("OpenCode CLI source should be configured");
        assert_eq!(source.source_type, SourceType::AgentHook);
        assert!(parsed.source("agents_notifier").is_some());
        assert!(parsed.source("codex_desktop").is_none());
        assert!(parsed.source("codex_cli").is_none());
        assert_eq!(
            parsed.routes[0].sources,
            vec!["opencode_cli".to_string(), "agents_notifier".to_string()]
        );
    }

    #[test]
    fn writes_parseable_feishu_lark_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_feishu_lark_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
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
        assert!(parsed.source("agents_notifier").is_some());
        assert!(parsed.source("codex_cli").is_none());
    }

    #[test]
    fn writes_parseable_webhook_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_webhook_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "https://example.com/hook",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("webhook")
                .and_then(|provider| provider.url.as_deref()),
            Some("https://example.com/hook")
        );
        assert!(parsed.source("codex_desktop").is_some());
        assert!(parsed.source("agents_notifier").is_some());
        assert!(parsed.source("codex_cli").is_none());
    }

    #[test]
    fn writes_parseable_pushover_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_pushover_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "123456789012345678901234567890",
            "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ",
            Some("iphone".to_string()),
            Some("pushover".to_string()),
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("pushover")
                .and_then(|provider| provider.app_token.as_deref()),
            Some("123456789012345678901234567890")
        );
        assert_eq!(
            parsed
                .provider("pushover")
                .and_then(|provider| provider.user_key.as_deref()),
            Some("ABCDEFGHIJABCDEFGHIJABCDEFGHIJ")
        );
        assert!(parsed.source("codex_desktop").is_some());
        assert!(parsed.source("agents_notifier").is_some());
        assert!(parsed.source("codex_cli").is_none());
    }

    #[test]
    fn writes_parseable_slack_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_slack_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            &slack_test_url(),
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("slack")
                .and_then(|provider| provider.url.as_deref()),
            Some(slack_test_url().as_str())
        );
        assert!(parsed.source("codex_desktop").is_some());
        assert!(parsed.source("agents_notifier").is_some());
        assert!(parsed.source("codex_cli").is_none());
    }

    #[test]
    fn writes_parseable_discord_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_discord_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "https://discord.com/api/webhooks/123456789012345678/token",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("discord")
                .and_then(|provider| provider.url.as_deref()),
            Some("https://discord.com/api/webhooks/123456789012345678/token")
        );
        assert!(parsed.source("codex_desktop").is_some());
        assert!(parsed.source("agents_notifier").is_some());
        assert!(parsed.source("codex_cli").is_none());
    }

    #[test]
    fn writes_parseable_telegram_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_telegram_config(
            AgentSelection::GithubCopilotCli,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "123456:test-token",
            "123456789",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("telegram")
                .and_then(|provider| provider.chat_id.as_deref()),
            Some("123456789")
        );
        assert_eq!(
            parsed
                .provider("telegram")
                .and_then(|provider| provider.bot_token.as_deref()),
            Some("123456:test-token")
        );
        let source = parsed
            .source("github_copilot_cli")
            .expect("GitHub Copilot CLI source should be configured");
        assert_eq!(source.source_type, SourceType::AgentHook);
    }

    #[test]
    fn writes_parseable_whatsapp_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_whatsapp_config(
            AgentSelection::GeminiCli,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "test-access-token",
            "123456789",
            "15551234567",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("whatsapp")
                .and_then(|provider| provider.phone_number_id.as_deref()),
            Some("123456789")
        );
        assert_eq!(
            parsed
                .provider("whatsapp")
                .and_then(|provider| provider.recipient_phone_number.as_deref()),
            Some("15551234567")
        );
        let source = parsed
            .source("gemini_cli")
            .expect("Gemini CLI source should be configured");
        assert_eq!(source.source_type, SourceType::AgentHook);
    }

    #[test]
    fn writes_parseable_weixin_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_weixin_config(
            AgentSelection::Aider,
            AnswerDetail::Preview,
            PromptDetail::Off,
            DEFAULT_WEIXIN_BASE_URL,
            "test-token",
            "user@im.wechat",
            "test-context-token",
            Some("test-route".to_string()),
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("weixin")
                .and_then(|provider| provider.base_url.as_deref()),
            Some(DEFAULT_WEIXIN_BASE_URL)
        );
        assert_eq!(
            parsed
                .provider("weixin")
                .and_then(|provider| provider.recipient_user_id.as_deref()),
            Some("user@im.wechat")
        );
        let source = parsed
            .source("aider")
            .expect("Aider source should be configured");
        assert_eq!(source.source_type, SourceType::AgentHook);
    }

    #[test]
    fn writes_parseable_microsoft_teams_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_microsoft_teams_config(
            AgentSelection::Aider,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "https://example.com/workflow?sig=secret",
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        assert_eq!(
            parsed
                .provider("microsoft_teams")
                .and_then(|provider| provider.url.as_deref()),
            Some("https://example.com/workflow?sig=secret")
        );
        let source = parsed
            .source("aider")
            .expect("Aider source should be configured");
        assert_eq!(source.source_type, SourceType::AgentHook);
    }

    #[test]
    fn writes_parseable_email_smtp_config() {
        let dir = tempdir().expect("tempdir should be created");
        let path = dir.path().join("config.toml");
        let config = build_email_smtp_config(
            AgentSelection::CodexCli,
            AnswerDetail::Full,
            PromptDetail::On,
            "smtp.example.com",
            587,
            EmailSmtpSecurity::Starttls,
            Some("alerts@example.com".to_string()),
            Some("smtp-password".to_string()),
            "Agents Notifier <alerts@example.com>",
            vec!["Felix <felix@example.com>".to_string()],
            Some("reply@example.com".to_string()),
        );

        write_config(&path, &config).expect("config should be written");

        let parsed = Config::from_path(&path).expect("written config should parse");
        let provider = parsed
            .provider("email")
            .expect("email provider should be configured");
        assert_eq!(provider.host.as_deref(), Some("smtp.example.com"));
        assert_eq!(provider.port, Some(587));
        assert_eq!(provider.security, Some(EmailSmtpSecurity::Starttls));
        assert_eq!(
            provider.to.as_ref().expect("recipients should exist"),
            &vec!["Felix <felix@example.com>".to_string()]
        );
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
    fn accepts_https_and_local_webhook_urls() {
        assert_eq!(
            resolve_webhook_url("https://example.com/agents-notifier")
                .expect("HTTPS webhook should be valid"),
            "https://example.com/agents-notifier"
        );
        assert_eq!(
            resolve_webhook_url("http://127.0.0.1:8080/hook")
                .expect("local HTTP webhook should be valid"),
            "http://127.0.0.1:8080/hook"
        );
    }

    #[test]
    fn accepts_official_slack_and_discord_webhook_urls() {
        assert_eq!(
            resolve_slack_webhook_url(&slack_test_url()).expect("Slack webhook should be valid"),
            slack_test_url()
        );
        assert_eq!(
            resolve_discord_webhook_url(
                "https://discord.com/api/webhooks/123456789012345678/token"
            )
            .expect("Discord webhook should be valid"),
            "https://discord.com/api/webhooks/123456789012345678/token"
        );
    }

    #[test]
    fn accepts_telegram_whatsapp_weixin_and_microsoft_teams_inputs() {
        assert_eq!(
            resolve_telegram_bot_token("123456:test-token")
                .expect("Telegram bot token should be valid"),
            "123456:test-token"
        );
        assert_eq!(
            resolve_telegram_chat_id("@agents_notifier").expect("Telegram chat id should be valid"),
            "@agents_notifier"
        );
        assert_eq!(
            resolve_whatsapp_access_token("test-access-token")
                .expect("WhatsApp access token should be valid"),
            "test-access-token"
        );
        assert_eq!(
            resolve_whatsapp_phone_number_id("123456789")
                .expect("WhatsApp phone number id should be valid"),
            "123456789"
        );
        assert_eq!(
            resolve_whatsapp_recipient_phone_number("15551234567")
                .expect("WhatsApp recipient should be valid"),
            "15551234567"
        );
        assert_eq!(
            resolve_weixin_base_url(DEFAULT_WEIXIN_BASE_URL)
                .expect("Weixin base URL should be valid"),
            DEFAULT_WEIXIN_BASE_URL
        );
        assert_eq!(
            resolve_weixin_token("test-token").expect("Weixin token should be valid"),
            "test-token"
        );
        assert_eq!(
            resolve_weixin_recipient_user_id("user@im.wechat")
                .expect("Weixin recipient user id should be valid"),
            "user@im.wechat"
        );
        assert_eq!(
            resolve_weixin_context_token("test-context-token")
                .expect("Weixin context token should be valid"),
            "test-context-token"
        );
        assert_eq!(
            resolve_weixin_route_tag("test-route").expect("Weixin route tag should be valid"),
            Some("test-route".to_string())
        );
        assert_eq!(
            resolve_microsoft_teams_webhook_url("https://example.com/workflow?sig=secret")
                .expect("Microsoft Teams webhook should be valid"),
            "https://example.com/workflow?sig=secret"
        );
    }

    #[test]
    fn accepts_email_smtp_inputs() {
        assert_eq!(
            resolve_email_smtp_host("smtp.example.com").expect("SMTP host should be valid"),
            "smtp.example.com"
        );
        assert_eq!(
            resolve_email_smtp_port("587").expect("SMTP port should be valid"),
            587
        );
        assert_eq!(
            resolve_email_smtp_security("starttls").expect("SMTP security should be valid"),
            EmailSmtpSecurity::Starttls
        );
        assert_eq!(
            resolve_email_smtp_optional_username("alerts@example.com")
                .expect("SMTP username should be valid"),
            Some("alerts@example.com".to_string())
        );
        assert_eq!(
            resolve_email_smtp_password("smtp-password").expect("SMTP password should be valid"),
            "smtp-password"
        );
        assert_eq!(
            resolve_email_smtp_mailbox("Agents Notifier <alerts@example.com>", "from")
                .expect("SMTP mailbox should be valid"),
            "Agents Notifier <alerts@example.com>"
        );
        assert_eq!(
            resolve_email_smtp_recipients("Felix <felix@example.com>, team@example.com")
                .expect("SMTP recipients should be valid"),
            vec![
                "Felix <felix@example.com>".to_string(),
                "team@example.com".to_string()
            ]
        );
    }

    #[test]
    fn accepts_valid_pushover_identifiers_and_device() {
        assert_eq!(
            resolve_pushover_app_token("123456789012345678901234567890")
                .expect("app token should be valid"),
            "123456789012345678901234567890"
        );
        assert_eq!(
            resolve_pushover_user_key("ABCDEFGHIJABCDEFGHIJABCDEFGHIJ")
                .expect("user key should be valid"),
            "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ"
        );
        assert_eq!(
            resolve_pushover_device("iphone,work_mac").expect("device should be valid"),
            Some("iphone,work_mac".to_string())
        );
    }

    #[test]
    fn rejects_invalid_pushover_identifier_and_device() {
        let token_err =
            resolve_pushover_app_token("too-short").expect_err("short app token should fail");
        assert!(token_err.to_string().contains("30 letters or numbers"));

        let device_err =
            resolve_pushover_device("bad/device").expect_err("bad device name should fail");
        assert!(
            device_err
                .to_string()
                .contains("comma-separated device names")
        );
    }

    #[test]
    fn rejects_insecure_remote_webhook_url() {
        let err = resolve_webhook_url("http://example.com/hook")
            .expect_err("remote HTTP webhook should fail");

        assert!(err.to_string().contains("must use HTTPS"));
    }

    #[test]
    fn rejects_webhook_url_with_basic_auth() {
        let err = resolve_webhook_url("https://user:pass@example.com/hook")
            .expect_err("webhook URL credentials should fail");

        assert!(err.to_string().contains("must not include username"));
    }

    #[test]
    fn extracts_ntfy_subscriptions_from_config() {
        let config = build_ntfy_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "agents-notifier-test",
        );

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
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
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
    fn extracts_webhook_targets_without_printing_full_url() {
        let config = build_webhook_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "https://example.com/secret-token",
        );

        let targets = webhook_targets(&config);

        assert_eq!(
            targets,
            vec![WebhookTarget {
                provider_id: "webhook".to_string(),
                webhook_host: "example.com".to_string(),
            }]
        );
    }

    #[test]
    fn extracts_pushover_targets_without_printing_private_keys() {
        let config = build_pushover_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "123456789012345678901234567890",
            "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ",
            Some("iphone".to_string()),
            None,
        );

        let targets = pushover_targets(&config);

        assert_eq!(
            targets,
            vec![PushoverTarget {
                provider_id: "pushover".to_string(),
                device: Some("iphone".to_string()),
                sound: None,
            }]
        );
    }

    #[test]
    fn extracts_slack_and_discord_targets_without_printing_webhook_tokens() {
        let slack_config = build_slack_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            &slack_test_url(),
        );
        let discord_config = build_discord_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "https://discord.com/api/webhooks/123456789012345678/token",
        );

        assert_eq!(
            slack_targets(&slack_config),
            vec![SlackTarget {
                provider_id: "slack".to_string(),
                webhook_host: "hooks.slack.com".to_string(),
            }]
        );
        assert_eq!(
            discord_targets(&discord_config),
            vec![DiscordTarget {
                provider_id: "discord".to_string(),
                webhook_host: "discord.com".to_string(),
            }]
        );
    }

    #[test]
    fn extracts_new_provider_targets_without_printing_private_tokens() {
        let telegram_config = build_telegram_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "123456:test-token",
            "@agents_notifier",
        );
        let whatsapp_config = build_whatsapp_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "test-access-token",
            "123456789",
            "15551234567",
        );
        let weixin_config = build_weixin_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            DEFAULT_WEIXIN_BASE_URL,
            "test-token",
            "user@im.wechat",
            "test-context-token",
            None,
        );
        let teams_config = build_microsoft_teams_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "https://example.com/workflow?sig=secret",
        );
        let email_config = build_email_smtp_config(
            AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "smtp.example.com",
            587,
            EmailSmtpSecurity::Starttls,
            Some("alerts@example.com".to_string()),
            Some("smtp-password".to_string()),
            "Agents Notifier <alerts@example.com>",
            vec!["Felix <felix@example.com>".to_string()],
            None,
        );

        assert_eq!(
            telegram_targets(&telegram_config),
            vec![TelegramTarget {
                provider_id: "telegram".to_string(),
                chat_id: "@agents_notifier".to_string(),
            }]
        );
        assert_eq!(
            whatsapp_targets(&whatsapp_config),
            vec![WhatsappTarget {
                provider_id: "whatsapp".to_string(),
                recipient_phone_number: "15551234567".to_string(),
            }]
        );
        assert_eq!(
            weixin_targets(&weixin_config),
            vec![WeixinTarget {
                provider_id: "weixin".to_string(),
                base_url_host: "ilinkai.weixin.qq.com".to_string(),
                recipient_user_id: "user@im.wechat".to_string(),
            }]
        );
        assert_eq!(
            microsoft_teams_targets(&teams_config),
            vec![MicrosoftTeamsTarget {
                provider_id: "microsoft_teams".to_string(),
                webhook_host: "example.com".to_string(),
            }]
        );
        assert_eq!(
            email_smtp_targets(&email_config),
            vec![EmailSmtpTarget {
                provider_id: "email".to_string(),
                host: "smtp.example.com".to_string(),
                port: 587,
                from: "Agents Notifier <alerts@example.com>".to_string(),
                to: vec!["Felix <felix@example.com>".to_string()],
            }]
        );
    }

    #[test]
    fn skipped_test_notification_message_confirms_running_service() {
        assert!(TEST_NOTIFICATION_SKIPPED_MESSAGE.contains("Service is running"));
        assert!(TEST_NOTIFICATION_SKIPPED_MESSAGE.contains("No test notification was sent"));
    }

    fn slack_test_url() -> String {
        format!(
            "https://hooks.slack.com/services/{}/{}/{}",
            "T00000000", "B00000000", "test-token"
        )
    }
}
