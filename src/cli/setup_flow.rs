use super::*;

mod output;
mod prompts;

pub(super) use output::*;
pub(super) use prompts::*;

pub(super) struct LoadedConfig {
    pub(super) config: Config,
    pub(super) guided_setup: Option<GuidedSetup>,
}

pub(super) enum GuidedSetup {
    Ntfy {
        agent: setup::AgentSelection,
        topic: String,
    },
    FeishuLark {
        agent: setup::AgentSelection,
    },
    Webhook {
        agent: setup::AgentSelection,
    },
    Pushover {
        agent: setup::AgentSelection,
    },
    Slack {
        agent: setup::AgentSelection,
    },
    Discord {
        agent: setup::AgentSelection,
    },
    Telegram {
        agent: setup::AgentSelection,
    },
    Whatsapp {
        agent: setup::AgentSelection,
    },
    Wechat {
        agent: setup::AgentSelection,
    },
    MicrosoftTeams {
        agent: setup::AgentSelection,
    },
    EmailSmtp {
        agent: setup::AgentSelection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InitialProvider {
    Ntfy,
    FeishuLark,
    Webhook,
    Pushover,
    Slack,
    Discord,
    Telegram,
    Whatsapp,
    Wechat,
    MicrosoftTeams,
    EmailSmtp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WechatSetupMethod {
    QrLogin,
    ExistingToken,
}

pub(super) struct WechatLogin {
    token: String,
    base_url: String,
    scanned_user_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SetupDefaults {
    pub(super) language: Option<CliLanguage>,
    pub(super) agent: Option<setup::AgentSelection>,
    pub(super) answer_detail: Option<AnswerDetail>,
    pub(super) prompt_detail: Option<PromptDetail>,
    pub(super) provider: Option<InitialProvider>,
    pub(super) ntfy_topic: Option<String>,
    pub(super) feishu_lark_webhook_url: Option<String>,
    pub(super) feishu_lark_secret: Option<String>,
    pub(super) webhook_url: Option<String>,
    pub(super) pushover_app_token: Option<String>,
    pub(super) pushover_user_key: Option<String>,
    pub(super) pushover_device: Option<String>,
    pub(super) pushover_sound: Option<String>,
    pub(super) slack_webhook_url: Option<String>,
    pub(super) discord_webhook_url: Option<String>,
    pub(super) telegram_bot_token: Option<String>,
    pub(super) telegram_chat_id: Option<String>,
    pub(super) whatsapp_access_token: Option<String>,
    pub(super) whatsapp_phone_number_id: Option<String>,
    pub(super) whatsapp_recipient_phone_number: Option<String>,
    pub(super) wechat_base_url: Option<String>,
    pub(super) wechat_token: Option<String>,
    pub(super) wechat_recipient_user_id: Option<String>,
    pub(super) wechat_context_token: Option<String>,
    pub(super) wechat_route_tag: Option<String>,
    pub(super) microsoft_teams_webhook_url: Option<String>,
    pub(super) email_smtp_host: Option<String>,
    pub(super) email_smtp_port: Option<u16>,
    pub(super) email_smtp_security: Option<EmailSmtpSecurity>,
    pub(super) email_smtp_username: Option<String>,
    pub(super) email_smtp_password: Option<String>,
    pub(super) email_smtp_from: Option<String>,
    pub(super) email_smtp_to: Option<Vec<String>>,
    pub(super) email_smtp_reply_to: Option<String>,
}

impl SetupDefaults {
    pub(super) fn from_config(config: &Config) -> Self {
        Self {
            language: Some(config.cli.language),
            agent: first_configured_agent(config),
            answer_detail: Some(config.notification.answer_detail),
            prompt_detail: Some(config.notification.prompt_detail),
            provider: first_configured_provider(config),
            ntfy_topic: first_provider_of_type(config, ProviderType::Ntfy)
                .and_then(|provider| provider.topic.clone()),
            feishu_lark_webhook_url: first_provider_of_type(config, ProviderType::FeishuLark)
                .and_then(configured_provider_url),
            feishu_lark_secret: first_provider_of_type(config, ProviderType::FeishuLark)
                .and_then(configured_provider_secret),
            webhook_url: first_provider_of_type(config, ProviderType::Webhook)
                .and_then(configured_provider_url),
            pushover_app_token: first_provider_of_type(config, ProviderType::Pushover)
                .and_then(|provider| provider.app_token.clone()),
            pushover_user_key: first_provider_of_type(config, ProviderType::Pushover)
                .and_then(|provider| provider.user_key.clone()),
            pushover_device: first_provider_of_type(config, ProviderType::Pushover)
                .and_then(|provider| provider.device.clone()),
            pushover_sound: first_provider_of_type(config, ProviderType::Pushover)
                .and_then(|provider| provider.sound.clone()),
            slack_webhook_url: first_provider_of_type(config, ProviderType::Slack)
                .and_then(configured_provider_url),
            discord_webhook_url: first_provider_of_type(config, ProviderType::Discord)
                .and_then(configured_provider_url),
            telegram_bot_token: first_provider_of_type(config, ProviderType::Telegram)
                .and_then(|provider| provider.bot_token.clone()),
            telegram_chat_id: first_provider_of_type(config, ProviderType::Telegram)
                .and_then(|provider| provider.chat_id.clone()),
            whatsapp_access_token: first_provider_of_type(config, ProviderType::Whatsapp)
                .and_then(|provider| provider.access_token.clone()),
            whatsapp_phone_number_id: first_provider_of_type(config, ProviderType::Whatsapp)
                .and_then(|provider| provider.phone_number_id.clone()),
            whatsapp_recipient_phone_number: first_provider_of_type(config, ProviderType::Whatsapp)
                .and_then(|provider| provider.recipient_phone_number.clone()),
            wechat_base_url: first_provider_of_type(config, ProviderType::Wechat)
                .and_then(|provider| provider.base_url.clone()),
            wechat_token: first_provider_of_type(config, ProviderType::Wechat)
                .and_then(|provider| provider.token.clone()),
            wechat_recipient_user_id: first_provider_of_type(config, ProviderType::Wechat)
                .and_then(|provider| provider.recipient_user_id.clone()),
            wechat_context_token: first_provider_of_type(config, ProviderType::Wechat)
                .and_then(|provider| provider.context_token.clone()),
            wechat_route_tag: first_provider_of_type(config, ProviderType::Wechat)
                .and_then(|provider| provider.route_tag.clone()),
            microsoft_teams_webhook_url: first_provider_of_type(
                config,
                ProviderType::MicrosoftTeams,
            )
            .and_then(configured_provider_url),
            email_smtp_host: first_provider_of_type(config, ProviderType::EmailSmtp)
                .and_then(|provider| provider.host.clone()),
            email_smtp_port: first_provider_of_type(config, ProviderType::EmailSmtp)
                .and_then(|provider| provider.port),
            email_smtp_security: first_provider_of_type(config, ProviderType::EmailSmtp)
                .and_then(|provider| provider.security),
            email_smtp_username: first_provider_of_type(config, ProviderType::EmailSmtp)
                .and_then(|provider| provider.username.clone()),
            email_smtp_password: first_provider_of_type(config, ProviderType::EmailSmtp)
                .and_then(|provider| provider.password.clone()),
            email_smtp_from: first_provider_of_type(config, ProviderType::EmailSmtp)
                .and_then(|provider| provider.from.clone()),
            email_smtp_to: first_provider_of_type(config, ProviderType::EmailSmtp)
                .and_then(|provider| provider.to.clone()),
            email_smtp_reply_to: first_provider_of_type(config, ProviderType::EmailSmtp)
                .and_then(|provider| provider.reply_to.clone()),
        }
    }

    fn from_path(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let config = Config::from_path(path)?;
        Ok(Self::from_config(&config))
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ConfigWriteMode {
    Created,
    Updated,
}

impl ConfigWriteMode {
    fn past_tense(self, i18n: I18n) -> &'static str {
        match self {
            Self::Created => i18n.text(Text::ConfigCreated),
            Self::Updated => i18n.text(Text::ConfigUpdated),
        }
    }
}

impl InitialProvider {
    fn display_name(self) -> &'static str {
        match self {
            Self::Ntfy => "ntfy",
            Self::FeishuLark => "Feishu/Lark custom bot",
            Self::Webhook => "Webhook",
            Self::Pushover => "Pushover",
            Self::Slack => "Slack",
            Self::Discord => "Discord",
            Self::Telegram => "Telegram",
            Self::Whatsapp => "WhatsApp",
            Self::Wechat => "WeChat",
            Self::MicrosoftTeams => "Microsoft Teams",
            Self::EmailSmtp => "Email SMTP",
        }
    }
}

pub(super) async fn load_config(path: &Path, allow_setup: bool) -> anyhow::Result<LoadedConfig> {
    match Config::from_path(path) {
        Ok(config) => Ok(LoadedConfig {
            config,
            guided_setup: None,
        }),
        Err(ConfigError::NotFound { path }) if allow_setup && is_interactive_terminal() => {
            run_first_start_setup(Path::new(&path)).await
        }
        Err(ConfigError::NotFound { path }) => {
            anyhow::bail!("{}", setup::missing_config_message(&path))
        }
        Err(error) => Err(error).context("config load failed"),
    }
}

pub(super) fn is_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub(super) fn prompt_theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

pub(super) fn prompt_text(
    prompt: impl Into<String>,
    read_context: impl Into<String>,
) -> anyhow::Result<String> {
    let theme = prompt_theme();
    let read_context = read_context.into();
    Input::<String>::with_theme(&theme)
        .with_prompt(prompt.into())
        .allow_empty(true)
        .interact()
        .with_context(|| read_context)
}

pub(super) fn prompt_private_text(
    prompt: impl Into<String>,
    initial_text: Option<&str>,
    read_context: impl Into<String>,
) -> anyhow::Result<String> {
    let theme = prompt_theme();
    let read_context = read_context.into();
    let input = Input::<String>::with_theme(&theme)
        .with_prompt(prompt.into())
        .allow_empty(true)
        .report(false);
    let input = if let Some(initial_text) = initial_text {
        input.with_initial_text(initial_text)
    } else {
        input
    };

    input.interact().with_context(|| read_context)
}

pub(super) fn prompt_secret(
    prompt: impl Into<String>,
    read_context: impl Into<String>,
) -> anyhow::Result<String> {
    let theme = prompt_theme();
    let read_context = read_context.into();
    Password::with_theme(&theme)
        .with_prompt(prompt.into())
        .allow_empty_password(true)
        .report(false)
        .interact()
        .with_context(|| read_context)
}

pub(super) fn prompt_confirm(prompt: &str, default: bool) -> anyhow::Result<bool> {
    let theme = prompt_theme();
    Confirm::with_theme(&theme)
        .with_prompt(prompt)
        .default(default)
        .interact()
        .context("failed to read confirmation")
}

pub(super) fn prompt_for_language(default: Option<CliLanguage>) -> anyhow::Result<CliLanguage> {
    let effective_default = language_default(default)?;
    let options = [CliLanguage::English, CliLanguage::SimplifiedChinese];
    let default_index = options
        .iter()
        .position(|language| *language == effective_default)
        .unwrap_or(0);
    let items = options
        .iter()
        .map(|language| language.display_name())
        .collect::<Vec<_>>();
    let theme = prompt_theme();
    let selection = Select::with_theme(&theme)
        .with_prompt(I18n::new(effective_default).text(Text::LanguagePrompt))
        .items(&items)
        .default(default_index)
        .interact()
        .context("failed to read language choice")?;

    Ok(options[selection])
}

pub(super) fn language_default(default: Option<CliLanguage>) -> anyhow::Result<CliLanguage> {
    if let Ok(value) = std::env::var("AGENTS_NOTIFIER_LANGUAGE") {
        let Some(language) = CliLanguage::parse(&value) else {
            anyhow::bail!(
                "AGENTS_NOTIFIER_LANGUAGE must be `en` or `zh-CN`; got `{}`",
                value
            );
        };
        return Ok(language);
    }

    Ok(default.unwrap_or_default())
}

pub(super) fn localized(
    i18n: I18n,
    english: &'static str,
    simplified_chinese: &'static str,
) -> &'static str {
    match i18n.language() {
        CliLanguage::English => english,
        CliLanguage::SimplifiedChinese => simplified_chinese,
    }
}

pub(super) fn localized_string(i18n: I18n, english: String, simplified_chinese: String) -> String {
    match i18n.language() {
        CliLanguage::English => english,
        CliLanguage::SimplifiedChinese => simplified_chinese,
    }
}

pub(super) fn write_setup_config(
    path: &Path,
    config: &mut Config,
    language: CliLanguage,
) -> anyhow::Result<()> {
    config.cli = CliConfig::new(language);
    setup::write_config(path, config)
}

pub(super) async fn run_first_start_setup(path: &Path) -> anyhow::Result<LoadedConfig> {
    let language = prompt_for_language(None)?;
    let i18n = I18n::new(language);

    match language {
        CliLanguage::English => {
            println!("No Agents Notifier config found at `{}`.", path.display())
        }
        CliLanguage::SimplifiedChinese => {
            println!("没有找到 Agents Notifier config：`{}`。", path.display())
        }
    }
    println!();
    println!("{}", i18n.text(Text::FirstStartStartingSetup));
    println!("{}", i18n.text(Text::FirstStartSetupScope));
    println!();

    run_guided_provider_setup(
        path,
        ConfigWriteMode::Created,
        SetupDefaults {
            language: Some(language),
            ..SetupDefaults::default()
        },
        i18n,
    )
    .await
}

pub(super) async fn run_provider_setup(path: &Path) -> anyhow::Result<LoadedConfig> {
    if !is_interactive_terminal() {
        anyhow::bail!("{}", I18n::default().text(Text::SetupInteractiveRequired));
    }

    let mode = if path.exists() {
        ConfigWriteMode::Updated
    } else {
        ConfigWriteMode::Created
    };
    let mut defaults = SetupDefaults::from_path(path)?;
    let language = prompt_for_language(defaults.language)?;
    defaults.language = Some(language);
    let i18n = I18n::new(language);

    print_heading(i18n.text(Text::SetupTitle));
    match language {
        CliLanguage::English => println!(
            "This replaces the notification, agent, provider, and route sections in `{}`.",
            path.display()
        ),
        CliLanguage::SimplifiedChinese => println!(
            "这会替换 `{}` 里的 notification、agent、provider 和 route 配置。",
            path.display()
        ),
    }
    println!("{}", i18n.text(Text::SetupDoesNotChangeAgentSettings));
    println!();

    run_guided_provider_setup(path, mode, defaults, i18n).await
}

pub(super) async fn run_guided_provider_setup(
    path: &Path,
    mode: ConfigWriteMode,
    defaults: SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    let agent = prompt_for_agent(defaults.agent, i18n)?;
    let provider = prompt_for_initial_provider(defaults.provider, i18n)?;
    let answer_detail = answer_detail_for_provider(provider, defaults.answer_detail, i18n)?;
    let prompt_detail = prompt_detail_for_provider(provider, defaults.prompt_detail, i18n)?;

    match provider {
        InitialProvider::Ntfy => run_ntfy_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
        InitialProvider::FeishuLark => run_feishu_lark_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
        InitialProvider::Webhook => run_webhook_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
        InitialProvider::Pushover => run_pushover_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
        InitialProvider::Slack => run_slack_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
        InitialProvider::Discord => run_discord_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
        InitialProvider::Telegram => run_telegram_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
        InitialProvider::Whatsapp => run_whatsapp_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
        InitialProvider::Wechat => {
            run_wechat_setup(
                path,
                mode,
                agent,
                answer_detail,
                prompt_detail,
                &defaults,
                i18n,
            )
            .await
        }
        InitialProvider::MicrosoftTeams => run_microsoft_teams_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
        InitialProvider::EmailSmtp => run_email_smtp_setup(
            path,
            mode,
            agent,
            answer_detail,
            prompt_detail,
            &defaults,
            i18n,
        ),
    }
}

pub(super) fn answer_detail_for_provider(
    provider: InitialProvider,
    default: Option<AnswerDetail>,
    i18n: I18n,
) -> anyhow::Result<AnswerDetail> {
    match provider {
        InitialProvider::Ntfy => {
            println!();
            println!(
                "{}",
                fixed_answer_detail_message(
                    i18n,
                    "ntfy",
                    "ntfy has a documented message size limit",
                    "ntfy 有明确的消息长度限制"
                )
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Pushover => {
            println!();
            println!(
                "{}",
                fixed_answer_detail_message(
                    i18n,
                    "Pushover",
                    "Pushover messages are limited to 1024 characters",
                    "Pushover 消息最多 1024 字符"
                )
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Slack => {
            println!();
            println!(
                "{}",
                fixed_answer_detail_message(
                    i18n,
                    "Slack",
                    "Slack has documented message length and truncation limits",
                    "Slack 有明确的消息长度和截断限制"
                )
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Discord => {
            println!();
            println!(
                "{}",
                fixed_answer_detail_message(
                    i18n,
                    "Discord",
                    "Discord webhook content is limited to 2000 characters",
                    "Discord webhook 内容最多 2000 字符"
                )
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Telegram => {
            println!();
            println!(
                "{}",
                fixed_answer_detail_message(
                    i18n,
                    "Telegram",
                    "Telegram Bot API text messages are limited to 4096 characters",
                    "Telegram Bot API 文本消息最多 4096 字符"
                )
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Whatsapp => {
            println!();
            println!(
                "{}",
                fixed_answer_detail_message(
                    i18n,
                    "WhatsApp",
                    "Agents Notifier uses a 4096-character delivery guard for WhatsApp text bodies",
                    "Agents Notifier 对 WhatsApp 文本使用 4096 字符发送保护线"
                )
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Wechat => {
            println!();
            println!(
                "{}",
                fixed_answer_detail_message(
                    i18n,
                    localized(i18n, "WeChat", "微信"),
                    "Agents Notifier uses a 3800-character delivery guard for WeChat iLink text messages",
                    "Agents Notifier 对微信 iLink 文本消息使用 3800 字符发送保护线"
                )
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::MicrosoftTeams => {
            println!();
            println!(
                "{}",
                fixed_answer_detail_message(
                    i18n,
                    "Microsoft Teams",
                    "incoming webhook messages have a 28 KB size limit",
                    "incoming webhook 消息有 28 KB 大小限制"
                )
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::FeishuLark | InitialProvider::Webhook | InitialProvider::EmailSmtp => {
            prompt_for_answer_detail(default, i18n)
        }
    }
}

pub(super) fn prompt_detail_for_provider(
    provider: InitialProvider,
    default: Option<PromptDetail>,
    i18n: I18n,
) -> anyhow::Result<PromptDetail> {
    match provider {
        InitialProvider::Ntfy => {
            println!();
            println!(
                "{}",
                fixed_prompt_detail_message(
                    i18n,
                    "ntfy",
                    "ntfy has a documented message size limit",
                    "ntfy 有明确的消息长度限制"
                )
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Pushover => {
            println!();
            println!(
                "{}",
                fixed_prompt_detail_message(
                    i18n,
                    "Pushover",
                    "Pushover messages are limited to 1024 characters",
                    "Pushover 消息最多 1024 字符"
                )
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Slack => {
            println!();
            println!(
                "{}",
                fixed_prompt_detail_message(
                    i18n,
                    "Slack",
                    "Slack has documented message length and truncation limits",
                    "Slack 有明确的消息长度和截断限制"
                )
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Discord => {
            println!();
            println!(
                "{}",
                fixed_prompt_detail_message(
                    i18n,
                    "Discord",
                    "Discord webhook content is limited to 2000 characters",
                    "Discord webhook 内容最多 2000 字符"
                )
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Telegram => {
            println!();
            println!(
                "{}",
                fixed_prompt_detail_message(
                    i18n,
                    "Telegram",
                    "Telegram Bot API text messages are limited to 4096 characters",
                    "Telegram Bot API 文本消息最多 4096 字符"
                )
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Whatsapp => {
            println!();
            println!(
                "{}",
                fixed_prompt_detail_message(
                    i18n,
                    "WhatsApp",
                    "Agents Notifier uses a 4096-character delivery guard for WhatsApp text bodies",
                    "Agents Notifier 对 WhatsApp 文本使用 4096 字符发送保护线"
                )
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Wechat => {
            println!();
            println!(
                "{}",
                fixed_prompt_detail_message(
                    i18n,
                    localized(i18n, "WeChat", "微信"),
                    "Agents Notifier uses a 3800-character delivery guard for WeChat iLink text messages",
                    "Agents Notifier 对微信 iLink 文本消息使用 3800 字符发送保护线"
                )
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::MicrosoftTeams => {
            println!();
            println!(
                "{}",
                fixed_prompt_detail_message(
                    i18n,
                    "Microsoft Teams",
                    "incoming webhook messages have a 28 KB size limit",
                    "incoming webhook 消息有 28 KB 大小限制"
                )
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::FeishuLark | InitialProvider::Webhook | InitialProvider::EmailSmtp => {
            prompt_for_prompt_detail(default, i18n)
        }
    }
}

pub(super) fn fixed_answer_detail_message(
    i18n: I18n,
    provider: &str,
    english_reason: &str,
    chinese_reason: &str,
) -> String {
    match i18n.language() {
        CliLanguage::English => format!(
            "Answer detail is fixed to Preview for {provider} because {english_reason}. Agents Notifier will keep {provider} notifications short enough for reliable delivery."
        ),
        CliLanguage::SimplifiedChinese => format!(
            "{provider} 的回答内容固定为 Preview，因为{chinese_reason}。Agents Notifier 会保持通知短小，确保可靠送达。"
        ),
    }
}

pub(super) fn fixed_prompt_detail_message(
    i18n: I18n,
    provider: &str,
    english_reason: &str,
    chinese_reason: &str,
) -> String {
    match i18n.language() {
        CliLanguage::English => format!(
            "Prompt detail is disabled for {provider} because {english_reason}. Agents Notifier will keep prompts out of {provider} notifications for reliable delivery."
        ),
        CliLanguage::SimplifiedChinese => format!(
            "{provider} 已关闭 prompt 内容，因为{chinese_reason}。Agents Notifier 不会把 prompt 放进 {provider} 通知里。"
        ),
    }
}

pub(super) fn run_ntfy_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    let generated_topic = setup::generated_ntfy_topic();

    println!();
    println!("{}", i18n.text(Text::NtfyIntro1));
    println!("{}", i18n.text(Text::NtfyIntro2));
    println!();

    let topic = prompt_for_ntfy_topic(&generated_topic, defaults.ntfy_topic.as_deref(), i18n)?;
    let mut config = setup::build_ntfy_config(agent, answer_detail, prompt_detail, &topic);
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::Ntfy { agent, topic }),
    })
}

pub(super) fn run_feishu_lark_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::FeishuLarkIntro1));
    println!("{}", i18n.text(Text::FeishuLarkIntro2));
    println!("{}", i18n.text(Text::FeishuLarkIntro3));
    println!();

    let webhook_url =
        prompt_for_feishu_lark_webhook_url(defaults.feishu_lark_webhook_url.as_deref(), i18n)?;
    let secret = prompt_for_feishu_lark_secret(defaults.feishu_lark_secret.as_deref(), i18n)?;
    let mut config =
        setup::build_feishu_lark_config(agent, answer_detail, prompt_detail, &webhook_url, secret);
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::FeishuLark { agent }),
    })
}

pub(super) fn run_webhook_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::WebhookIntro1));
    println!("{}", i18n.text(Text::WebhookIntro2));
    println!();

    let webhook_url = prompt_for_webhook_url(defaults.webhook_url.as_deref(), i18n)?;
    let mut config = setup::build_webhook_config(agent, answer_detail, prompt_detail, &webhook_url);
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::Webhook { agent }),
    })
}

pub(super) fn run_pushover_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::PushoverIntro1));
    println!("{}", i18n.text(Text::PushoverIntro2));
    println!();

    let app_token = prompt_for_pushover_app_token(defaults.pushover_app_token.as_deref(), i18n)?;
    let user_key = prompt_for_pushover_user_key(defaults.pushover_user_key.as_deref(), i18n)?;
    let device = prompt_for_pushover_device(defaults.pushover_device.as_deref(), i18n)?;
    let sound = prompt_for_pushover_sound(defaults.pushover_sound.as_deref(), i18n)?;
    let mut config = setup::build_pushover_config(
        agent,
        answer_detail,
        prompt_detail,
        &app_token,
        &user_key,
        device,
        sound,
    );
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::Pushover { agent }),
    })
}

pub(super) fn run_slack_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::SlackIntro1));
    println!("{}", i18n.text(Text::SlackIntro2));
    println!();

    let webhook_url = prompt_for_slack_webhook_url(defaults.slack_webhook_url.as_deref(), i18n)?;
    let mut config = setup::build_slack_config(agent, answer_detail, prompt_detail, &webhook_url);
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::Slack { agent }),
    })
}

pub(super) fn run_discord_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::DiscordIntro1));
    println!("{}", i18n.text(Text::DiscordIntro2));
    println!();

    let webhook_url =
        prompt_for_discord_webhook_url(defaults.discord_webhook_url.as_deref(), i18n)?;
    let mut config = setup::build_discord_config(agent, answer_detail, prompt_detail, &webhook_url);
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::Discord { agent }),
    })
}

pub(super) fn run_telegram_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::TelegramIntro1));
    println!("{}", i18n.text(Text::TelegramIntro2));
    println!();

    let bot_token = prompt_for_telegram_bot_token(defaults.telegram_bot_token.as_deref(), i18n)?;
    let chat_id = prompt_for_telegram_chat_id(defaults.telegram_chat_id.as_deref(), i18n)?;
    let mut config =
        setup::build_telegram_config(agent, answer_detail, prompt_detail, &bot_token, &chat_id);
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::Telegram { agent }),
    })
}

