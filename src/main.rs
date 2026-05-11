use std::fmt::Display;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::{Parser, Subcommand};
use console::style;
use dialoguer::{Confirm, Input, Password, Select, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use qrcode::QrCode;
use qrcode::render::unicode;
use tokio::time::sleep;

use agents_notifier::config::{
    AnswerDetail, CliConfig, CliLanguage, Config, ConfigError, EmailSmtpSecurity, PromptDetail,
    ProviderConfig, ProviderType, SourceConfig, SourceType,
};
use agents_notifier::i18n::{I18n, Text};
use agents_notifier::local_ingress::{self, LocalSignalEvent};
use agents_notifier::local_open_bridge;
#[cfg(target_os = "macos")]
use agents_notifier::paths::pid_file_path;
use agents_notifier::paths::{
    app_support_dir_path, default_config_file_path, ingress_endpoint, log_file_path,
    service_metadata_path,
};
#[cfg(target_os = "macos")]
use agents_notifier::process::{StopOutcome, SystemProcessManager, stop_with_manager};
use agents_notifier::providers::build_providers;
use agents_notifier::router::Provider;
use agents_notifier::service::{
    PlatformServiceManager, ServiceDefinition, ServiceStartOutcome, ServiceStopOutcome,
    load_metadata,
};
use agents_notifier::setup;
use agents_notifier::sources::codex_desktop;

const SERVICE_READY_TIMEOUT: Duration = Duration::from_secs(5);
const SERVICE_READY_POLL_INTERVAL: Duration = Duration::from_millis(100);
const START_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

struct ProgressLine {
    message: &'static str,
    spinner: Option<ProgressBar>,
}

enum ProgressOutcome {
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SetupProviderSummary {
    provider_name: &'static str,
    fields: Vec<SetupProviderSummaryField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SetupProviderSummaryField {
    label: &'static str,
    value: String,
    tone: SetupProviderSummaryTone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupProviderSummaryTone {
    Plain,
    Success,
    Warning,
}

impl ProgressLine {
    fn start(message: &'static str) -> anyhow::Result<Self> {
        if !io::stderr().is_terminal() {
            println!("{message}...");
            return Ok(Self {
                message,
                spinner: None,
            });
        }

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::with_template("{spinner} {msg}")
                .context("failed to build progress style")?
                .tick_strings(&["-", "\\", "|", "/"]),
        );
        spinner.set_message(message);
        spinner.enable_steady_tick(START_PROGRESS_INTERVAL);

        Ok(Self {
            message,
            spinner: Some(spinner),
        })
    }

    fn finish(self, outcome: ProgressOutcome) -> anyhow::Result<()> {
        let Some(spinner) = self.spinner else {
            return Ok(());
        };

        spinner.finish_and_clear();

        match outcome {
            ProgressOutcome::Ready => println!("{} {}", self.message, style("ready.").green()),
            ProgressOutcome::Failed => println!("{} {}", self.message, style("failed.").red()),
        }
        Ok(())
    }
}

#[derive(Debug, Parser)]
#[command(name = "agents-notifier")]
#[command(version)]
#[command(about = "Local-first notification routing for coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Start the local notification service")]
    Start {
        #[arg(long, value_name = "PATH", help = "Path to the config file")]
        config: Option<PathBuf>,
    },
    #[command(about = "Set up Agents Notifier and start the service")]
    Setup {
        #[arg(long, value_name = "PATH", help = "Path to the config file")]
        config: Option<PathBuf>,
    },
    #[command(about = "Run the local notification service in the foreground")]
    Watch {
        #[arg(long, value_name = "PATH", help = "Path to the config file")]
        config: Option<PathBuf>,
    },
    #[command(about = "Stop the local notification service")]
    Stop,
    #[command(about = "Stop the service and remove Agents Notifier local files")]
    Uninstall,
    #[command(about = "Show the local notification service status")]
    Status,
    #[command(about = "Print the agents-notifier version")]
    Version,
    #[command(about = "Submit a hook event to the running local service")]
    Emit {
        #[arg(long, help = "Configured source id for this event")]
        source: String,
        #[arg(long, help = "Notification title")]
        title: String,
        #[arg(long, help = "Notification body")]
        body: String,
    },
}

struct LoadedConfig {
    config: Config,
    guided_setup: Option<GuidedSetup>,
}

enum GuidedSetup {
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
    Weixin {
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
enum InitialProvider {
    Ntfy,
    FeishuLark,
    Webhook,
    Pushover,
    Slack,
    Discord,
    Telegram,
    Whatsapp,
    Weixin,
    MicrosoftTeams,
    EmailSmtp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WeixinSetupMethod {
    QrLogin,
    ExistingToken,
}

struct WeixinLogin {
    token: String,
    base_url: String,
    scanned_user_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct SetupDefaults {
    language: Option<CliLanguage>,
    agent: Option<setup::AgentSelection>,
    answer_detail: Option<AnswerDetail>,
    prompt_detail: Option<PromptDetail>,
    provider: Option<InitialProvider>,
    ntfy_topic: Option<String>,
    feishu_lark_webhook_url: Option<String>,
    feishu_lark_secret: Option<String>,
    webhook_url: Option<String>,
    pushover_app_token: Option<String>,
    pushover_user_key: Option<String>,
    pushover_device: Option<String>,
    pushover_sound: Option<String>,
    slack_webhook_url: Option<String>,
    discord_webhook_url: Option<String>,
    telegram_bot_token: Option<String>,
    telegram_chat_id: Option<String>,
    whatsapp_access_token: Option<String>,
    whatsapp_phone_number_id: Option<String>,
    whatsapp_recipient_phone_number: Option<String>,
    weixin_base_url: Option<String>,
    weixin_token: Option<String>,
    weixin_recipient_user_id: Option<String>,
    weixin_context_token: Option<String>,
    weixin_route_tag: Option<String>,
    microsoft_teams_webhook_url: Option<String>,
    email_smtp_host: Option<String>,
    email_smtp_port: Option<u16>,
    email_smtp_security: Option<EmailSmtpSecurity>,
    email_smtp_username: Option<String>,
    email_smtp_password: Option<String>,
    email_smtp_from: Option<String>,
    email_smtp_to: Option<Vec<String>>,
    email_smtp_reply_to: Option<String>,
}

impl SetupDefaults {
    fn from_config(config: &Config) -> Self {
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
            weixin_base_url: first_provider_of_type(config, ProviderType::Weixin)
                .and_then(|provider| provider.base_url.clone()),
            weixin_token: first_provider_of_type(config, ProviderType::Weixin)
                .and_then(|provider| provider.token.clone()),
            weixin_recipient_user_id: first_provider_of_type(config, ProviderType::Weixin)
                .and_then(|provider| provider.recipient_user_id.clone()),
            weixin_context_token: first_provider_of_type(config, ProviderType::Weixin)
                .and_then(|provider| provider.context_token.clone()),
            weixin_route_tag: first_provider_of_type(config, ProviderType::Weixin)
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
enum ConfigWriteMode {
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
            Self::Weixin => "WeChat",
            Self::MicrosoftTeams => "Microsoft Teams",
            Self::EmailSmtp => "Email SMTP",
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Start {
            config: config_path,
        } => {
            let allow_setup = config_path.is_none();
            let config_path = resolve_config_path(config_path)?;
            let loaded = load_config(&config_path, allow_setup).await?;
            run_start_service(&config_path, loaded, false).await
        }
        Command::Setup {
            config: config_path,
        } => {
            let config_path = resolve_config_path(config_path)?;
            let loaded = run_provider_setup(&config_path).await?;
            run_start_service(&config_path, loaded, true).await
        }
        Command::Watch {
            config: config_path,
        } => {
            let config_path = resolve_config_path(config_path)?;
            let loaded = load_config(&config_path, false).await?;
            init_tracing(&loaded.config.log.level)?;
            run_watch(&loaded.config).await
        }
        Command::Stop => run_stop(),
        Command::Uninstall => run_uninstall(),
        Command::Status => run_status(),
        Command::Version => {
            println!("agents-notifier {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Emit {
            source,
            title,
            body,
        } => run_emit(&source, title, body).await,
    }
}

async fn load_config(path: &Path, allow_setup: bool) -> anyhow::Result<LoadedConfig> {
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

fn is_interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn prompt_theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

fn prompt_text(
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

fn prompt_private_text(
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

fn prompt_secret(
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

fn prompt_confirm(prompt: &str, default: bool) -> anyhow::Result<bool> {
    let theme = prompt_theme();
    Confirm::with_theme(&theme)
        .with_prompt(prompt)
        .default(default)
        .interact()
        .context("failed to read confirmation")
}

fn prompt_for_language(default: Option<CliLanguage>) -> anyhow::Result<CliLanguage> {
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

fn language_default(default: Option<CliLanguage>) -> anyhow::Result<CliLanguage> {
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

fn localized(i18n: I18n, english: &'static str, simplified_chinese: &'static str) -> &'static str {
    match i18n.language() {
        CliLanguage::English => english,
        CliLanguage::SimplifiedChinese => simplified_chinese,
    }
}

fn localized_string(i18n: I18n, english: String, simplified_chinese: String) -> String {
    match i18n.language() {
        CliLanguage::English => english,
        CliLanguage::SimplifiedChinese => simplified_chinese,
    }
}

fn print_heading(title: &str) {
    println!("{}", style(title).bold());
}

fn print_section(title: &str) {
    println!();
    print_heading(title);
}

fn print_field(label: &str, value: impl Display) {
    println!("  {} {}", style(format!("{label}:")).dim(), value);
}

fn print_success_field(label: &str, value: impl Display) {
    print_field(label, style(value.to_string()).green());
}

fn print_path_field(label: &str, value: impl Display) {
    print_field(label, style(value.to_string()).cyan());
}

fn print_bool_field(label: &str, value: bool) {
    if value {
        print_success_field(label, "yes");
    } else {
        print_field(label, style("no").yellow());
    }
}

fn print_setup_summary(
    mode: ConfigWriteMode,
    path: &Path,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    i18n: I18n,
) {
    println!(
        "{} config: {}",
        style(mode.past_tense(i18n)).green(),
        style(path.display()).cyan()
    );
    println!(
        "{}: {}",
        i18n.text(Text::AgentField),
        style(agent.display_name()).green()
    );
    println!(
        "{}: {}",
        i18n.text(Text::AnswerDetailField),
        style(i18n.answer_detail(answer_detail)).green()
    );
    println!(
        "{}: {}",
        i18n.text(Text::PromptDetailField),
        style(i18n.prompt_detail(prompt_detail)).green()
    );
}

fn print_provider_configured(provider: &str, i18n: I18n) {
    println!("{provider}: {}", style(i18n.text(Text::Configured)).green());
}

fn print_setup_provider_summary(config: &Config, i18n: I18n) {
    for summary in setup_provider_summaries(config) {
        print_provider_configured(summary.provider_name, i18n);
        for field in &summary.fields {
            print_setup_provider_summary_field(field, i18n);
        }
    }
}

fn print_setup_provider_summary_field(field: &SetupProviderSummaryField, i18n: I18n) {
    let value = localized_summary_value(&field.value, i18n);
    match field.tone {
        SetupProviderSummaryTone::Plain => print_field(field.label, value),
        SetupProviderSummaryTone::Success => print_success_field(field.label, value),
        SetupProviderSummaryTone::Warning => print_field(field.label, style(value).yellow()),
    }
}

fn localized_summary_value(value: &str, i18n: I18n) -> &str {
    if i18n.language() == CliLanguage::English {
        return value;
    }

    match value {
        "configured" => i18n.text(Text::Configured),
        "not configured" => i18n.text(Text::NotConfigured),
        "not used" => i18n.text(Text::NotUsed),
        "all devices" => i18n.text(Text::AllDevices),
        "account default" => i18n.text(Text::AccountDefault),
        _ => value,
    }
}

fn setup_provider_summaries(config: &Config) -> Vec<SetupProviderSummary> {
    config
        .providers
        .iter()
        .map(setup_provider_summary)
        .collect()
}

fn setup_provider_summary(provider: &ProviderConfig) -> SetupProviderSummary {
    match &provider.provider_type {
        ProviderType::Ntfy => SetupProviderSummary {
            provider_name: "ntfy",
            fields: vec![
                plain_summary_field("server", required_summary_string(provider, "server")),
                plain_summary_field("topic", required_summary_string(provider, "topic")),
            ],
        },
        ProviderType::FeishuLark => SetupProviderSummary {
            provider_name: "Feishu/Lark custom bot",
            fields: vec![
                plain_summary_field("webhook", provider_url_summary(provider)),
                configured_or_not_summary_field(
                    "signature verification",
                    provider_secret_configured(&provider.secret, &provider.secret_env),
                ),
            ],
        },
        ProviderType::Webhook => SetupProviderSummary {
            provider_name: "Webhook",
            fields: vec![plain_summary_field(
                "endpoint",
                provider_url_summary(provider),
            )],
        },
        ProviderType::Pushover => SetupProviderSummary {
            provider_name: "Pushover",
            fields: vec![
                configured_or_not_summary_field(
                    "application token",
                    provider_secret_configured(&provider.app_token, &provider.app_token_env),
                ),
                configured_or_not_summary_field(
                    "user or group key",
                    provider_secret_configured(&provider.user_key, &provider.user_key_env),
                ),
                plain_summary_field(
                    "device",
                    provider
                        .device
                        .as_deref()
                        .unwrap_or("all devices")
                        .to_string(),
                ),
                plain_summary_field(
                    "sound",
                    provider
                        .sound
                        .as_deref()
                        .unwrap_or("account default")
                        .to_string(),
                ),
            ],
        },
        ProviderType::Slack => SetupProviderSummary {
            provider_name: "Slack",
            fields: vec![plain_summary_field(
                "webhook",
                provider_url_summary(provider),
            )],
        },
        ProviderType::Discord => SetupProviderSummary {
            provider_name: "Discord",
            fields: vec![plain_summary_field(
                "webhook",
                provider_url_summary(provider),
            )],
        },
        ProviderType::Telegram => SetupProviderSummary {
            provider_name: "Telegram",
            fields: vec![
                configured_or_not_summary_field(
                    "bot token",
                    provider_secret_configured(&provider.bot_token, &provider.bot_token_env),
                ),
                plain_summary_field("chat id", required_summary_string(provider, "chat_id")),
            ],
        },
        ProviderType::Whatsapp => SetupProviderSummary {
            provider_name: "WhatsApp",
            fields: vec![
                configured_or_not_summary_field(
                    "access token",
                    provider_secret_configured(&provider.access_token, &provider.access_token_env),
                ),
                plain_summary_field(
                    "phone number id",
                    required_summary_string(provider, "phone_number_id"),
                ),
                plain_summary_field(
                    "recipient",
                    required_summary_string(provider, "recipient_phone_number"),
                ),
            ],
        },
        ProviderType::Weixin => SetupProviderSummary {
            provider_name: "WeChat",
            fields: vec![
                configured_or_not_summary_field(
                    "iLink token",
                    provider_secret_configured(&provider.token, &provider.token_env),
                ),
                configured_or_not_summary_field(
                    "context token",
                    provider_secret_configured(
                        &provider.context_token,
                        &provider.context_token_env,
                    ),
                ),
                plain_summary_field("endpoint", required_summary_host(provider, "base_url")),
                plain_summary_field(
                    "recipient",
                    required_summary_string(provider, "recipient_user_id"),
                ),
            ],
        },
        ProviderType::MicrosoftTeams => SetupProviderSummary {
            provider_name: "Microsoft Teams",
            fields: vec![plain_summary_field(
                "webhook",
                provider_url_summary(provider),
            )],
        },
        ProviderType::EmailSmtp => {
            let mut fields = vec![
                plain_summary_field("server", email_smtp_server_summary(provider)),
                plain_summary_field("security", email_smtp_security_summary(provider)),
                email_smtp_authentication_summary_field(provider),
                plain_summary_field("from", required_summary_string(provider, "from")),
                plain_summary_field("to", required_summary_recipients(provider)),
            ];

            if let Some(reply_to) = present_summary_str(provider.reply_to.as_deref()) {
                fields.push(plain_summary_field("reply-to", reply_to.to_string()));
            }

            SetupProviderSummary {
                provider_name: "Email SMTP",
                fields,
            }
        }
    }
}

fn plain_summary_field(label: &'static str, value: String) -> SetupProviderSummaryField {
    SetupProviderSummaryField {
        label,
        value,
        tone: SetupProviderSummaryTone::Plain,
    }
}

fn configured_or_not_summary_field(
    label: &'static str,
    configured: bool,
) -> SetupProviderSummaryField {
    SetupProviderSummaryField {
        label,
        value: if configured {
            "configured".to_string()
        } else {
            "not configured".to_string()
        },
        tone: if configured {
            SetupProviderSummaryTone::Success
        } else {
            SetupProviderSummaryTone::Warning
        },
    }
}

fn provider_secret_configured(inline: &Option<String>, env: &Option<String>) -> bool {
    present_summary_str(inline.as_deref()).is_some()
        || present_summary_str(env.as_deref()).is_some()
}

fn provider_url_summary(provider: &ProviderConfig) -> String {
    if let Some(url) = present_summary_str(provider.url.as_deref()) {
        return safe_url_host(url);
    }
    if let Some(env_name) = present_summary_str(provider.url_env.as_deref()) {
        return format!("env:{env_name}");
    }

    panic!(
        "{} provider `{}` is missing a webhook URL source",
        provider.provider_type.as_str(),
        provider.id
    );
}

fn required_summary_host(provider: &ProviderConfig, field: &'static str) -> String {
    safe_url_host(&required_summary_string(provider, field))
}

fn required_summary_string(provider: &ProviderConfig, field: &'static str) -> String {
    let value = match field {
        "base_url" => provider.base_url.as_deref(),
        "server" => provider.server.as_deref(),
        "topic" => provider.topic.as_deref(),
        "chat_id" => provider.chat_id.as_deref(),
        "phone_number_id" => provider.phone_number_id.as_deref(),
        "recipient_phone_number" => provider.recipient_phone_number.as_deref(),
        "recipient_user_id" => provider.recipient_user_id.as_deref(),
        "from" => provider.from.as_deref(),
        _ => panic!("unsupported setup summary field `{field}`"),
    };

    present_summary_str(value)
        .unwrap_or_else(|| {
            panic!(
                "{} provider `{}` is missing `{field}`",
                provider.provider_type.as_str(),
                provider.id
            )
        })
        .to_string()
}

fn required_summary_recipients(provider: &ProviderConfig) -> String {
    provider
        .to
        .as_ref()
        .filter(|recipients| !recipients.is_empty())
        .unwrap_or_else(|| {
            panic!(
                "{} provider `{}` is missing `to`",
                provider.provider_type.as_str(),
                provider.id
            )
        })
        .join(", ")
}

fn email_smtp_server_summary(provider: &ProviderConfig) -> String {
    let host = provider.host.as_deref().unwrap_or_else(|| {
        panic!(
            "{} provider `{}` is missing `host`",
            provider.provider_type.as_str(),
            provider.id
        )
    });
    let port = provider.port.unwrap_or_else(|| {
        panic!(
            "{} provider `{}` is missing `port`",
            provider.provider_type.as_str(),
            provider.id
        )
    });

    format!("{host}:{port}")
}

fn email_smtp_security_summary(provider: &ProviderConfig) -> String {
    provider
        .security
        .map(email_smtp_security_display)
        .unwrap_or_else(|| {
            panic!(
                "{} provider `{}` is missing `security`",
                provider.provider_type.as_str(),
                provider.id
            )
        })
        .to_string()
}

fn email_smtp_authentication_summary_field(provider: &ProviderConfig) -> SetupProviderSummaryField {
    match (
        provider_secret_configured(&provider.username, &provider.username_env),
        provider_secret_configured(&provider.password, &provider.password_env),
    ) {
        (true, true) => configured_or_not_summary_field("authentication", true),
        (false, false) => plain_summary_field("authentication", "not used".to_string()),
        _ => panic!(
            "{} provider `{}` must configure both username and password, or neither",
            provider.provider_type.as_str(),
            provider.id
        ),
    }
}

fn present_summary_str(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn write_setup_config(
    path: &Path,
    config: &mut Config,
    language: CliLanguage,
) -> anyhow::Result<()> {
    config.cli = CliConfig::new(language);
    setup::write_config(path, config)
}

async fn run_first_start_setup(path: &Path) -> anyhow::Result<LoadedConfig> {
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

async fn run_provider_setup(path: &Path) -> anyhow::Result<LoadedConfig> {
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

async fn run_guided_provider_setup(
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
        InitialProvider::Weixin => {
            run_weixin_setup(
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

fn answer_detail_for_provider(
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
        InitialProvider::Weixin => {
            println!();
            println!(
                "{}",
                fixed_answer_detail_message(
                    i18n,
                    "WeChat",
                    "Agents Notifier uses a 3800-character delivery guard for WeChat iLink text messages",
                    "Agents Notifier 对 WeChat iLink 文本消息使用 3800 字符发送保护线"
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

fn prompt_detail_for_provider(
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
        InitialProvider::Weixin => {
            println!();
            println!(
                "{}",
                fixed_prompt_detail_message(
                    i18n,
                    "WeChat",
                    "Agents Notifier uses a 3800-character delivery guard for WeChat iLink text messages",
                    "Agents Notifier 对 WeChat iLink 文本消息使用 3800 字符发送保护线"
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

fn fixed_answer_detail_message(
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

fn fixed_prompt_detail_message(
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

fn run_ntfy_setup(
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

fn run_feishu_lark_setup(
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

fn run_webhook_setup(
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

fn run_pushover_setup(
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

fn run_slack_setup(
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

fn run_discord_setup(
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

fn run_telegram_setup(
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

fn run_whatsapp_setup(
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

async fn run_weixin_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
    i18n: I18n,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("{}", i18n.text(Text::WeixinIntro1));
    println!("{}", i18n.text(Text::WeixinIntro2));
    println!();

    let mut base_url = prompt_for_weixin_base_url(defaults.weixin_base_url.as_deref(), i18n)?;
    let route_tag = prompt_for_weixin_route_tag(defaults.weixin_route_tag.as_deref(), i18n)?;
    let setup_method = prompt_for_weixin_setup_method(i18n)?;

    let (token, scanned_user_id) = match setup_method {
        WeixinSetupMethod::QrLogin => {
            let login = run_weixin_qr_login(&base_url, route_tag.as_deref(), i18n).await?;
            base_url = login.base_url;
            (login.token, login.scanned_user_id)
        }
        WeixinSetupMethod::ExistingToken => {
            let token = prompt_for_weixin_token(defaults.weixin_token.as_deref(), i18n)?;
            println!("{}", i18n.text(Text::WeixinTokenVerifying));
            setup::verify_weixin_token(&base_url, &token, route_tag.as_deref()).await?;
            println!("{}", style(i18n.text(Text::WeixinTokenVerified)).green());
            (token, None)
        }
    };

    let token_changed = defaults.weixin_token.as_deref() != Some(token.as_str());
    let linked_recipient = if let (false, Some(recipient_user_id), Some(context_token)) = (
        token_changed,
        defaults.weixin_recipient_user_id.as_ref(),
        defaults.weixin_context_token.as_ref(),
    ) {
        if prompt_confirm(i18n.text(Text::WeixinKeepRecipientPrompt), true)? {
            setup::WeixinRecipientLink {
                recipient_user_id: recipient_user_id.clone(),
                context_token: context_token.clone(),
            }
        } else {
            link_weixin_recipient(
                &base_url,
                &token,
                route_tag.as_deref(),
                scanned_user_id.as_deref(),
                i18n,
            )
            .await?
        }
    } else {
        link_weixin_recipient(
            &base_url,
            &token,
            route_tag.as_deref(),
            scanned_user_id.as_deref(),
            i18n,
        )
        .await?
    };

    let mut config = setup::build_weixin_config(
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
        guided_setup: Some(GuidedSetup::Weixin { agent }),
    })
}

fn run_microsoft_teams_setup(
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

fn run_email_smtp_setup(
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

fn prompt_for_agent(
    default: Option<setup::AgentSelection>,
    i18n: I18n,
) -> anyhow::Result<setup::AgentSelection> {
    let options = supported_agent_options();
    let supported_default =
        default.filter(|agent| options.iter().any(|(_, candidate)| candidate == agent));
    let effective_default = supported_default.unwrap_or_else(default_agent_for_platform);
    let default_index = options
        .iter()
        .position(|(_, agent)| *agent == effective_default)
        .unwrap_or(0);
    let items = options
        .iter()
        .map(|(_, agent)| {
            let mut label = agent.display_name().to_string();
            if Some(*agent) == supported_default {
                label.push_str(&format!(" ({})", i18n.text(Text::CurrentSuffix)));
            } else if supported_default.is_none() && *agent == effective_default {
                label.push_str(&format!(" ({})", i18n.text(Text::RecommendedSuffix)));
            }
            label
        })
        .collect::<Vec<_>>();
    let theme = prompt_theme();
    let selection = Select::with_theme(&theme)
        .with_prompt(i18n.text(Text::AgentPrompt))
        .items(&items)
        .default(default_index)
        .interact()
        .context("failed to read agent choice")?;

    Ok(options[selection].1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "non-host platform variants keep setup platform rules testable on every OS"
    )
)]
enum RuntimePlatform {
    Macos,
    Linux,
    Windows,
}

impl RuntimePlatform {
    fn current() -> Self {
        #[cfg(target_os = "macos")]
        return Self::Macos;

        #[cfg(target_os = "linux")]
        return Self::Linux;

        #[cfg(windows)]
        return Self::Windows;
    }

    fn supports_codex_desktop(self) -> bool {
        matches!(self, Self::Macos | Self::Windows)
    }
}

fn supported_agent_options() -> Vec<(&'static str, setup::AgentSelection)> {
    supported_agent_options_for_platform(RuntimePlatform::current())
}

fn supported_agent_options_for_platform(
    platform: RuntimePlatform,
) -> Vec<(&'static str, setup::AgentSelection)> {
    if platform.supports_codex_desktop() {
        return vec![
            ("1", setup::AgentSelection::CodexDesktop),
            ("2", setup::AgentSelection::CodexCli),
            ("3", setup::AgentSelection::ClaudeCode),
            ("4", setup::AgentSelection::CursorCli),
            ("5", setup::AgentSelection::OpenCodeCli),
            ("6", setup::AgentSelection::OpenClaw),
            ("7", setup::AgentSelection::HermesAgentCli),
            ("8", setup::AgentSelection::GithubCopilotCli),
            ("9", setup::AgentSelection::GeminiCli),
            ("10", setup::AgentSelection::Aider),
        ];
    }

    vec![
        ("1", setup::AgentSelection::CodexCli),
        ("2", setup::AgentSelection::ClaudeCode),
        ("3", setup::AgentSelection::CursorCli),
        ("4", setup::AgentSelection::OpenCodeCli),
        ("5", setup::AgentSelection::OpenClaw),
        ("6", setup::AgentSelection::HermesAgentCli),
        ("7", setup::AgentSelection::GithubCopilotCli),
        ("8", setup::AgentSelection::GeminiCli),
        ("9", setup::AgentSelection::Aider),
    ]
}

fn default_agent_for_platform() -> setup::AgentSelection {
    default_agent_for_runtime_platform(RuntimePlatform::current())
}

fn default_agent_for_runtime_platform(platform: RuntimePlatform) -> setup::AgentSelection {
    if platform.supports_codex_desktop() {
        setup::AgentSelection::CodexDesktop
    } else {
        setup::AgentSelection::CodexCli
    }
}

fn prompt_for_answer_detail(
    default: Option<AnswerDetail>,
    i18n: I18n,
) -> anyhow::Result<AnswerDetail> {
    let effective_default = default.unwrap_or(AnswerDetail::Preview);
    let options = [AnswerDetail::Preview, AnswerDetail::Full];
    let default_index = options
        .iter()
        .position(|answer_detail| *answer_detail == effective_default)
        .unwrap_or(0);
    let items = options
        .iter()
        .map(|answer_detail| {
            let mut label = i18n.answer_detail(*answer_detail).to_string();
            if Some(*answer_detail) == default {
                label.push_str(&format!(" ({})", i18n.text(Text::CurrentSuffix)));
            } else if default.is_none() && *answer_detail == AnswerDetail::Preview {
                label.push_str(&format!(" ({})", i18n.text(Text::RecommendedSuffix)));
            }
            label
        })
        .collect::<Vec<_>>();
    let theme = prompt_theme();
    let selection = Select::with_theme(&theme)
        .with_prompt(i18n.text(Text::AnswerDetailPrompt))
        .items(&items)
        .default(default_index)
        .interact()
        .context("failed to read answer detail choice")?;

    Ok(options[selection])
}

fn prompt_for_prompt_detail(
    default: Option<PromptDetail>,
    i18n: I18n,
) -> anyhow::Result<PromptDetail> {
    let effective_default = default.unwrap_or(PromptDetail::Off);
    let options = [PromptDetail::Off, PromptDetail::On];
    let default_index = options
        .iter()
        .position(|prompt_detail| *prompt_detail == effective_default)
        .unwrap_or(0);
    let items = options
        .iter()
        .map(|prompt_detail| {
            let mut label = i18n.prompt_detail(*prompt_detail).to_string();
            if Some(*prompt_detail) == default {
                label.push_str(&format!(" ({})", i18n.text(Text::CurrentSuffix)));
            } else if default.is_none() && *prompt_detail == PromptDetail::Off {
                label.push_str(&format!(" ({})", i18n.text(Text::RecommendedSuffix)));
            }
            label
        })
        .collect::<Vec<_>>();
    let theme = prompt_theme();
    let selection = Select::with_theme(&theme)
        .with_prompt(i18n.text(Text::PromptDetailPrompt))
        .items(&items)
        .default(default_index)
        .interact()
        .context("failed to read prompt detail choice")?;

    Ok(options[selection])
}

fn prompt_for_initial_provider(
    default: Option<InitialProvider>,
    i18n: I18n,
) -> anyhow::Result<InitialProvider> {
    let effective_default = default.unwrap_or(InitialProvider::Ntfy);
    let options = [
        InitialProvider::Ntfy,
        InitialProvider::Slack,
        InitialProvider::Discord,
        InitialProvider::Pushover,
        InitialProvider::FeishuLark,
        InitialProvider::Webhook,
        InitialProvider::Telegram,
        InitialProvider::Whatsapp,
        InitialProvider::Weixin,
        InitialProvider::MicrosoftTeams,
        InitialProvider::EmailSmtp,
    ];
    let default_index = options
        .iter()
        .position(|provider| *provider == effective_default)
        .unwrap_or(0);
    let items = options
        .iter()
        .map(|provider| {
            let mut label = provider.display_name().to_string();
            if Some(*provider) == default {
                label.push_str(&format!(" ({})", i18n.text(Text::CurrentSuffix)));
            } else if default.is_none() && *provider == InitialProvider::Ntfy {
                label.push_str(&format!(" ({})", i18n.text(Text::RecommendedSuffix)));
            }
            label
        })
        .collect::<Vec<_>>();
    let theme = prompt_theme();
    let selection = Select::with_theme(&theme)
        .with_prompt(i18n.text(Text::ProviderPrompt))
        .items(&items)
        .default(default_index)
        .interact()
        .context("failed to read provider choice")?;

    Ok(options[selection])
}

fn prompt_for_ntfy_topic(
    generated_topic: &str,
    current_topic: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let default_topic = current_topic.unwrap_or(generated_topic);
        let prompt = if let Some(current_topic) = current_topic {
            localized_string(
                i18n,
                format!("Topic [{current_topic}, press Enter to keep]"),
                format!("Topic [{current_topic}，按 Enter 保留]"),
            )
        } else {
            format!("Topic [{generated_topic}]")
        };
        let input = prompt_text(prompt, "failed to read topic")?;

        match setup::resolve_ntfy_topic(&input, default_topic) {
            Ok(topic) => return Ok(topic),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_feishu_lark_webhook_url(
    current_url: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_url) = current_url {
            localized_string(
                i18n,
                format!(
                    "Webhook URL [configured: {}, edit or press Enter to keep]",
                    safe_url_host(current_url)
                ),
                format!(
                    "Webhook URL [已配置：{}，修改或按 Enter 保留]",
                    safe_url_host(current_url)
                ),
            )
        } else {
            "Webhook URL".to_string()
        };
        let input = prompt_private_text(prompt, current_url, "failed to read webhook URL")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_url.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_feishu_lark_webhook_url(candidate) {
            Ok(url) => return Ok(url),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_feishu_lark_secret(
    current_secret: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<Option<String>> {
    let prompt = if current_secret.is_some() {
        localized(
            i18n,
            "Signing secret (optional) [configured, press Enter to keep, type `none` to clear]",
            "签名 secret（可选）[已配置，按 Enter 保留，输入 `none` 清除]",
        )
    } else {
        localized(i18n, "Signing secret (optional)", "签名 secret（可选）")
    };
    let input = prompt_secret(prompt, "failed to read signing secret")?;

    let input = input.trim();
    if input.is_empty() {
        return Ok(current_secret.map(ToOwned::to_owned));
    }
    if input.eq_ignore_ascii_case("none") {
        return Ok(None);
    }

    Ok(setup::resolve_feishu_lark_secret(input))
}

fn prompt_for_webhook_url(current_url: Option<&str>, i18n: I18n) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_url) = current_url {
            localized_string(
                i18n,
                format!(
                    "Webhook URL [configured: {}, edit or press Enter to keep]",
                    safe_url_host(current_url)
                ),
                format!(
                    "Webhook URL [已配置：{}，修改或按 Enter 保留]",
                    safe_url_host(current_url)
                ),
            )
        } else {
            "Webhook URL".to_string()
        };
        let input = prompt_private_text(prompt, current_url, "failed to read webhook URL")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_url.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_webhook_url(candidate) {
            Ok(url) => return Ok(url),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_slack_webhook_url(current_url: Option<&str>, i18n: I18n) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_url) = current_url {
            localized_string(
                i18n,
                format!(
                    "Slack webhook URL [configured: {}, edit or press Enter to keep]",
                    safe_url_host(current_url)
                ),
                format!(
                    "Slack webhook URL [已配置：{}，修改或按 Enter 保留]",
                    safe_url_host(current_url)
                ),
            )
        } else {
            "Slack webhook URL".to_string()
        };
        let input = prompt_private_text(prompt, current_url, "failed to read Slack webhook URL")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_url.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_slack_webhook_url(candidate) {
            Ok(url) => return Ok(url),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_discord_webhook_url(current_url: Option<&str>, i18n: I18n) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_url) = current_url {
            localized_string(
                i18n,
                format!(
                    "Discord webhook URL [configured: {}, edit or press Enter to keep]",
                    safe_url_host(current_url)
                ),
                format!(
                    "Discord webhook URL [已配置：{}，修改或按 Enter 保留]",
                    safe_url_host(current_url)
                ),
            )
        } else {
            "Discord webhook URL".to_string()
        };
        let input = prompt_private_text(prompt, current_url, "failed to read Discord webhook URL")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_url.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_discord_webhook_url(candidate) {
            Ok(url) => return Ok(url),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_telegram_bot_token(
    current_token: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if current_token.is_some() {
            localized(
                i18n,
                "Telegram bot token [configured, press Enter to keep]",
                "Telegram bot token [已配置，按 Enter 保留]",
            )
        } else {
            "Telegram bot token"
        };
        let input = prompt_secret(prompt, "failed to read Telegram bot token")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_token.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_telegram_bot_token(candidate) {
            Ok(token) => return Ok(token),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_telegram_chat_id(
    current_chat_id: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_chat_id) = current_chat_id {
            localized_string(
                i18n,
                format!("Telegram chat id [{current_chat_id}, press Enter to keep]"),
                format!("Telegram chat id [{current_chat_id}，按 Enter 保留]"),
            )
        } else {
            localized(
                i18n,
                "Telegram chat id, for example 123456789 or @channelusername",
                "Telegram chat id，例如 123456789 或 @channelusername",
            )
            .to_string()
        };
        let input = prompt_text(prompt, "failed to read Telegram chat id")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_chat_id.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_telegram_chat_id(candidate) {
            Ok(chat_id) => return Ok(chat_id),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_whatsapp_access_token(
    current_token: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if current_token.is_some() {
            localized(
                i18n,
                "WhatsApp access token [configured, press Enter to keep]",
                "WhatsApp access token [已配置，按 Enter 保留]",
            )
        } else {
            "WhatsApp access token"
        };
        let input = prompt_secret(prompt, "failed to read WhatsApp access token")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_token.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_whatsapp_access_token(candidate) {
            Ok(token) => return Ok(token),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_whatsapp_phone_number_id(
    current_phone_number_id: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_phone_number_id) = current_phone_number_id {
            localized_string(
                i18n,
                format!(
                    "WhatsApp phone number ID [{current_phone_number_id}, press Enter to keep]"
                ),
                format!("WhatsApp phone number ID [{current_phone_number_id}，按 Enter 保留]"),
            )
        } else {
            "WhatsApp phone number ID".to_string()
        };
        let input = prompt_text(prompt, "failed to read WhatsApp phone number ID")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_phone_number_id.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_whatsapp_phone_number_id(candidate) {
            Ok(phone_number_id) => return Ok(phone_number_id),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_whatsapp_recipient_phone_number(
    current_phone_number: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_phone_number) = current_phone_number {
            localized_string(
                i18n,
                format!(
                    "WhatsApp recipient phone number [{current_phone_number}, press Enter to keep]"
                ),
                format!("WhatsApp recipient phone number [{current_phone_number}，按 Enter 保留]"),
            )
        } else {
            localized(
                i18n,
                "WhatsApp recipient phone number, digits only with country code",
                "WhatsApp 接收手机号，只填数字，包含国家区号",
            )
            .to_string()
        };
        let input = prompt_text(prompt, "failed to read WhatsApp recipient phone number")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_phone_number.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_whatsapp_recipient_phone_number(candidate) {
            Ok(phone_number) => return Ok(phone_number),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_weixin_base_url(
    current_base_url: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let default_base_url = current_base_url.unwrap_or(setup::DEFAULT_WEIXIN_BASE_URL);
        let prompt = if let Some(current_base_url) = current_base_url {
            localized_string(
                i18n,
                format!(
                    "WeChat gateway URL [{current_base_url}, press Enter to keep unless your iLink provider gave you another URL]"
                ),
                format!(
                    "WeChat gateway URL [{current_base_url}，没有新的 iLink URL 就按 Enter 保留]"
                ),
            )
        } else {
            localized_string(
                i18n,
                format!(
                    "WeChat gateway URL [{}; press Enter for the default]",
                    setup::DEFAULT_WEIXIN_BASE_URL
                ),
                format!(
                    "WeChat gateway URL [{}；按 Enter 使用默认值]",
                    setup::DEFAULT_WEIXIN_BASE_URL
                ),
            )
        };
        let input = prompt_text(prompt, "failed to read WeChat iLink base URL")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            default_base_url
        } else {
            input
        };

        match setup::resolve_weixin_base_url(candidate) {
            Ok(base_url) => return Ok(base_url),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt_for_weixin_route_tag(
    current_route_tag: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<Option<String>> {
    loop {
        let prompt = if let Some(current_route_tag) = current_route_tag {
            localized_string(
                i18n,
                format!(
                    "Optional WeChat route tag [{current_route_tag}, press Enter to keep, type `none` to clear]"
                ),
                format!(
                    "可选 WeChat route tag [{current_route_tag}，按 Enter 保留，输入 `none` 清除]"
                ),
            )
        } else {
            localized(
                i18n,
                "Optional WeChat route tag (advanced; press Enter to skip unless your iLink provider gave you an SKRouteTag)",
                "可选 WeChat route tag（高级项；没有服务方给你的 SKRouteTag 就直接按 Enter 跳过）",
            )
            .to_string()
        };
        let input = prompt_text(prompt, "failed to read WeChat SKRouteTag")?;

        let input = input.trim();
        if input.is_empty() {
            return Ok(current_route_tag.map(ToOwned::to_owned));
        }
        if current_route_tag.is_some() && input.eq_ignore_ascii_case("none") {
            return Ok(None);
        }

        match setup::resolve_weixin_route_tag(input) {
            Ok(route_tag) => return Ok(route_tag),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt_for_weixin_setup_method(i18n: I18n) -> anyhow::Result<WeixinSetupMethod> {
    let options = [WeixinSetupMethod::QrLogin, WeixinSetupMethod::ExistingToken];
    let default_index = options
        .iter()
        .position(|method| *method == WeixinSetupMethod::QrLogin)
        .unwrap_or(0);
    let items = options
        .iter()
        .map(|method| match method {
            WeixinSetupMethod::QrLogin => i18n.text(Text::WeixinScanQrOption),
            WeixinSetupMethod::ExistingToken => i18n.text(Text::WeixinExistingTokenOption),
        })
        .collect::<Vec<_>>();
    let theme = prompt_theme();
    let selection = Select::with_theme(&theme)
        .with_prompt(i18n.text(Text::WeixinSetupMethodPrompt))
        .items(&items)
        .default(default_index)
        .interact()
        .context("failed to read WeChat setup method")?;

    Ok(options[selection])
}

fn prompt_for_weixin_token(current_token: Option<&str>, i18n: I18n) -> anyhow::Result<String> {
    loop {
        let prompt = if current_token.is_some() {
            localized(
                i18n,
                "WeChat iLink token [configured, press Enter to keep]",
                "WeChat iLink token [已配置，按 Enter 保留]",
            )
        } else {
            "WeChat iLink token"
        };
        let input = prompt_secret(prompt, "failed to read WeChat iLink token")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_token.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_weixin_token(candidate) {
            Ok(token) => return Ok(token),
            Err(error) => println!("{error}"),
        }
    }
}

async fn run_weixin_qr_login(
    base_url: &str,
    route_tag: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<WeixinLogin> {
    let mut qr =
        setup::fetch_weixin_qr_code(base_url, route_tag, setup::DEFAULT_WEIXIN_BOT_TYPE).await?;
    print_weixin_qr_code(&qr.qr_content, i18n)?;

    let deadline = Instant::now() + setup::DEFAULT_WEIXIN_QR_TIMEOUT;
    let mut refresh_count = 1;
    let mut scanned_message_printed = false;

    while Instant::now() < deadline {
        let status = setup::poll_weixin_qr_status(base_url, &qr.qr_key, route_tag).await?;
        match status.status.as_str() {
            "" | "wait" => {
                sleep(Duration::from_secs(1)).await;
            }
            "scaned" | "scanned" => {
                if !scanned_message_printed {
                    println!("{}", i18n.text(Text::WeixinQrScanned));
                    scanned_message_printed = true;
                }
                sleep(Duration::from_secs(1)).await;
            }
            "expired" => {
                refresh_count += 1;
                if refresh_count > 3 {
                    anyhow::bail!("WeChat QR code expired too many times; run setup again");
                }
                println!("{}", i18n.text(Text::WeixinQrExpired));
                qr = setup::fetch_weixin_qr_code(
                    base_url,
                    route_tag,
                    setup::DEFAULT_WEIXIN_BOT_TYPE,
                )
                .await?;
                print_weixin_qr_code(&qr.qr_content, i18n)?;
                scanned_message_printed = false;
            }
            "confirmed" => {
                let token = status
                    .token
                    .ok_or_else(|| anyhow::anyhow!("WeChat login confirmed without token"))?;
                let token = setup::resolve_weixin_token(&token)?;
                let base_url = status
                    .base_url
                    .as_deref()
                    .unwrap_or(base_url)
                    .trim_end_matches('/');
                let base_url = setup::resolve_weixin_base_url(base_url)?;
                println!("{}", style(i18n.text(Text::WeixinQrConfirmed)).green());
                return Ok(WeixinLogin {
                    token,
                    base_url,
                    scanned_user_id: status.scanned_user_id,
                });
            }
            _ => {
                sleep(Duration::from_secs(1)).await;
            }
        }
    }

    anyhow::bail!("timed out waiting for WeChat QR login")
}

async fn link_weixin_recipient(
    base_url: &str,
    token: &str,
    route_tag: Option<&str>,
    expected_user_id: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<setup::WeixinRecipientLink> {
    println!();
    println!(
        "{}",
        style(i18n.text(Text::WeixinActionHeading)).yellow().bold()
    );
    println!(
        "1. {} {}",
        i18n.text(Text::WeixinActionStep1),
        style("WeixinClawBot").cyan().bold()
    );
    println!(
        "2. {} {}",
        i18n.text(Text::WeixinActionStep2),
        style("hi").green().bold()
    );
    println!(
        "{}",
        style(i18n.text(Text::WeixinActionDoNotType)).red().bold()
    );
    wait_for_enter(
        &style(i18n.text(Text::WeixinActionDone))
            .green()
            .bold()
            .to_string(),
    )?;
    println!("{}", i18n.text(Text::WeixinWaitingForMessage));
    setup::poll_weixin_recipient_link(
        base_url,
        token,
        route_tag,
        expected_user_id,
        setup::DEFAULT_WEIXIN_LINK_TIMEOUT,
    )
    .await
}

fn print_weixin_qr_code(qr_content: &str, i18n: I18n) -> anyhow::Result<()> {
    println!();
    println!("{}", i18n.text(Text::WeixinQrScanPrompt));
    let code = QrCode::new(qr_content.as_bytes()).context("failed to render WeChat QR code")?;
    let image = code.render::<unicode::Dense1x2>().quiet_zone(true).build();
    println!("{image}");
    println!("QR content: {qr_content}");
    println!();
    Ok(())
}

fn prompt_for_microsoft_teams_webhook_url(
    current_url: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_url) = current_url {
            localized_string(
                i18n,
                format!(
                    "Microsoft Teams webhook URL [configured: {}, edit or press Enter to keep]",
                    safe_url_host(current_url)
                ),
                format!(
                    "Microsoft Teams webhook URL [已配置：{}，修改或按 Enter 保留]",
                    safe_url_host(current_url)
                ),
            )
        } else {
            "Microsoft Teams webhook URL".to_string()
        };
        let input = prompt_private_text(
            prompt,
            current_url,
            "failed to read Microsoft Teams webhook URL",
        )?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_url.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_microsoft_teams_webhook_url(candidate) {
            Ok(url) => return Ok(url),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_email_smtp_host(current_host: Option<&str>, i18n: I18n) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_host) = current_host {
            localized_string(
                i18n,
                format!("SMTP host [{current_host}, press Enter to keep]"),
                format!("SMTP host [{current_host}，按 Enter 保留]"),
            )
        } else {
            localized(
                i18n,
                "SMTP host, for example smtp.gmail.com",
                "SMTP host，例如 smtp.gmail.com",
            )
            .to_string()
        };
        let input = prompt_text(prompt, "failed to read SMTP host")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_host.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_email_smtp_host(candidate) {
            Ok(host) => return Ok(host),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt_for_email_smtp_security(
    current_security: Option<EmailSmtpSecurity>,
    i18n: I18n,
) -> anyhow::Result<EmailSmtpSecurity> {
    let effective_default = current_security.unwrap_or(EmailSmtpSecurity::Starttls);
    let options = [EmailSmtpSecurity::Starttls, EmailSmtpSecurity::ImplicitTls];
    let default_index = options
        .iter()
        .position(|security| *security == effective_default)
        .unwrap_or(0);
    let items = options
        .iter()
        .map(|security| {
            let mut label = email_smtp_security_display(*security).to_string();
            if Some(*security) == current_security {
                label.push_str(&format!(" ({})", i18n.text(Text::CurrentSuffix)));
            } else if current_security.is_none() && *security == EmailSmtpSecurity::Starttls {
                label.push_str(&format!(" ({})", i18n.text(Text::RecommendedSuffix)));
            }
            label
        })
        .collect::<Vec<_>>();
    let theme = prompt_theme();
    let selection = Select::with_theme(&theme)
        .with_prompt(localized(i18n, "SMTP security", "SMTP 加密方式"))
        .items(&items)
        .default(default_index)
        .interact()
        .context("failed to read SMTP security")?;

    Ok(options[selection])
}

fn prompt_for_email_smtp_port(default_port: u16, i18n: I18n) -> anyhow::Result<u16> {
    loop {
        let input = prompt_text(
            localized_string(
                i18n,
                format!("SMTP port [{default_port}, press Enter to keep]"),
                format!("SMTP port [{default_port}，按 Enter 保留]"),
            ),
            "failed to read SMTP port",
        )?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            default_port.to_string()
        } else {
            input.to_string()
        };

        match setup::resolve_email_smtp_port(&candidate) {
            Ok(port) => return Ok(port),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt_for_email_smtp_username(
    current_username: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<Option<String>> {
    loop {
        let prompt = if let Some(current_username) = current_username {
            localized_string(
                i18n,
                format!(
                    "SMTP username [{current_username}, press Enter to keep, type `none` for no auth]"
                ),
                format!("SMTP username [{current_username}，按 Enter 保留，输入 `none` 关闭 auth]"),
            )
        } else {
            localized(
                i18n,
                "SMTP username, or press Enter for no auth",
                "SMTP username。没有 auth 就按 Enter",
            )
            .to_string()
        };
        let input = prompt_text(prompt, "failed to read SMTP username")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_username.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_email_smtp_optional_username(candidate) {
            Ok(username) => return Ok(username),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt_for_email_smtp_password(
    current_password: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if current_password.is_some() {
            localized(
                i18n,
                "SMTP password [configured, press Enter to keep]",
                "SMTP password [已配置，按 Enter 保留]",
            )
        } else {
            localized(
                i18n,
                "SMTP password or SMTP API key",
                "SMTP password 或 SMTP API key",
            )
        };
        let input = prompt_secret(prompt, "failed to read SMTP password")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_password.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_email_smtp_password(candidate) {
            Ok(password) => return Ok(password),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt_for_email_smtp_mailbox(
    label: &'static str,
    current_mailbox: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if let Some(current_mailbox) = current_mailbox {
            localized_string(
                i18n,
                format!("{label} [{current_mailbox}, press Enter to keep]"),
                format!("{label} [{current_mailbox}，按 Enter 保留]"),
            )
        } else {
            localized_string(
                i18n,
                format!("{label}, for example Agents Notifier <alerts@example.com>"),
                format!("{label}，例如 Agents Notifier <alerts@example.com>"),
            )
        };
        let input = prompt_text(prompt, format!("failed to read {label}"))?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_mailbox.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_email_smtp_mailbox(candidate, label) {
            Ok(mailbox) => return Ok(mailbox),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt_for_email_smtp_optional_mailbox(
    label: &'static str,
    current_mailbox: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<Option<String>> {
    loop {
        let prompt = if let Some(current_mailbox) = current_mailbox {
            localized_string(
                i18n,
                format!("{label} [{current_mailbox}, press Enter to keep, type `none` to clear]"),
                format!("{label} [{current_mailbox}，按 Enter 保留，输入 `none` 清除]"),
            )
        } else {
            localized_string(
                i18n,
                format!("{label}, or press Enter to skip"),
                format!("{label}，跳过就按 Enter"),
            )
        };
        let input = prompt_text(prompt, format!("failed to read {label}"))?;

        let input = input.trim();
        if input.is_empty() {
            return Ok(current_mailbox.map(ToOwned::to_owned));
        }
        if input.eq_ignore_ascii_case("none") {
            return Ok(None);
        }

        match setup::resolve_email_smtp_mailbox(input, label) {
            Ok(mailbox) => return Ok(Some(mailbox)),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt_for_email_smtp_recipients(
    current_recipients: Option<&[String]>,
    i18n: I18n,
) -> anyhow::Result<Vec<String>> {
    loop {
        let prompt = if let Some(current_recipients) = current_recipients {
            localized_string(
                i18n,
                format!(
                    "Recipient emails [{}; press Enter to keep]",
                    current_recipients.join(", ")
                ),
                format!(
                    "收件人邮箱 [{}；按 Enter 保留]",
                    current_recipients.join(", ")
                ),
            )
        } else {
            localized(
                i18n,
                "Recipient emails, comma-separated",
                "收件人邮箱，多个用英文逗号分隔",
            )
            .to_string()
        };
        let input = prompt_text(prompt, "failed to read recipient emails")?;

        let input = input.trim();
        if input.is_empty()
            && let Some(current_recipients) = current_recipients
        {
            return Ok(current_recipients.to_vec());
        }

        match setup::resolve_email_smtp_recipients(input) {
            Ok(recipients) => return Ok(recipients),
            Err(error) => println!("{error}"),
        }
    }
}

fn prompt_for_pushover_app_token(
    current_token: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if current_token.is_some() {
            localized(
                i18n,
                "Pushover application API token [configured, press Enter to keep]",
                "Pushover application API token [已配置，按 Enter 保留]",
            )
        } else {
            "Pushover application API token"
        };
        let input = prompt_secret(prompt, "failed to read Pushover application API token")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_token.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_pushover_app_token(candidate) {
            Ok(token) => return Ok(token),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_pushover_user_key(
    current_user_key: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<String> {
    loop {
        let prompt = if current_user_key.is_some() {
            localized(
                i18n,
                "Pushover user or group key [configured, press Enter to keep]",
                "Pushover user 或 group key [已配置，按 Enter 保留]",
            )
        } else {
            "Pushover user or group key"
        };
        let input = prompt_secret(prompt, "failed to read Pushover user or group key")?;

        let input = input.trim();
        let candidate = if input.is_empty() {
            current_user_key.unwrap_or(input)
        } else {
            input
        };

        match setup::resolve_pushover_user_key(candidate) {
            Ok(user_key) => return Ok(user_key),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_pushover_device(
    current_device: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<Option<String>> {
    loop {
        let prompt = if let Some(current_device) = current_device {
            localized_string(
                i18n,
                format!(
                    "Pushover device [{current_device}, press Enter to keep, type `none` to clear]"
                ),
                format!("Pushover device [{current_device}，按 Enter 保留，输入 `none` 清除]"),
            )
        } else {
            localized(
                i18n,
                "Pushover device, or press Enter to send to all devices",
                "Pushover device。发给所有设备就按 Enter",
            )
            .to_string()
        };
        let input = prompt_text(prompt, "failed to read Pushover device")?;

        let input = input.trim();
        if input.is_empty() {
            return Ok(current_device.map(ToOwned::to_owned));
        }
        if input.eq_ignore_ascii_case("none") {
            return Ok(None);
        }

        match setup::resolve_pushover_device(input) {
            Ok(device) => return Ok(device),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_pushover_sound(
    current_sound: Option<&str>,
    i18n: I18n,
) -> anyhow::Result<Option<String>> {
    let prompt = if let Some(current_sound) = current_sound {
        localized_string(
            i18n,
            format!("Pushover sound [{current_sound}, press Enter to keep, type `none` to clear]"),
            format!("Pushover sound [{current_sound}，按 Enter 保留，输入 `none` 清除]"),
        )
    } else {
        localized(
            i18n,
            "Pushover sound, or press Enter to use your account default",
            "Pushover sound。使用账号默认声音就按 Enter",
        )
        .to_string()
    };
    let input = prompt_text(prompt, "failed to read Pushover sound")?;

    let input = input.trim();
    if input.is_empty() {
        return Ok(current_sound.map(ToOwned::to_owned));
    }
    if input.eq_ignore_ascii_case("none") {
        return Ok(None);
    }

    Ok(setup::resolve_pushover_sound(input))
}

fn resolve_config_path(config: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match config {
        Some(path) => Ok(path),
        None => default_config_file_path(),
    }
}

async fn run_start_service(
    config_path: &Path,
    loaded: LoadedConfig,
    force_restart: bool,
) -> anyhow::Result<()> {
    let i18n = I18n::new(loaded.config.cli.language);
    ensure_sources_supported_on_current_platform(&loaded.config)?;
    build_providers(&loaded.config).context("provider setup failed")?;
    if loaded.guided_setup.is_some() {
        println!();
    }
    let progress = ProgressLine::start("Starting agents-notifier")?;
    let start_result = async {
        cleanup_legacy_background_service()?;

        let manager = PlatformServiceManager::system()?;
        let definition = build_service_definition(config_path)?;
        let metadata_path = service_metadata_path()?;
        let outcome = if force_restart {
            manager.stop()?;
            cleanup_ingress_endpoint_if_exists()?;
            manager.install_or_update(&definition, &metadata_path)?
        } else {
            manager.install_or_update(&definition, &metadata_path)?
        };
        wait_for_service(&ingress_endpoint()?).await?;

        let status = manager.status()?;
        anyhow::Ok((outcome, status.pid, definition.log_file, status.platform))
    }
    .await;

    let (outcome, pid, log_file, manager_name) = match start_result {
        Ok(started) => {
            progress.finish(ProgressOutcome::Ready)?;
            started
        }
        Err(error) => {
            progress.finish(ProgressOutcome::Failed)?;
            return Err(error);
        }
    };

    print_service_start_outcome(outcome, pid, &log_file, manager_name, i18n);

    if let Some(setup) = loaded.guided_setup {
        finish_guided_setup(setup, i18n).await?;
    } else {
        print_notification_targets(&loaded.config, i18n);
        offer_test_notification(&loaded.config, i18n).await?;
    }

    Ok(())
}

fn build_service_definition(config_path: &Path) -> anyhow::Result<ServiceDefinition> {
    let binary_path = resolved_current_exe()?;
    let working_dir = std::env::current_dir().context("failed to detect working directory")?;
    let config_path = if config_path.is_absolute() {
        config_path.to_path_buf()
    } else {
        working_dir.join(config_path)
    };
    let log_file = log_file_path()?;
    let home = home_env_value()?;
    let env_path = std::env::var("PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_service_env_path);

    Ok(ServiceDefinition {
        binary_path,
        config_path,
        working_dir,
        log_file,
        home,
        env_path,
    })
}

fn default_service_env_path() -> String {
    #[cfg(windows)]
    return r"C:\Windows\System32;C:\Windows;C:\Windows\System32\WindowsPowerShell\v1.0"
        .to_string();

    #[cfg(not(windows))]
    return "/usr/local/bin:/usr/bin:/bin:/opt/homebrew/bin".to_string();
}

fn home_env_value() -> anyhow::Result<String> {
    #[cfg(windows)]
    let name = "USERPROFILE";

    #[cfg(not(windows))]
    let name = "HOME";

    std::env::var(name).with_context(|| format!("{name} is not set"))
}

fn resolved_current_exe() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    Ok(fs::canonicalize(&exe).unwrap_or(exe))
}

fn print_service_start_outcome(
    outcome: ServiceStartOutcome,
    pid: Option<u32>,
    log_file: &Path,
    manager_name: &str,
    i18n: I18n,
) {
    print_section(i18n.text(Text::ServiceSection));
    match outcome {
        ServiceStartOutcome::AlreadyRunning => {
            print_success_field("status", i18n.text(Text::ServiceStatusAlreadyRunning));
        }
        ServiceStartOutcome::Started => {
            print_success_field("status", i18n.text(Text::ServiceStatusRunning));
        }
        ServiceStartOutcome::UpdatedAndStarted => {
            print_success_field("status", i18n.text(Text::ServiceStatusUpdatedAndRestarted));
        }
    }
    print_field("manager", manager_name);
    if let Some(pid) = pid {
        print_field("pid", pid);
    }
    print_path_field("log", log_file.display());
}

fn print_notification_targets(config: &Config, i18n: I18n) {
    let agents = configured_agents(config);
    let subscriptions = setup::ntfy_subscriptions(config);
    let feishu_lark_targets = setup::feishu_lark_targets(config);
    let webhook_targets = setup::webhook_targets(config);
    let pushover_targets = setup::pushover_targets(config);
    let slack_targets = setup::slack_targets(config);
    let discord_targets = setup::discord_targets(config);
    let telegram_targets = setup::telegram_targets(config);
    let whatsapp_targets = setup::whatsapp_targets(config);
    let weixin_targets = setup::weixin_targets(config);
    let microsoft_teams_targets = setup::microsoft_teams_targets(config);
    let email_smtp_targets = setup::email_smtp_targets(config);

    print_section(i18n.text(Text::NotificationsSection));
    print_field(
        i18n.text(Text::AnswerDetailField),
        i18n.answer_detail(config.notification.answer_detail),
    );
    print_field(
        i18n.text(Text::PromptDetailField),
        i18n.prompt_detail(config.notification.prompt_detail),
    );

    if !agents.is_empty() {
        print_section(i18n.text(Text::WatchedAgentsSection));
        for agent in &agents {
            print_success_field(i18n.text(Text::AgentField), agent);
        }
    }

    if !subscriptions.is_empty() {
        print_section(i18n.text(Text::PhoneSubscriptionSection));
        for subscription in &subscriptions {
            print_field("server", &subscription.server);
            print_field("topic", &subscription.topic);
        }
    }

    if !feishu_lark_targets.is_empty() {
        print_section("Feishu/Lark custom bot");
        for target in &feishu_lark_targets {
            print_field("provider", &target.provider_id);
            print_field("webhook", &target.webhook_host);
            print_field(
                "signature verification",
                if target.signed {
                    style(i18n.text(Text::Configured)).green()
                } else {
                    style(i18n.text(Text::NotConfigured)).yellow()
                },
            );
        }
    }

    if !webhook_targets.is_empty() {
        print_section("Webhook");
        for target in &webhook_targets {
            print_field("provider", &target.provider_id);
            print_field("endpoint", &target.webhook_host);
        }
    }

    if !pushover_targets.is_empty() {
        print_section("Pushover");
        for target in &pushover_targets {
            print_field("provider", &target.provider_id);
            print_field(
                "device",
                target
                    .device
                    .as_deref()
                    .unwrap_or(i18n.text(Text::AllDevices)),
            );
            print_field(
                "sound",
                target
                    .sound
                    .as_deref()
                    .unwrap_or(i18n.text(Text::AccountDefault)),
            );
        }
    }

    if !slack_targets.is_empty() {
        print_section("Slack");
        for target in &slack_targets {
            print_field("provider", &target.provider_id);
            print_field("webhook", &target.webhook_host);
        }
    }

    if !discord_targets.is_empty() {
        print_section("Discord");
        for target in &discord_targets {
            print_field("provider", &target.provider_id);
            print_field("webhook", &target.webhook_host);
        }
    }

    if !telegram_targets.is_empty() {
        print_section("Telegram");
        for target in &telegram_targets {
            print_field("provider", &target.provider_id);
            print_field("chat id", &target.chat_id);
        }
    }

    if !whatsapp_targets.is_empty() {
        print_section("WhatsApp");
        for target in &whatsapp_targets {
            print_field("provider", &target.provider_id);
            print_field("recipient", &target.recipient_phone_number);
        }
    }

    if !weixin_targets.is_empty() {
        print_section("WeChat");
        for target in &weixin_targets {
            print_field("provider", &target.provider_id);
            print_field("endpoint", &target.base_url_host);
            print_field("recipient", &target.recipient_user_id);
        }
    }

    if !microsoft_teams_targets.is_empty() {
        print_section("Microsoft Teams");
        for target in &microsoft_teams_targets {
            print_field("provider", &target.provider_id);
            print_field("webhook", &target.webhook_host);
        }
    }

    if !email_smtp_targets.is_empty() {
        print_section("Email SMTP");
        for target in &email_smtp_targets {
            print_field("provider", &target.provider_id);
            print_field("server", format!("{}:{}", target.host, target.port));
            print_field("from", &target.from);
            print_field("to", target.to.join(", "));
        }
    }
}

fn configured_agents(config: &Config) -> Vec<String> {
    config
        .sources
        .iter()
        .filter_map(|source| match source.source_type {
            SourceType::CodexDesktop => Some("Codex Desktop".to_string()),
            SourceType::CodexCli => Some("Codex CLI".to_string()),
            SourceType::ClaudeCode => Some("Claude Code".to_string()),
            SourceType::AgentHook => Some(
                setup::AgentSelection::from_hook_source_id(&source.id)
                    .map(|agent| agent.display_name().to_string())
                    .unwrap_or_else(|| format!("Agent hook ({})", source.id)),
            ),
            SourceType::AgentsNotifier => None,
        })
        .collect()
}

fn first_configured_agent(config: &Config) -> Option<setup::AgentSelection> {
    config
        .sources
        .iter()
        .find_map(|source| match &source.source_type {
            SourceType::CodexDesktop => Some(setup::AgentSelection::CodexDesktop),
            SourceType::CodexCli => Some(setup::AgentSelection::CodexCli),
            SourceType::ClaudeCode => Some(setup::AgentSelection::ClaudeCode),
            SourceType::AgentHook => setup::AgentSelection::from_hook_source_id(&source.id),
            SourceType::AgentsNotifier => None,
        })
}

fn first_configured_provider(config: &Config) -> Option<InitialProvider> {
    config
        .providers
        .first()
        .map(|provider| match &provider.provider_type {
            ProviderType::Ntfy => InitialProvider::Ntfy,
            ProviderType::FeishuLark => InitialProvider::FeishuLark,
            ProviderType::Webhook => InitialProvider::Webhook,
            ProviderType::Pushover => InitialProvider::Pushover,
            ProviderType::Slack => InitialProvider::Slack,
            ProviderType::Discord => InitialProvider::Discord,
            ProviderType::Telegram => InitialProvider::Telegram,
            ProviderType::Whatsapp => InitialProvider::Whatsapp,
            ProviderType::Weixin => InitialProvider::Weixin,
            ProviderType::MicrosoftTeams => InitialProvider::MicrosoftTeams,
            ProviderType::EmailSmtp => InitialProvider::EmailSmtp,
        })
}

fn first_provider_of_type(config: &Config, provider_type: ProviderType) -> Option<&ProviderConfig> {
    config
        .providers
        .iter()
        .find(|provider| provider.provider_type.as_str() == provider_type.as_str())
}

fn configured_provider_url(provider: &ProviderConfig) -> Option<String> {
    provider.url.clone()
}

fn configured_provider_secret(provider: &ProviderConfig) -> Option<String> {
    provider.secret.clone()
}

fn email_smtp_security_display(security: EmailSmtpSecurity) -> &'static str {
    match security {
        EmailSmtpSecurity::Starttls => "STARTTLS",
        EmailSmtpSecurity::ImplicitTls => "Implicit TLS",
    }
}

fn default_email_smtp_port(security: EmailSmtpSecurity) -> u16 {
    match security {
        EmailSmtpSecurity::Starttls => 587,
        EmailSmtpSecurity::ImplicitTls => 465,
    }
}

fn safe_url_host(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "configured".to_string())
}

fn print_status_notification_targets(config_path: &Path) {
    match Config::from_path(config_path) {
        Ok(config) => {
            let i18n = I18n::new(config.cli.language);
            print_notification_targets(&config, i18n);
        }
        Err(error) => {
            print_section("Config");
            print_field("status", style(error.to_string()).red());
        }
    }
}

fn run_status() -> anyhow::Result<()> {
    let manager = PlatformServiceManager::system()?;
    let service_file = manager.service_file_path()?;
    let metadata_path = service_metadata_path()?;
    let endpoint = ingress_endpoint()?;
    let status = manager.status()?;
    let metadata = load_metadata(&metadata_path).ok();

    print_heading("agents-notifier status");
    print_section("Service");
    print_field("manager", status.platform);
    print_bool_field("installed", status.installed);
    print_bool_field("running", status.running);
    if let Some(pid) = status.pid {
        print_field("pid", pid);
    }
    if let Some(service_file) = service_file {
        print_path_field("service file", service_file.display());
    }
    print_path_field("ingress", endpoint.display());
    print_bool_field(
        "ingress ready",
        local_ingress_ready_now(&endpoint, status.running),
    );

    let config_path = metadata
        .as_ref()
        .map(|metadata| PathBuf::from(&metadata.config_path))
        .unwrap_or(default_config_file_path()?);
    let log_file = metadata
        .as_ref()
        .map(|metadata| PathBuf::from(&metadata.log_file))
        .unwrap_or(log_file_path()?);

    print_section("Files");
    print_path_field("config", config_path.display());
    print_path_field("log", log_file.display());
    print_status_notification_targets(&config_path);

    Ok(())
}

fn local_ingress_ready_now(
    endpoint: &agents_notifier::paths::IngressEndpoint,
    service_running: bool,
) -> bool {
    #[cfg(unix)]
    let _ = service_running;

    match endpoint {
        #[cfg(unix)]
        agents_notifier::paths::IngressEndpoint::UnixSocket(path) => path.exists(),
        #[cfg(windows)]
        agents_notifier::paths::IngressEndpoint::WindowsNamedPipe(_) => service_running,
    }
}

#[cfg(not(target_os = "macos"))]
fn cleanup_legacy_background_service() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn cleanup_legacy_background_service() -> anyhow::Result<()> {
    let pid_file = pid_file_path()?;
    match stop_with_manager(&pid_file, &SystemProcessManager)? {
        StopOutcome::MissingPidFile => {}
        StopOutcome::RemovedStalePidFile { .. } | StopOutcome::Stopped { .. } => {
            cleanup_ingress_endpoint_if_exists()?;
        }
    }

    Ok(())
}

async fn finish_guided_setup(setup: GuidedSetup, i18n: I18n) -> anyhow::Result<()> {
    let endpoint = ingress_endpoint()?;
    wait_for_service(&endpoint).await?;

    match setup {
        GuidedSetup::Ntfy { agent, topic } => {
            println!();
            println!("{}", i18n.text(Text::NextPhone));
            println!("{}", i18n.text(Text::NtfyFinishStep1));
            println!("{}", i18n.text(Text::NtfyFinishStep2));
            println!("{}", i18n.text(Text::NtfyFinishStep3));
            println!("4. Topic: {topic}");
            println!();
            wait_for_enter(i18n.text(Text::SendTestPromptNtfy))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::FeishuLark { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextFeishuLark));
            wait_for_enter(i18n.text(Text::SendTestPromptFeishuLark))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::Webhook { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextWebhook));
            wait_for_enter(i18n.text(Text::SendTestPromptWebhook))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::Pushover { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextPushover));
            wait_for_enter(i18n.text(Text::SendTestPromptPushover))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::Slack { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextSlack));
            wait_for_enter(i18n.text(Text::SendTestPromptSlack))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::Discord { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextDiscord));
            wait_for_enter(i18n.text(Text::SendTestPromptDiscord))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::Telegram { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextTelegram));
            wait_for_enter(i18n.text(Text::SendTestPromptTelegram))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::Whatsapp { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextWhatsapp));
            wait_for_enter(i18n.text(Text::SendTestPromptWhatsapp))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::Weixin { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextWeixin));
            wait_for_enter(i18n.text(Text::SendTestPromptWeixin))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::MicrosoftTeams { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextTeams));
            wait_for_enter(i18n.text(Text::SendTestPromptTeams))?;
            print_agent_setup_note(agent, i18n);
        }
        GuidedSetup::EmailSmtp { agent } => {
            println!();
            println!("{}", i18n.text(Text::NextEmail));
            wait_for_enter(i18n.text(Text::SendTestPromptEmail))?;
            print_agent_setup_note(agent, i18n);
        }
    }

    send_test_notification().await?;

    println!("{}", style(i18n.text(Text::TestSent)).green());
    if prompt_confirm(i18n.text(Text::DidItArrive), true)? {
        println!("{}", style(i18n.text(Text::SetupComplete)).green());
    } else {
        println!("{}", style(i18n.text(Text::TestDidNotArrive)).yellow());
        println!("{}", i18n.text(Text::CheckProviderSettingsAndRetest));
        println!(
            "agents-notifier emit --source agents_notifier --title \"Agents Notifier\" --body \"Test notification from your computer.\""
        );
    }

    Ok(())
}

fn print_agent_setup_note(agent: setup::AgentSelection, i18n: I18n) {
    if i18n.language() == CliLanguage::SimplifiedChinese {
        match agent {
            setup::AgentSelection::CodexDesktop => {
                println!("Agents Notifier 会监听这台电脑上的 Codex Desktop 完成事件。");
            }
            setup::AgentSelection::CodexCli => {
                println!("Agents Notifier 已准备接收 Codex CLI hook event。");
                println!("让 Codex CLI hook 调用：`agents-notifier emit --source codex_cli`。");
            }
            setup::AgentSelection::ClaudeCode => {
                println!("Agents Notifier 已准备接收 Claude Code hook event。");
                println!("让 Claude Code hook 调用：`agents-notifier emit --source claude_code`。");
            }
            setup::AgentSelection::CursorCli => {
                println!("Agents Notifier 已准备接收 Cursor CLI hook event。");
                println!(
                    "让 Cursor CLI wrapper 在 CLI 成功退出后调用：`agents-notifier emit --source cursor_cli`。"
                );
            }
            setup::AgentSelection::OpenCodeCli => {
                println!("Agents Notifier 已准备接收 OpenCode CLI hook event。");
                println!(
                    "让 OpenCode plugin 在 session idle 时调用：`agents-notifier emit --source opencode_cli`。"
                );
            }
            setup::AgentSelection::OpenClaw => {
                println!("Agents Notifier 已准备接收 OpenClaw hook event。");
                println!(
                    "让 OpenClaw plugin hook 在 agent_end 调用：`agents-notifier emit --source openclaw`。"
                );
            }
            setup::AgentSelection::HermesAgentCli => {
                println!("Agents Notifier 已准备接收 Hermes Agent CLI hook event。");
                println!(
                    "让 Hermes plugin hook 在 post_llm_call 调用：`agents-notifier emit --source hermes_agent_cli`。"
                );
            }
            setup::AgentSelection::GithubCopilotCli => {
                println!("Agents Notifier 已准备接收 GitHub Copilot CLI hook event。");
                println!(
                    "让 GitHub Copilot CLI notification hook 调用：`agents-notifier emit --source github_copilot_cli`。"
                );
            }
            setup::AgentSelection::GeminiCli => {
                println!("Agents Notifier 已准备接收 Gemini CLI hook event。");
                println!(
                    "让 Gemini CLI AfterAgent 或 Notification hook 调用：`agents-notifier emit --source gemini_cli`。"
                );
            }
            setup::AgentSelection::Aider => {
                println!("Agents Notifier 已准备接收 Aider notification event。");
                println!(
                    "让 Aider notifications-command 调用：`agents-notifier emit --source aider`。"
                );
            }
        }
        return;
    }

    match agent {
        setup::AgentSelection::CodexDesktop => {
            println!("Agents Notifier will watch Codex Desktop completed jobs on this computer.");
        }
        setup::AgentSelection::CodexCli => {
            println!("Agents Notifier is ready for Codex CLI hook events.");
            println!(
                "Configure your Codex CLI hook to call `agents-notifier emit --source codex_cli`."
            );
        }
        setup::AgentSelection::ClaudeCode => {
            println!("Agents Notifier is ready for Claude Code hook events.");
            println!(
                "Configure your Claude Code hook to call `agents-notifier emit --source claude_code`."
            );
        }
        setup::AgentSelection::CursorCli => {
            println!("Agents Notifier is ready for Cursor CLI hook events.");
            println!(
                "Configure your Cursor CLI wrapper to call `agents-notifier emit --source cursor_cli` after the CLI exits successfully."
            );
        }
        setup::AgentSelection::OpenCodeCli => {
            println!("Agents Notifier is ready for OpenCode CLI hook events.");
            println!(
                "Configure your OpenCode plugin to call `agents-notifier emit --source opencode_cli` when the session becomes idle."
            );
        }
        setup::AgentSelection::OpenClaw => {
            println!("Agents Notifier is ready for OpenClaw hook events.");
            println!(
                "Configure your OpenClaw plugin hook to call `agents-notifier emit --source openclaw` from agent_end."
            );
        }
        setup::AgentSelection::HermesAgentCli => {
            println!("Agents Notifier is ready for Hermes Agent CLI hook events.");
            println!(
                "Configure your Hermes plugin hook to call `agents-notifier emit --source hermes_agent_cli` from post_llm_call."
            );
        }
        setup::AgentSelection::GithubCopilotCli => {
            println!("Agents Notifier is ready for GitHub Copilot CLI hook events.");
            println!(
                "Configure your GitHub Copilot CLI notification hook to call `agents-notifier emit --source github_copilot_cli`."
            );
        }
        setup::AgentSelection::GeminiCli => {
            println!("Agents Notifier is ready for Gemini CLI hook events.");
            println!(
                "Configure your Gemini CLI AfterAgent or Notification hook to call `agents-notifier emit --source gemini_cli`."
            );
        }
        setup::AgentSelection::Aider => {
            println!("Agents Notifier is ready for Aider notification events.");
            println!(
                "Configure Aider notifications-command to call `agents-notifier emit --source aider`."
            );
        }
    }
}

async fn offer_test_notification(config: &Config, i18n: I18n) -> anyhow::Result<()> {
    if !is_interactive_terminal() || !can_send_test_notification(config) {
        return Ok(());
    }

    if !prompt_confirm(i18n.text(Text::SendTestNow), true)? {
        println!("{}", setup::TEST_NOTIFICATION_SKIPPED_MESSAGE);
        return Ok(());
    }

    send_test_notification().await?;
    println!("{}", style(i18n.text(Text::TestSent)).green());
    if prompt_confirm(i18n.text(Text::DidItArrive), true)? {
        println!("{}", style(i18n.text(Text::Working)).green());
    } else {
        println!(
            "{}",
            style(i18n.text(Text::ServiceRunningButTestMissing)).yellow()
        );
        println!("{}", i18n.text(Text::CheckProviderSettingsPrinted));
    }

    Ok(())
}

fn can_send_test_notification(config: &Config) -> bool {
    config.source("agents_notifier").is_some()
        && config.routes.iter().any(|route| {
            route
                .sources
                .iter()
                .any(|source| source == "agents_notifier")
                && !route.providers.is_empty()
        })
}

async fn send_test_notification() -> anyhow::Result<()> {
    let endpoint = ingress_endpoint()?;
    wait_for_service(&endpoint).await?;

    local_ingress::submit_event(
        &endpoint,
        &LocalSignalEvent {
            source_id: "agents_notifier".to_string(),
            title: "Agents Notifier".to_string(),
            body:
                "Test notification from your computer. If this arrived, Agents Notifier is working."
                    .to_string(),
        },
    )
    .await
    .context("failed to send test notification through the local service")
}

async fn wait_for_service(
    endpoint: &agents_notifier::paths::IngressEndpoint,
) -> anyhow::Result<()> {
    let started = Instant::now();
    loop {
        if local_ingress::is_ready(endpoint).await {
            return Ok(());
        }

        if started.elapsed() >= SERVICE_READY_TIMEOUT {
            anyhow::bail!(
                "agents-notifier service did not become ready at `{}`; check the log file",
                endpoint.display()
            );
        }

        sleep(SERVICE_READY_POLL_INTERVAL).await;
    }
}

fn wait_for_enter(prompt: &str) -> anyhow::Result<()> {
    let _ = prompt_text(prompt, "failed to read input")?;
    Ok(())
}

fn run_stop() -> anyhow::Result<()> {
    init_tracing("info")?;
    let manager = PlatformServiceManager::system()?;
    match manager.stop()? {
        ServiceStopOutcome::NotInstalled => {
            println!("agents-notifier service is not installed.");
        }
        ServiceStopOutcome::AlreadyStopped => {
            println!("agents-notifier service is already stopped.");
        }
        ServiceStopOutcome::Stopped => {
            println!("agents-notifier service stopped.");
        }
    }

    cleanup_ingress_endpoint_if_exists()?;
    cleanup_legacy_background_service()?;

    Ok(())
}

fn run_uninstall() -> anyhow::Result<()> {
    init_tracing("info")?;
    let manager = PlatformServiceManager::system()?;
    manager.uninstall()?;
    cleanup_ingress_endpoint_if_exists()?;
    cleanup_legacy_background_service()?;

    remove_path_if_exists(&app_support_dir_path()?)?;
    if let Some(log_dir) = log_file_path()?.parent() {
        remove_path_if_exists(log_dir)?;
    }
    if let Some(config_dir) = default_config_file_path()?.parent() {
        remove_path_if_exists(config_dir)?;
    }

    let current_binary = std::env::current_exe().context("failed to locate current binary")?;
    let install_method = std::env::var("AGENTS_NOTIFIER_INSTALL_METHOD").ok();
    if should_remove_current_binary_for_install_method(&current_binary, install_method.as_deref()) {
        remove_path_if_exists(&current_binary)?;
    } else if install_method.as_deref() == Some("npm") {
        println!(
            "left npm-managed binary in place: {}",
            current_binary.display()
        );
        println!("Remove the npm package with: npm uninstall -g agents-notifier");
    } else {
        println!(
            "left development binary in place: {}",
            current_binary.display()
        );
    }

    println!("agents-notifier uninstalled.");
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect `{}`", path.display()))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove directory `{}`", path.display()))?;
    } else {
        fs::remove_file(path)
            .with_context(|| format!("failed to remove file `{}`", path.display()))?;
    }

    println!("removed: {}", path.display());
    Ok(())
}

#[cfg(test)]
fn should_remove_current_binary(path: &Path) -> bool {
    should_remove_current_binary_for_install_method(path, None)
}

fn should_remove_current_binary_for_install_method(
    path: &Path,
    install_method: Option<&str>,
) -> bool {
    if install_method == Some("npm") {
        return false;
    }

    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if file_name != "agents-notifier" && file_name != "agents-notifier.exe" {
        return false;
    }

    !path
        .components()
        .any(|component| component.as_os_str() == "target")
}

fn cleanup_ingress_endpoint_if_exists() -> anyhow::Result<()> {
    let endpoint = ingress_endpoint()?;
    local_ingress::cleanup_endpoint(&endpoint)
}

async fn run_watch(config: &Config) -> anyhow::Result<()> {
    ensure_sources_supported_on_current_platform(config)?;
    let providers = build_providers(config).context("provider setup failed")?;
    let provider_refs: Vec<&dyn Provider> = providers
        .iter()
        .map(|provider| provider.as_ref() as &dyn Provider)
        .collect();
    let endpoint = ingress_endpoint()?;
    let codex_desktop_source = codex_desktop_source(config);
    let needs_local_open_bridge = config
        .providers
        .iter()
        .any(|provider| provider.provider_type == ProviderType::FeishuLark);

    if let (Some(source), true) = (codex_desktop_source, needs_local_open_bridge) {
        tokio::select! {
            result = codex_desktop::watch(config, source, &provider_refs) => result,
            result = local_ingress::serve(config, &provider_refs, &endpoint) => result,
            result = local_open_bridge::serve() => result,
        }
    } else if let Some(source) = codex_desktop_source {
        tokio::select! {
            result = codex_desktop::watch(config, source, &provider_refs) => result,
            result = local_ingress::serve(config, &provider_refs, &endpoint) => result,
        }
    } else if needs_local_open_bridge {
        tokio::select! {
            result = local_ingress::serve(config, &provider_refs, &endpoint) => result,
            result = local_open_bridge::serve() => result,
        }
    } else {
        local_ingress::serve(config, &provider_refs, &endpoint).await
    }
}

fn codex_desktop_source(config: &Config) -> Option<&SourceConfig> {
    config
        .sources
        .iter()
        .find(|source| source.source_type == SourceType::CodexDesktop)
}

fn ensure_sources_supported_on_current_platform(config: &Config) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    if config
        .sources
        .iter()
        .any(|source| source.source_type == SourceType::CodexDesktop)
    {
        anyhow::bail!("Codex Desktop source is supported only on macOS and Windows");
    }

    let _ = config;
    Ok(())
}

async fn run_emit(source_id: &str, title: String, body: String) -> anyhow::Result<()> {
    let endpoint = ingress_endpoint()?;
    let event = LocalSignalEvent {
        source_id: source_id.to_string(),
        title,
        body,
    };
    local_ingress::submit_event(&endpoint, &event).await
}

fn init_tracing(level: &str) -> anyhow::Result<()> {
    let level = match level {
        "trace" => tracing::Level::TRACE,
        "debug" => tracing::Level::DEBUG,
        "info" => tracing::Level::INFO,
        "warn" => tracing::Level::WARN,
        "error" => tracing::Level::ERROR,
        value => anyhow::bail!("unsupported log level `{value}`"),
    };

    let _ = tracing_subscriber::fmt().with_max_level(level).try_init();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn uninstall_removes_installed_binary_paths() {
        assert!(should_remove_current_binary(Path::new(
            "/Users/tester/.local/bin/agents-notifier"
        )));
        assert!(should_remove_current_binary(Path::new(
            "/Users/tester/.cargo/bin/agents-notifier"
        )));
    }

    #[test]
    fn uninstall_keeps_development_binary_paths() {
        assert!(!should_remove_current_binary(Path::new(
            "/repo/target/debug/agents-notifier"
        )));
        assert!(!should_remove_current_binary(Path::new(
            "/repo/target/release/agents-notifier"
        )));
    }

    #[test]
    fn uninstall_keeps_npm_managed_binary_paths() {
        assert!(!should_remove_current_binary_for_install_method(
            Path::new(
                "/usr/local/lib/node_modules/agents-notifier-linux-x64-gnu/bin/agents-notifier"
            ),
            Some("npm")
        ));
        assert!(!should_remove_current_binary_for_install_method(
            Path::new(
                r"C:\Users\tester\AppData\Roaming\npm\node_modules\agents-notifier-win32-x64-msvc\bin\agents-notifier.exe"
            ),
            Some("npm")
        ));
    }

    #[test]
    fn setup_defaults_preserve_existing_config_answers() {
        let mut config = setup::build_feishu_lark_config(
            setup::AgentSelection::ClaudeCode,
            AnswerDetail::Full,
            PromptDetail::On,
            "https://open.larksuite.com/open-apis/bot/v2/hook/secret-token",
            Some("signing-secret".to_string()),
        );
        config.cli.language = CliLanguage::SimplifiedChinese;

        let defaults = SetupDefaults::from_config(&config);

        assert_eq!(defaults.language, Some(CliLanguage::SimplifiedChinese));
        assert_eq!(defaults.agent, Some(setup::AgentSelection::ClaudeCode));
        assert_eq!(defaults.answer_detail, Some(AnswerDetail::Full));
        assert_eq!(defaults.prompt_detail, Some(PromptDetail::On));
        assert_eq!(defaults.provider, Some(InitialProvider::FeishuLark));
        assert_eq!(
            defaults.feishu_lark_webhook_url.as_deref(),
            Some("https://open.larksuite.com/open-apis/bot/v2/hook/secret-token")
        );
        assert_eq!(
            defaults.feishu_lark_secret.as_deref(),
            Some("signing-secret")
        );
    }

    #[test]
    fn setup_defaults_do_not_inline_env_values() {
        let config = Config::from_toml_str(
            r#"
schema_version = 1

[notification]
answer_detail = "preview"

[[sources]]
id = "codex_desktop"
type = "codex_desktop"

[[sources]]
id = "agents_notifier"
type = "agents_notifier"

[[providers]]
id = "work_chat"
type = "feishu_lark"
url_env = "AGENTS_NOTIFIER_FEISHU_LARK_WEBHOOK_URL"
secret_env = "AGENTS_NOTIFIER_FEISHU_LARK_SECRET"

[[routes]]
sources = ["codex_desktop", "agents_notifier"]
providers = ["work_chat"]
"#,
        )
        .expect("test config should be valid");

        let defaults = SetupDefaults::from_config(&config);

        assert_eq!(defaults.provider, Some(InitialProvider::FeishuLark));
        assert_eq!(defaults.feishu_lark_webhook_url, None);
        assert_eq!(defaults.feishu_lark_secret, None);
    }

    #[test]
    fn setup_defaults_preserve_existing_pushover_config() {
        let config = setup::build_pushover_config(
            setup::AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "123456789012345678901234567890",
            "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ",
            Some("iphone".to_string()),
            Some("pushover".to_string()),
        );

        let defaults = SetupDefaults::from_config(&config);

        assert_eq!(defaults.provider, Some(InitialProvider::Pushover));
        assert_eq!(
            defaults.pushover_app_token.as_deref(),
            Some("123456789012345678901234567890")
        );
        assert_eq!(
            defaults.pushover_user_key.as_deref(),
            Some("ABCDEFGHIJABCDEFGHIJABCDEFGHIJ")
        );
        assert_eq!(defaults.pushover_device.as_deref(), Some("iphone"));
        assert_eq!(defaults.pushover_sound.as_deref(), Some("pushover"));
    }

    #[test]
    fn setup_defaults_preserve_existing_slack_and_discord_config() {
        let slack_config = setup::build_slack_config(
            setup::AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            &slack_test_url(),
        );
        let slack_defaults = SetupDefaults::from_config(&slack_config);

        assert_eq!(slack_defaults.provider, Some(InitialProvider::Slack));
        assert_eq!(
            slack_defaults.slack_webhook_url.as_deref(),
            Some(slack_test_url().as_str())
        );

        let discord_config = setup::build_discord_config(
            setup::AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "https://discord.com/api/webhooks/123456789012345678/token",
        );
        let discord_defaults = SetupDefaults::from_config(&discord_config);

        assert_eq!(discord_defaults.provider, Some(InitialProvider::Discord));
        assert_eq!(
            discord_defaults.discord_webhook_url.as_deref(),
            Some("https://discord.com/api/webhooks/123456789012345678/token")
        );
    }

    #[test]
    fn prompt_detail_is_forced_off_for_length_limited_providers() {
        let i18n = I18n::default();
        assert_eq!(
            prompt_detail_for_provider(InitialProvider::Ntfy, Some(PromptDetail::On), i18n)
                .expect("ntfy prompt detail should resolve"),
            PromptDetail::Off
        );
        assert_eq!(
            prompt_detail_for_provider(InitialProvider::Pushover, Some(PromptDetail::On), i18n)
                .expect("Pushover prompt detail should resolve"),
            PromptDetail::Off
        );
        assert_eq!(
            prompt_detail_for_provider(InitialProvider::Slack, Some(PromptDetail::On), i18n)
                .expect("Slack prompt detail should resolve"),
            PromptDetail::Off
        );
        assert_eq!(
            prompt_detail_for_provider(InitialProvider::Discord, Some(PromptDetail::On), i18n)
                .expect("Discord prompt detail should resolve"),
            PromptDetail::Off
        );
        assert_eq!(
            prompt_detail_for_provider(InitialProvider::Weixin, Some(PromptDetail::On), i18n)
                .expect("WeChat prompt detail should resolve"),
            PromptDetail::Off
        );
    }

    #[test]
    fn answer_detail_is_forced_to_preview_for_length_limited_providers() {
        let i18n = I18n::default();
        assert_eq!(
            answer_detail_for_provider(InitialProvider::Ntfy, Some(AnswerDetail::Full), i18n)
                .expect("ntfy answer detail should resolve"),
            AnswerDetail::Preview
        );
        assert_eq!(
            answer_detail_for_provider(InitialProvider::Pushover, Some(AnswerDetail::Full), i18n)
                .expect("Pushover answer detail should resolve"),
            AnswerDetail::Preview
        );
        assert_eq!(
            answer_detail_for_provider(InitialProvider::Slack, Some(AnswerDetail::Full), i18n)
                .expect("Slack answer detail should resolve"),
            AnswerDetail::Preview
        );
        assert_eq!(
            answer_detail_for_provider(InitialProvider::Discord, Some(AnswerDetail::Full), i18n)
                .expect("Discord answer detail should resolve"),
            AnswerDetail::Preview
        );
        assert_eq!(
            answer_detail_for_provider(InitialProvider::Weixin, Some(AnswerDetail::Full), i18n)
                .expect("WeChat answer detail should resolve"),
            AnswerDetail::Preview
        );
    }

    #[test]
    fn macos_and_windows_offer_codex_desktop_as_default_agent() {
        assert_eq!(
            default_agent_for_runtime_platform(RuntimePlatform::Macos),
            setup::AgentSelection::CodexDesktop
        );
        assert_eq!(
            default_agent_for_runtime_platform(RuntimePlatform::Windows),
            setup::AgentSelection::CodexDesktop
        );

        let macos_options = supported_agent_options_for_platform(RuntimePlatform::Macos);
        let windows_options = supported_agent_options_for_platform(RuntimePlatform::Windows);

        assert_eq!(
            macos_options.first(),
            Some(&("1", setup::AgentSelection::CodexDesktop))
        );
        assert_eq!(
            windows_options.first(),
            Some(&("1", setup::AgentSelection::CodexDesktop))
        );
    }

    #[test]
    fn linux_starts_setup_at_codex_cli() {
        assert_eq!(
            default_agent_for_runtime_platform(RuntimePlatform::Linux),
            setup::AgentSelection::CodexCli
        );

        let options = supported_agent_options_for_platform(RuntimePlatform::Linux);

        assert_eq!(
            options.first(),
            Some(&("1", setup::AgentSelection::CodexCli))
        );
        assert!(
            !options
                .iter()
                .any(|(_, agent)| *agent == setup::AgentSelection::CodexDesktop)
        );
    }

    #[test]
    fn setup_provider_summary_reports_feishu_lark_without_exposing_webhook_token() {
        let config = setup::build_feishu_lark_config(
            setup::AgentSelection::CodexDesktop,
            AnswerDetail::Full,
            PromptDetail::On,
            "https://open.larksuite.com/open-apis/bot/v2/hook/secret-token",
            None,
        );

        let summary = single_setup_provider_summary(&config);

        assert_eq!(
            summary,
            SetupProviderSummary {
                provider_name: "Feishu/Lark custom bot",
                fields: vec![
                    plain_summary_field("webhook", "open.larksuite.com".to_string()),
                    SetupProviderSummaryField {
                        label: "signature verification",
                        value: "not configured".to_string(),
                        tone: SetupProviderSummaryTone::Warning,
                    },
                ],
            }
        );
        assert!(!format!("{summary:?}").contains("secret-token"));
    }

    #[test]
    fn setup_provider_summary_reports_signature_without_exposing_secret() {
        let config = setup::build_feishu_lark_config(
            setup::AgentSelection::CodexDesktop,
            AnswerDetail::Full,
            PromptDetail::On,
            "https://open.larksuite.com/open-apis/bot/v2/hook/secret-token",
            Some("signing-secret".to_string()),
        );

        let summary = single_setup_provider_summary(&config);

        assert_eq!(
            summary.fields[1],
            SetupProviderSummaryField {
                label: "signature verification",
                value: "configured".to_string(),
                tone: SetupProviderSummaryTone::Success,
            }
        );
        let rendered = format!("{summary:?}");
        assert!(!rendered.contains("secret-token"));
        assert!(!rendered.contains("signing-secret"));
    }

    #[test]
    fn setup_provider_summary_reports_hidden_credentials_without_printing_values() {
        let telegram_config = setup::build_telegram_config(
            setup::AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "123456:test-token",
            "-1001234567890",
        );
        let telegram = single_setup_provider_summary(&telegram_config);

        assert_eq!(
            telegram.fields,
            vec![
                SetupProviderSummaryField {
                    label: "bot token",
                    value: "configured".to_string(),
                    tone: SetupProviderSummaryTone::Success,
                },
                plain_summary_field("chat id", "-1001234567890".to_string()),
            ]
        );
        assert!(!format!("{telegram:?}").contains("123456:test-token"));

        let pushover_config = setup::build_pushover_config(
            setup::AgentSelection::CodexDesktop,
            AnswerDetail::Preview,
            PromptDetail::Off,
            "123456789012345678901234567890",
            "ABCDEFGHIJABCDEFGHIJABCDEFGHIJ",
            Some("iphone".to_string()),
            Some("pushover".to_string()),
        );
        let pushover = single_setup_provider_summary(&pushover_config);

        assert_eq!(
            pushover.fields,
            vec![
                SetupProviderSummaryField {
                    label: "application token",
                    value: "configured".to_string(),
                    tone: SetupProviderSummaryTone::Success,
                },
                SetupProviderSummaryField {
                    label: "user or group key",
                    value: "configured".to_string(),
                    tone: SetupProviderSummaryTone::Success,
                },
                plain_summary_field("device", "iphone".to_string()),
                plain_summary_field("sound", "pushover".to_string()),
            ]
        );
        let rendered = format!("{pushover:?}");
        assert!(!rendered.contains("123456789012345678901234567890"));
        assert!(!rendered.contains("ABCDEFGHIJABCDEFGHIJABCDEFGHIJ"));
    }

    #[test]
    fn setup_provider_summary_reports_email_without_printing_password() {
        let config = setup::build_email_smtp_config(
            setup::AgentSelection::CodexDesktop,
            AnswerDetail::Full,
            PromptDetail::On,
            "smtp.example.com",
            587,
            EmailSmtpSecurity::Starttls,
            Some("alerts@example.com".to_string()),
            Some("smtp-password".to_string()),
            "Agents Notifier <alerts@example.com>",
            vec!["team@example.com".to_string()],
            Some("reply@example.com".to_string()),
        );

        let summary = single_setup_provider_summary(&config);

        assert_eq!(
            summary.fields,
            vec![
                plain_summary_field("server", "smtp.example.com:587".to_string()),
                plain_summary_field("security", "STARTTLS".to_string()),
                SetupProviderSummaryField {
                    label: "authentication",
                    value: "configured".to_string(),
                    tone: SetupProviderSummaryTone::Success,
                },
                plain_summary_field("from", "Agents Notifier <alerts@example.com>".to_string()),
                plain_summary_field("to", "team@example.com".to_string()),
                plain_summary_field("reply-to", "reply@example.com".to_string()),
            ]
        );
        assert!(!format!("{summary:?}").contains("smtp-password"));
    }

    #[test]
    fn safe_url_host_does_not_expose_webhook_token() {
        assert_eq!(
            safe_url_host("https://open.larksuite.com/open-apis/bot/v2/hook/secret-token"),
            "open.larksuite.com"
        );
    }

    fn single_setup_provider_summary(config: &Config) -> SetupProviderSummary {
        let mut summaries = setup_provider_summaries(config);
        assert_eq!(summaries.len(), 1);
        summaries.remove(0)
    }

    fn slack_test_url() -> String {
        format!(
            "https://hooks.slack.com/services/{}/{}/{}",
            "T00000000", "B00000000", "test-token"
        )
    }
}