pub(super) fn run_whatsapp_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::WhatsappIntro1));
    println!("{}", i18n.text(Text::WhatsappIntro2));
    println!();

    let access_token =
        prompt_for_whatsapp_access_token(defaults.whatsapp_access_token.as_deref(), i18n)?;
    let phone_number_id =
        prompt_for_whatsapp_phone_number_id(defaults.whatsapp_phone_number_id.as_deref(), i18n)?;
    let recipient_phone_number = prompt_for_whatsapp_recipient_phone_number(
        defaults.whatsapp_recipient_phone_number.as_deref(),
        i18n,
    )?;
    let mut config = setup::build_whatsapp_config(
        agent,
        answer_detail,
        prompt_detail,
        &access_token,
        &phone_number_id,
        &recipient_phone_number,
    );
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::Whatsapp { agent }),
    })
}

pub(super) async fn run_wechat_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::WechatIntro1));
    println!("{}", i18n.text(Text::WechatIntro2));
    println!();

    let mut base_url = prompt_for_wechat_base_url(defaults.wechat_base_url.as_deref(), i18n)?;
    let route_tag = prompt_for_wechat_route_tag(defaults.wechat_route_tag.as_deref(), i18n)?;
    let setup_method = prompt_for_wechat_setup_method(i18n)?;

    let (token, scanned_user_id) = match setup_method {
        WechatSetupMethod::QrLogin => {
            let login = run_wechat_qr_login(&base_url, route_tag.as_deref(), i18n).await?;
            base_url = login.base_url;
            (login.token, login.scanned_user_id)
        }
        WechatSetupMethod::ExistingToken => {
            let token = prompt_for_wechat_token(defaults.wechat_token.as_deref(), i18n)?;
            println!("{}", i18n.text(Text::WechatTokenVerifying));
            setup::verify_wechat_token(&base_url, &token, route_tag.as_deref()).await?;
            println!("{}", style(i18n.text(Text::WechatTokenVerified)).green());
            (token, None)
        }
    };

    let token_changed = defaults.wechat_token.as_deref() != Some(token.as_str());
    let linked_recipient = if let (false, Some(recipient_user_id), Some(context_token)) = (
        token_changed,
        defaults.wechat_recipient_user_id.as_ref(),
        defaults.wechat_context_token.as_ref(),
    ) {
        if prompt_confirm(i18n.text(Text::WechatKeepRecipientPrompt), true)? {
            setup::WechatRecipientLink {
                recipient_user_id: recipient_user_id.clone(),
                context_token: context_token.clone(),
            }
        } else {
            link_wechat_recipient(
                &base_url,
                &token,
                route_tag.as_deref(),
                scanned_user_id.as_deref(),
                i18n,
            )
            .await?
        }
    } else {
        link_wechat_recipient(
            &base_url,
            &token,
            route_tag.as_deref(),
            scanned_user_id.as_deref(),
            i18n,
        )
        .await?
    };

    let mut config = setup::build_wechat_config(
        agent,
        answer_detail,
        prompt_detail,
        &base_url,
        &token,
        &linked_recipient.recipient_user_id,
        &linked_recipient.context_token,
        route_tag,
    );
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::Wechat { agent }),
    })
}

pub(super) fn run_microsoft_teams_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::TeamsIntro1));
    println!("{}", i18n.text(Text::TeamsIntro2));
    println!();

    let webhook_url = prompt_for_microsoft_teams_webhook_url(
        defaults.microsoft_teams_webhook_url.as_deref(),
        i18n,
    )?;
    let mut config =
        setup::build_microsoft_teams_config(agent, answer_detail, prompt_detail, &webhook_url);
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::MicrosoftTeams { agent }),
    })
}

pub(super) fn run_email_smtp_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::EmailIntro1));
    println!("{}", i18n.text(Text::EmailIntro2));
    println!();

    let host = prompt_for_email_smtp_host(defaults.email_smtp_host.as_deref(), i18n)?;
    let security = prompt_for_email_smtp_security(defaults.email_smtp_security, i18n)?;
    let default_port = defaults
        .email_smtp_port
        .unwrap_or_else(|| default_email_smtp_port(security));
    let port = prompt_for_email_smtp_port(default_port, i18n)?;
    let username = prompt_for_email_smtp_username(defaults.email_smtp_username.as_deref(), i18n)?;
    let password = if username.is_some() {
        Some(prompt_for_email_smtp_password(
            defaults.email_smtp_password.as_deref(),
            i18n,
        )?)
    } else {
        None
    };
    let from_label = localized(i18n, "From email", "发件人邮箱");
    let reply_to_label = localized(i18n, "Reply-To email", "Reply-To 邮箱");
    let from =
        prompt_for_email_smtp_mailbox(from_label, defaults.email_smtp_from.as_deref(), i18n)?;
    let to = prompt_for_email_smtp_recipients(defaults.email_smtp_to.as_deref(), i18n)?;
    let reply_to = prompt_for_email_smtp_optional_mailbox(
        reply_to_label,
        defaults.email_smtp_reply_to.as_deref(),
        i18n,
    )?;
    let mut config = setup::build_email_smtp_config(
        agent,
        answer_detail,
        prompt_detail,
        &host,
        port,
        security,
        username,
        password,
        &from,
        to,
        reply_to,
    );
    write_setup_config(path, &mut config, i18n.language())?;

    println!();
    print_setup_summary(mode, path, agent, answer_detail, prompt_detail, i18n);
    print_setup_provider_summary(&config, i18n);

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::EmailSmtp { agent }),
    })
}
