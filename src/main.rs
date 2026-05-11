use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::{Parser, Subcommand};
use tokio::time::sleep;

use agents_notifier::config::{
    AnswerDetail, Config, ConfigError, EmailSmtpSecurity, PromptDetail, ProviderConfig,
    ProviderType, SourceConfig, SourceType,
};
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
    stop: Option<mpsc::Sender<()>>,
    handle: Option<thread::JoinHandle<()>>,
}

enum ProgressOutcome {
    Ready,
    Failed,
}

impl ProgressLine {
    fn start(message: &'static str) -> anyhow::Result<Self> {
        if !io::stdout().is_terminal() {
            println!("{message}...");
            return Ok(Self {
                stop: None,
                handle: None,
            });
        }

        print!("{message}...");
        io::stdout().flush().context("failed to flush stdout")?;

        let (stop, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            loop {
                match receiver.recv_timeout(START_PROGRESS_INTERVAL) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {
                        print!(".");
                        let _ = io::stdout().flush();
                    }
                }
            }
        });

        Ok(Self {
            stop: Some(stop),
            handle: Some(handle),
        })
    }

    fn finish(mut self, outcome: ProgressOutcome) -> anyhow::Result<()> {
        let Some(stop) = self.stop.take() else {
            return Ok(());
        };
        let _ = stop.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        match outcome {
            ProgressOutcome::Ready => println!(" ready."),
            ProgressOutcome::Failed => println!(" failed."),
        }
        io::stdout().flush().context("failed to flush stdout")
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
    MicrosoftTeams,
    EmailSmtp,
}

#[derive(Debug, Clone, Default)]
struct SetupDefaults {
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
    fn past_tense(self) -> &'static str {
        match self {
            Self::Created => "Created",
            Self::Updated => "Updated",
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
            let loaded = load_config(&config_path, allow_setup)?;
            run_start_service(&config_path, loaded, false).await
        }
        Command::Setup {
            config: config_path,
        } => {
            let config_path = resolve_config_path(config_path)?;
            let loaded = run_provider_setup(&config_path)?;
            run_start_service(&config_path, loaded, true).await
        }
        Command::Watch {
            config: config_path,
        } => {
            let config_path = resolve_config_path(config_path)?;
            let loaded = load_config(&config_path, false)?;
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

fn load_config(path: &Path, allow_setup: bool) -> anyhow::Result<LoadedConfig> {
    match Config::from_path(path) {
        Ok(config) => Ok(LoadedConfig {
            config,
            guided_setup: None,
        }),
        Err(ConfigError::NotFound { path }) if allow_setup && is_interactive_terminal() => {
            run_first_start_setup(Path::new(&path))
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

fn run_first_start_setup(path: &Path) -> anyhow::Result<LoadedConfig> {
    println!("No agents-notifier config found at `{}`.", path.display());
    println!();
    println!("Starting setup now.");
    println!("Setup connects one agent to one notification provider.");
    println!();

    run_guided_provider_setup(path, ConfigWriteMode::Created, SetupDefaults::default())
}

fn run_provider_setup(path: &Path) -> anyhow::Result<LoadedConfig> {
    if !is_interactive_terminal() {
        anyhow::bail!(
            "`agents-notifier setup` must be run in an interactive terminal so you can choose an agent and a provider."
        );
    }

    println!("Set up agent notifications.");
    println!(
        "This replaces the notification, agent, provider, and route sections in `{}`.",
        path.display()
    );
    println!("It does not change Codex or other agent settings.");
    println!();

    let mode = if path.exists() {
        ConfigWriteMode::Updated
    } else {
        ConfigWriteMode::Created
    };
    let defaults = SetupDefaults::from_path(path)?;
    run_guided_provider_setup(path, mode, defaults)
}

fn run_guided_provider_setup(
    path: &Path,
    mode: ConfigWriteMode,
    defaults: SetupDefaults,
) -> anyhow::Result<LoadedConfig> {
    let agent = prompt_for_agent(defaults.agent)?;
    let provider = prompt_for_initial_provider(defaults.provider)?;
    let answer_detail = answer_detail_for_provider(provider, defaults.answer_detail)?;
    let prompt_detail = prompt_detail_for_provider(provider, defaults.prompt_detail)?;

    match provider {
        InitialProvider::Ntfy => {
            run_ntfy_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::FeishuLark => {
            run_feishu_lark_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::Webhook => {
            run_webhook_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::Pushover => {
            run_pushover_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::Slack => {
            run_slack_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::Discord => {
            run_discord_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::Telegram => {
            run_telegram_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::Whatsapp => {
            run_whatsapp_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::MicrosoftTeams => {
            run_microsoft_teams_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::EmailSmtp => {
            run_email_smtp_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
    }
}

fn answer_detail_for_provider(
    provider: InitialProvider,
    default: Option<AnswerDetail>,
) -> anyhow::Result<AnswerDetail> {
    match provider {
        InitialProvider::Ntfy => {
            println!();
            println!(
                "Answer detail is fixed to Preview for ntfy because ntfy has a documented message size limit. Agents Notifier will keep ntfy notifications short enough for reliable delivery."
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Pushover => {
            println!();
            println!(
                "Answer detail is fixed to Preview for Pushover because Pushover messages are limited to 1024 characters. Agents Notifier will keep Pushover notifications short enough for reliable delivery."
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Slack => {
            println!();
            println!(
                "Answer detail is fixed to Preview for Slack because Slack has documented message length and truncation limits. Agents Notifier will keep Slack notifications short enough for reliable delivery."
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Discord => {
            println!();
            println!(
                "Answer detail is fixed to Preview for Discord because Discord webhook content is limited to 2000 characters. Agents Notifier will keep Discord notifications short enough for reliable delivery."
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Telegram => {
            println!();
            println!(
                "Answer detail is fixed to Preview for Telegram because Telegram Bot API text messages are limited to 4096 characters. Agents Notifier will keep Telegram notifications short enough for reliable delivery."
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::Whatsapp => {
            println!();
            println!(
                "Answer detail is fixed to Preview for WhatsApp because Agents Notifier uses a 4096-character delivery guard for WhatsApp text bodies."
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::MicrosoftTeams => {
            println!();
            println!(
                "Answer detail is fixed to Preview for Microsoft Teams because incoming webhook messages have a 28 KB size limit. Agents Notifier will keep Teams notifications short enough for reliable delivery."
            );
            Ok(AnswerDetail::Preview)
        }
        InitialProvider::FeishuLark | InitialProvider::Webhook | InitialProvider::EmailSmtp => {
            prompt_for_answer_detail(default)
        }
    }
}

fn prompt_detail_for_provider(
    provider: InitialProvider,
    default: Option<PromptDetail>,
) -> anyhow::Result<PromptDetail> {
    match provider {
        InitialProvider::Ntfy => {
            println!();
            println!(
                "Prompt detail is disabled for ntfy because ntfy has a documented message size limit. Agents Notifier will keep prompts out of ntfy notifications for reliable delivery."
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Pushover => {
            println!();
            println!(
                "Prompt detail is disabled for Pushover because Pushover messages are limited to 1024 characters. Agents Notifier will keep prompts out of Pushover notifications for reliable delivery."
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Slack => {
            println!();
            println!(
                "Prompt detail is disabled for Slack because Slack has documented message length and truncation limits. Agents Notifier will keep prompts out of Slack notifications for reliable delivery."
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Discord => {
            println!();
            println!(
                "Prompt detail is disabled for Discord because Discord webhook content is limited to 2000 characters. Agents Notifier will keep prompts out of Discord notifications for reliable delivery."
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Telegram => {
            println!();
            println!(
                "Prompt detail is disabled for Telegram because Telegram Bot API text messages are limited to 4096 characters. Agents Notifier will keep prompts out of Telegram notifications for reliable delivery."
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::Whatsapp => {
            println!();
            println!(
                "Prompt detail is disabled for WhatsApp because Agents Notifier uses a 4096-character delivery guard for WhatsApp text bodies."
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::MicrosoftTeams => {
            println!();
            println!(
                "Prompt detail is disabled for Microsoft Teams because incoming webhook messages have a 28 KB size limit. Agents Notifier will keep prompts out of Teams notifications for reliable delivery."
            );
            Ok(PromptDetail::Off)
        }
        InitialProvider::FeishuLark | InitialProvider::Webhook | InitialProvider::EmailSmtp => {
            prompt_for_prompt_detail(default)
        }
    }
}

fn run_ntfy_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
) -> anyhow::Result<LoadedConfig> {
    let generated_topic = setup::generated_ntfy_topic();

    println!();
    println!("ntfy sends notifications to your phone through a topic subscription.");
    println!("Install the ntfy app on your phone, then subscribe to the topic below.");
    println!();

    let topic = prompt_for_ntfy_topic(&generated_topic, defaults.ntfy_topic.as_deref())?;
    let config = setup::build_ntfy_config(agent, answer_detail, prompt_detail, &topic);
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("ntfy server: https://ntfy.sh");
    println!("ntfy topic: {topic}");

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
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("Feishu/Lark sends notifications to a group through a custom bot webhook.");
    println!("Add a custom bot to the target group, then paste its webhook URL.");
    println!(
        "If you enable security, use Signature Verification. Keyword security can block notifications unless every message contains your keyword."
    );
    println!();

    let webhook_url =
        prompt_for_feishu_lark_webhook_url(defaults.feishu_lark_webhook_url.as_deref())?;
    let secret = prompt_for_feishu_lark_secret(defaults.feishu_lark_secret.as_deref())?;
    let config =
        setup::build_feishu_lark_config(agent, answer_detail, prompt_detail, &webhook_url, secret);
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("Feishu/Lark custom bot: configured");

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
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("Webhook sends every signal as JSON to your HTTPS endpoint.");
    println!("Use this for internal tools, automation platforms, or your own notification bridge.");
    println!();

    let webhook_url = prompt_for_webhook_url(defaults.webhook_url.as_deref())?;
    let config = setup::build_webhook_config(agent, answer_detail, prompt_detail, &webhook_url);
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("webhook: configured");

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
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!(
        "Pushover sends notifications to your phone and desktop through your Pushover account."
    );
    println!("Create a Pushover application, then paste its API token and your user or group key.");
    println!();

    let app_token = prompt_for_pushover_app_token(defaults.pushover_app_token.as_deref())?;
    let user_key = prompt_for_pushover_user_key(defaults.pushover_user_key.as_deref())?;
    let device = prompt_for_pushover_device(defaults.pushover_device.as_deref())?;
    let sound = prompt_for_pushover_sound(defaults.pushover_sound.as_deref())?;
    let config = setup::build_pushover_config(
        agent,
        answer_detail,
        prompt_detail,
        &app_token,
        &user_key,
        device,
        sound,
    );
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("Pushover: configured");

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
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("Slack sends notifications to one channel through an incoming webhook.");
    println!("Create a Slack app, enable Incoming Webhooks, then paste the channel webhook URL.");
    println!();

    let webhook_url = prompt_for_slack_webhook_url(defaults.slack_webhook_url.as_deref())?;
    let config = setup::build_slack_config(agent, answer_detail, prompt_detail, &webhook_url);
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("Slack: configured");

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
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("Discord sends notifications to one channel through a channel webhook.");
    println!("Create a Discord channel webhook, then paste its webhook URL.");
    println!();

    let webhook_url = prompt_for_discord_webhook_url(defaults.discord_webhook_url.as_deref())?;
    let config = setup::build_discord_config(agent, answer_detail, prompt_detail, &webhook_url);
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("Discord: configured");

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
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("Telegram sends notifications through a Telegram bot to one chat or channel.");
    println!(
        "Create a bot with BotFather, add it to the target chat if needed, then paste the bot token and chat id."
    );
    println!();

    let bot_token = prompt_for_telegram_bot_token(defaults.telegram_bot_token.as_deref())?;
    let chat_id = prompt_for_telegram_chat_id(defaults.telegram_chat_id.as_deref())?;
    let config =
        setup::build_telegram_config(agent, answer_detail, prompt_detail, &bot_token, &chat_id);
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("Telegram: configured");

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
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("WhatsApp sends notifications through the WhatsApp Business Platform Cloud API.");
    println!(
        "Use a WhatsApp Business phone number ID, a system user access token, and one recipient phone number."
    );
    println!();

    let access_token = prompt_for_whatsapp_access_token(defaults.whatsapp_access_token.as_deref())?;
    let phone_number_id =
        prompt_for_whatsapp_phone_number_id(defaults.whatsapp_phone_number_id.as_deref())?;
    let recipient_phone_number = prompt_for_whatsapp_recipient_phone_number(
        defaults.whatsapp_recipient_phone_number.as_deref(),
    )?;
    let config = setup::build_whatsapp_config(
        agent,
        answer_detail,
        prompt_detail,
        &access_token,
        &phone_number_id,
        &recipient_phone_number,
    );
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("WhatsApp: configured");

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::Whatsapp { agent }),
    })
}

fn run_microsoft_teams_setup(
    path: &Path,
    mode: ConfigWriteMode,
    agent: setup::AgentSelection,
    answer_detail: AnswerDetail,
    prompt_detail: PromptDetail,
    defaults: &SetupDefaults,
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!(
        "Microsoft Teams sends notifications to one chat or channel through an incoming webhook."
    );
    println!("Create a Teams workflow or incoming webhook, then paste its webhook URL.");
    println!();

    let webhook_url =
        prompt_for_microsoft_teams_webhook_url(defaults.microsoft_teams_webhook_url.as_deref())?;
    let config =
        setup::build_microsoft_teams_config(agent, answer_detail, prompt_detail, &webhook_url);
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("Microsoft Teams: configured");

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
) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("Email SMTP sends notifications through your SMTP server as plain text email.");
    println!("Use STARTTLS on port 587 unless your provider explicitly gives you port 465.");
    println!();

    let host = prompt_for_email_smtp_host(defaults.email_smtp_host.as_deref())?;
    let security = prompt_for_email_smtp_security(defaults.email_smtp_security)?;
    let default_port = defaults
        .email_smtp_port
        .unwrap_or_else(|| default_email_smtp_port(security));
    let port = prompt_for_email_smtp_port(default_port)?;
    let username = prompt_for_email_smtp_username(defaults.email_smtp_username.as_deref())?;
    let password = if username.is_some() {
        Some(prompt_for_email_smtp_password(
            defaults.email_smtp_password.as_deref(),
        )?)
    } else {
        None
    };
    let from = prompt_for_email_smtp_mailbox("From email", defaults.email_smtp_from.as_deref())?;
    let to = prompt_for_email_smtp_recipients(defaults.email_smtp_to.as_deref())?;
    let reply_to = prompt_for_email_smtp_optional_mailbox(
        "Reply-To email",
        defaults.email_smtp_reply_to.as_deref(),
    )?;
    let config = setup::build_email_smtp_config(
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
    setup::write_config(path, &config)?;

    println!();
    println!("{} config: {}", mode.past_tense(), path.display());
    println!("agent: {}", agent.display_name());
    println!("answer detail: {}", answer_detail.display_name());
    println!("prompt detail: {}", prompt_detail.display_name());
    println!("Email SMTP: configured");

    Ok(LoadedConfig {
        config,
        guided_setup: Some(GuidedSetup::EmailSmtp { agent }),
    })
}

fn prompt_for_agent(
    default: Option<setup::AgentSelection>,
) -> anyhow::Result<setup::AgentSelection> {
    loop {
        let options = supported_agent_options();
        let supported_default =
            default.filter(|agent| options.iter().any(|(_, candidate)| candidate == agent));
        let effective_default = supported_default.unwrap_or_else(default_agent_for_platform);

        println!("Which agent should Agents Notifier watch?");
        for (choice, agent) in &options {
            println!("{choice}. {}", agent.display_name());
        }
        if let Some(default) = supported_default {
            println!("Current: {}", default.display_name());
        }
        let choices = options
            .iter()
            .map(|(choice, _)| choice.to_string())
            .collect::<Vec<_>>()
            .join("/");
        print!(
            "Choose an agent [{choices}, {}]: ",
            choice_prompt_hint(
                supported_default.map(agent_choice),
                agent_choice(effective_default)
            )
        );
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read agent choice")?;

        match input.trim() {
            "" => return Ok(effective_default),
            value => {
                if let Some((_, agent)) = options.iter().find(|(choice, _)| *choice == value) {
                    return Ok(*agent);
                }
                println!("Please enter one of: {choices}.");
            }
        }
        println!();
    }
}

fn supported_agent_options() -> Vec<(&'static str, setup::AgentSelection)> {
    #[cfg(target_os = "macos")]
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

    #[cfg(not(target_os = "macos"))]
    return vec![
        ("1", setup::AgentSelection::CodexCli),
        ("2", setup::AgentSelection::ClaudeCode),
        ("3", setup::AgentSelection::CursorCli),
        ("4", setup::AgentSelection::OpenCodeCli),
        ("5", setup::AgentSelection::OpenClaw),
        ("6", setup::AgentSelection::HermesAgentCli),
        ("7", setup::AgentSelection::GithubCopilotCli),
        ("8", setup::AgentSelection::GeminiCli),
        ("9", setup::AgentSelection::Aider),
    ];
}

fn default_agent_for_platform() -> setup::AgentSelection {
    #[cfg(target_os = "macos")]
    return setup::AgentSelection::CodexDesktop;

    #[cfg(not(target_os = "macos"))]
    return setup::AgentSelection::CodexCli;
}

fn prompt_for_answer_detail(default: Option<AnswerDetail>) -> anyhow::Result<AnswerDetail> {
    loop {
        let effective_default = default.unwrap_or(AnswerDetail::Preview);

        println!("Answer detail?");
        println!("1. Preview (Recommended)");
        println!("2. Full Answer");
        if let Some(default) = default {
            println!("Current: {}", default.display_name());
        }
        print!(
            "Choose answer detail [1/2, {}]: ",
            choice_prompt_hint(
                default.map(answer_detail_choice),
                answer_detail_choice(effective_default)
            )
        );
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read answer detail choice")?;

        match input.trim() {
            "" => return Ok(effective_default),
            "1" => return Ok(AnswerDetail::Preview),
            "2" => return Ok(AnswerDetail::Full),
            _ => println!("Please enter 1 or 2."),
        }
        println!();
    }
}

fn prompt_for_prompt_detail(default: Option<PromptDetail>) -> anyhow::Result<PromptDetail> {
    loop {
        let effective_default = default.unwrap_or(PromptDetail::Off);

        println!("Include your prompt?");
        println!("1. No (Recommended)");
        println!("2. Yes");
        if let Some(default) = default {
            println!("Current: {}", default.display_name());
        }
        print!(
            "Choose prompt detail [1/2, {}]: ",
            choice_prompt_hint(
                default.map(prompt_detail_choice),
                prompt_detail_choice(effective_default)
            )
        );
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read prompt detail choice")?;

        match input.trim() {
            "" => return Ok(effective_default),
            "1" => return Ok(PromptDetail::Off),
            "2" => return Ok(PromptDetail::On),
            _ => println!("Please enter 1 or 2."),
        }
        println!();
    }
}

fn prompt_for_initial_provider(
    default: Option<InitialProvider>,
) -> anyhow::Result<InitialProvider> {
    loop {
        let effective_default = default.unwrap_or(InitialProvider::Ntfy);

        println!("Where should Agents Notifier send notifications?");
        println!("1. ntfy");
        println!("2. Slack");
        println!("3. Discord");
        println!("4. Pushover");
        println!("5. Feishu/Lark custom bot");
        println!("6. Webhook");
        println!("7. Telegram");
        println!("8. WhatsApp");
        println!("9. Microsoft Teams");
        println!("10. Email SMTP");
        if let Some(default) = default {
            println!("Current: {}", default.display_name());
        }
        print!(
            "Choose a provider [1/2/3/4/5/6/7/8/9/10, {}]: ",
            choice_prompt_hint(
                default.map(provider_choice),
                provider_choice(effective_default)
            )
        );
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read provider choice")?;

        match input.trim() {
            "" => return Ok(effective_default),
            "1" => return Ok(InitialProvider::Ntfy),
            "2" => return Ok(InitialProvider::Slack),
            "3" => return Ok(InitialProvider::Discord),
            "4" => return Ok(InitialProvider::Pushover),
            "5" => return Ok(InitialProvider::FeishuLark),
            "6" => return Ok(InitialProvider::Webhook),
            "7" => return Ok(InitialProvider::Telegram),
            "8" => return Ok(InitialProvider::Whatsapp),
            "9" => return Ok(InitialProvider::MicrosoftTeams),
            "10" => return Ok(InitialProvider::EmailSmtp),
            _ => println!("Please enter 1, 2, 3, 4, 5, 6, 7, 8, 9, or 10."),
        }
        println!();
    }
}

fn prompt_for_ntfy_topic(
    generated_topic: &str,
    current_topic: Option<&str>,
) -> anyhow::Result<String> {
    loop {
        let default_topic = current_topic.unwrap_or(generated_topic);
        if let Some(current_topic) = current_topic {
            print!("Topic [{current_topic}]: ");
        } else {
            print!("Press Enter to use topic `{generated_topic}`, or type a different topic: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read topic")?;

        match setup::resolve_ntfy_topic(&input, default_topic) {
            Ok(topic) => return Ok(topic),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_feishu_lark_webhook_url(current_url: Option<&str>) -> anyhow::Result<String> {
    loop {
        if let Some(current_url) = current_url {
            print!(
                "Webhook URL [configured: {}, press Enter to keep]: ",
                safe_url_host(current_url)
            );
        } else {
            print!("Webhook URL: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read webhook URL")?;

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

fn prompt_for_feishu_lark_secret(current_secret: Option<&str>) -> anyhow::Result<Option<String>> {
    if current_secret.is_some() {
        print!("Signing secret [configured, press Enter to keep, type `none` to clear]: ");
    } else {
        print!("Signing secret, or press Enter if you did not enable Signature Verification: ");
    }
    io::stdout().flush().context("failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read signing secret")?;

    let input = input.trim();
    if input.is_empty() {
        return Ok(current_secret.map(ToOwned::to_owned));
    }
    if input.eq_ignore_ascii_case("none") {
        return Ok(None);
    }

    Ok(setup::resolve_feishu_lark_secret(input))
}

fn prompt_for_webhook_url(current_url: Option<&str>) -> anyhow::Result<String> {
    loop {
        if let Some(current_url) = current_url {
            print!(
                "Webhook URL [configured: {}, press Enter to keep]: ",
                safe_url_host(current_url)
            );
        } else {
            print!("Webhook URL: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read webhook URL")?;

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

fn prompt_for_slack_webhook_url(current_url: Option<&str>) -> anyhow::Result<String> {
    loop {
        if let Some(current_url) = current_url {
            print!(
                "Slack webhook URL [configured: {}, press Enter to keep]: ",
                safe_url_host(current_url)
            );
        } else {
            print!("Slack webhook URL: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read Slack webhook URL")?;

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

fn prompt_for_discord_webhook_url(current_url: Option<&str>) -> anyhow::Result<String> {
    loop {
        if let Some(current_url) = current_url {
            print!(
                "Discord webhook URL [configured: {}, press Enter to keep]: ",
                safe_url_host(current_url)
            );
        } else {
            print!("Discord webhook URL: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read Discord webhook URL")?;

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

fn prompt_for_telegram_bot_token(current_token: Option<&str>) -> anyhow::Result<String> {
    loop {
        if current_token.is_some() {
            print!("Telegram bot token [configured, press Enter to keep]: ");
        } else {
            print!("Telegram bot token: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read Telegram bot token")?;

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

fn prompt_for_telegram_chat_id(current_chat_id: Option<&str>) -> anyhow::Result<String> {
    loop {
        if let Some(current_chat_id) = current_chat_id {
            print!("Telegram chat id [{current_chat_id}, press Enter to keep]: ");
        } else {
            print!("Telegram chat id, for example 123456789 or @channelusername: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read Telegram chat id")?;

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

fn prompt_for_whatsapp_access_token(current_token: Option<&str>) -> anyhow::Result<String> {
    loop {
        if current_token.is_some() {
            print!("WhatsApp access token [configured, press Enter to keep]: ");
        } else {
            print!("WhatsApp access token: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read WhatsApp access token")?;

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
) -> anyhow::Result<String> {
    loop {
        if let Some(current_phone_number_id) = current_phone_number_id {
            print!("WhatsApp phone number ID [{current_phone_number_id}, press Enter to keep]: ");
        } else {
            print!("WhatsApp phone number ID: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read WhatsApp phone number ID")?;

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
) -> anyhow::Result<String> {
    loop {
        if let Some(current_phone_number) = current_phone_number {
            print!(
                "WhatsApp recipient phone number [{current_phone_number}, press Enter to keep]: "
            );
        } else {
            print!("WhatsApp recipient phone number, digits only with country code: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read WhatsApp recipient phone number")?;

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

fn prompt_for_microsoft_teams_webhook_url(current_url: Option<&str>) -> anyhow::Result<String> {
    loop {
        if let Some(current_url) = current_url {
            print!(
                "Microsoft Teams webhook URL [configured: {}, press Enter to keep]: ",
                safe_url_host(current_url)
            );
        } else {
            print!("Microsoft Teams webhook URL: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read Microsoft Teams webhook URL")?;

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

fn prompt_for_email_smtp_host(current_host: Option<&str>) -> anyhow::Result<String> {
    loop {
        if let Some(current_host) = current_host {
            print!("SMTP host [{current_host}, press Enter to keep]: ");
        } else {
            print!("SMTP host, for example smtp.gmail.com: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read SMTP host")?;

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
) -> anyhow::Result<EmailSmtpSecurity> {
    loop {
        let effective_default = current_security.unwrap_or(EmailSmtpSecurity::Starttls);

        println!("SMTP security:");
        println!("1. STARTTLS on an SMTP submission connection (Recommended)");
        println!("2. Implicit TLS");
        if let Some(current_security) = current_security {
            println!("Current: {}", email_smtp_security_display(current_security));
        }
        print!(
            "Choose SMTP security [1/2, {}]: ",
            choice_prompt_hint(
                current_security.map(email_smtp_security_choice),
                email_smtp_security_choice(effective_default)
            )
        );
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read SMTP security")?;

        match input.trim() {
            "" => return Ok(effective_default),
            "1" => return Ok(EmailSmtpSecurity::Starttls),
            "2" => return Ok(EmailSmtpSecurity::ImplicitTls),
            _ => println!("Please enter 1 or 2."),
        }
        println!();
    }
}

fn prompt_for_email_smtp_port(default_port: u16) -> anyhow::Result<u16> {
    loop {
        print!("SMTP port [{default_port}, press Enter to keep]: ");
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read SMTP port")?;

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
) -> anyhow::Result<Option<String>> {
    loop {
        if let Some(current_username) = current_username {
            print!(
                "SMTP username [{current_username}, press Enter to keep, type `none` for no auth]: "
            );
        } else {
            print!("SMTP username, or press Enter for no auth: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read SMTP username")?;

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

fn prompt_for_email_smtp_password(current_password: Option<&str>) -> anyhow::Result<String> {
    loop {
        if current_password.is_some() {
            print!("SMTP password [configured, press Enter to keep]: ");
        } else {
            print!("SMTP password or SMTP API key: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read SMTP password")?;

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
) -> anyhow::Result<String> {
    loop {
        if let Some(current_mailbox) = current_mailbox {
            print!("{label} [{current_mailbox}, press Enter to keep]: ");
        } else {
            print!("{label}, for example Agents Notifier <alerts@example.com>: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .with_context(|| format!("failed to read {label}"))?;

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
) -> anyhow::Result<Option<String>> {
    loop {
        if let Some(current_mailbox) = current_mailbox {
            print!("{label} [{current_mailbox}, press Enter to keep, type `none` to clear]: ");
        } else {
            print!("{label}, or press Enter to skip: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .with_context(|| format!("failed to read {label}"))?;

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
) -> anyhow::Result<Vec<String>> {
    loop {
        if let Some(current_recipients) = current_recipients {
            print!(
                "Recipient emails [{}; press Enter to keep]: ",
                current_recipients.join(", ")
            );
        } else {
            print!("Recipient emails, comma-separated: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read recipient emails")?;

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

fn prompt_for_pushover_app_token(current_token: Option<&str>) -> anyhow::Result<String> {
    loop {
        if current_token.is_some() {
            print!("Pushover application API token [configured, press Enter to keep]: ");
        } else {
            print!("Pushover application API token: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read Pushover application API token")?;

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

fn prompt_for_pushover_user_key(current_user_key: Option<&str>) -> anyhow::Result<String> {
    loop {
        if current_user_key.is_some() {
            print!("Pushover user or group key [configured, press Enter to keep]: ");
        } else {
            print!("Pushover user or group key: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read Pushover user or group key")?;

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

fn prompt_for_pushover_device(current_device: Option<&str>) -> anyhow::Result<Option<String>> {
    loop {
        if let Some(current_device) = current_device {
            print!(
                "Pushover device [{current_device}, press Enter to keep, type `none` to clear]: "
            );
        } else {
            print!("Pushover device, or press Enter to send to all devices: ");
        }
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read Pushover device")?;

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

fn prompt_for_pushover_sound(current_sound: Option<&str>) -> anyhow::Result<Option<String>> {
    if let Some(current_sound) = current_sound {
        print!("Pushover sound [{current_sound}, press Enter to keep, type `none` to clear]: ");
    } else {
        print!("Pushover sound, or press Enter to use your account default: ");
    }
    io::stdout().flush().context("failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read Pushover sound")?;

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
    ensure_sources_supported_on_current_platform(&loaded.config)?;
    build_providers(&loaded.config).context("provider setup failed")?;
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

    print_service_start_outcome(outcome, pid, &log_file, manager_name);

    if let Some(setup) = loaded.guided_setup {
        finish_guided_setup(setup).await?;
    } else {
        print_notification_targets(&loaded.config);
        offer_test_notification(&loaded.config).await?;
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
) {
    match outcome {
        ServiceStartOutcome::AlreadyRunning => {
            println!("agents-notifier service is already running.");
        }
        ServiceStartOutcome::Started => {
            println!("agents-notifier service is running.");
        }
        ServiceStartOutcome::UpdatedAndStarted => {
            println!("agents-notifier service was updated and restarted.");
        }
    }
    println!("manager: {manager_name}");
    if let Some(pid) = pid {
        println!("pid: {pid}");
    }
    println!("log: {}", log_file.display());
}

fn print_notification_targets(config: &Config) {
    let agents = configured_agents(config);
    let subscriptions = setup::ntfy_subscriptions(config);
    let feishu_lark_targets = setup::feishu_lark_targets(config);
    let webhook_targets = setup::webhook_targets(config);
    let pushover_targets = setup::pushover_targets(config);
    let slack_targets = setup::slack_targets(config);
    let discord_targets = setup::discord_targets(config);
    let telegram_targets = setup::telegram_targets(config);
    let whatsapp_targets = setup::whatsapp_targets(config);
    let microsoft_teams_targets = setup::microsoft_teams_targets(config);
    let email_smtp_targets = setup::email_smtp_targets(config);

    println!();
    println!("Notification:");
    println!(
        "answer detail: {}",
        config.notification.answer_detail.display_name()
    );
    println!(
        "prompt detail: {}",
        config.notification.prompt_detail.display_name()
    );

    if !agents.is_empty() {
        println!();
        println!("Watched agents:");
        for agent in &agents {
            println!("agent: {agent}");
        }
    }

    if !subscriptions.is_empty() {
        if !agents.is_empty() {
            println!();
        }
        println!("Phone subscription:");
        for subscription in &subscriptions {
            println!("server: {}", subscription.server);
            println!("topic: {}", subscription.topic);
        }
    }

    if !feishu_lark_targets.is_empty() {
        if !agents.is_empty() || !subscriptions.is_empty() {
            println!();
        }
        println!("Feishu/Lark custom bot:");
        for target in &feishu_lark_targets {
            println!("provider: {}", target.provider_id);
            println!("webhook: {}", target.webhook_host);
            println!(
                "signature verification: {}",
                if target.signed {
                    "configured"
                } else {
                    "not configured"
                }
            );
        }
    }

    if !webhook_targets.is_empty() {
        if !agents.is_empty() || !subscriptions.is_empty() || !feishu_lark_targets.is_empty() {
            println!();
        }
        println!("Webhook:");
        for target in &webhook_targets {
            println!("provider: {}", target.provider_id);
            println!("endpoint: {}", target.webhook_host);
        }
    }

    if !pushover_targets.is_empty() {
        if !agents.is_empty()
            || !subscriptions.is_empty()
            || !feishu_lark_targets.is_empty()
            || !webhook_targets.is_empty()
        {
            println!();
        }
        println!("Pushover:");
        for target in &pushover_targets {
            println!("provider: {}", target.provider_id);
            println!(
                "device: {}",
                target.device.as_deref().unwrap_or("all devices")
            );
            println!(
                "sound: {}",
                target.sound.as_deref().unwrap_or("account default")
            );
        }
    }

    if !slack_targets.is_empty() {
        if !agents.is_empty()
            || !subscriptions.is_empty()
            || !feishu_lark_targets.is_empty()
            || !webhook_targets.is_empty()
            || !pushover_targets.is_empty()
        {
            println!();
        }
        println!("Slack:");
        for target in &slack_targets {
            println!("provider: {}", target.provider_id);
            println!("webhook: {}", target.webhook_host);
        }
    }

    if !discord_targets.is_empty() {
        if !agents.is_empty()
            || !subscriptions.is_empty()
            || !feishu_lark_targets.is_empty()
            || !webhook_targets.is_empty()
            || !pushover_targets.is_empty()
            || !slack_targets.is_empty()
        {
            println!();
        }
        println!("Discord:");
        for target in &discord_targets {
            println!("provider: {}", target.provider_id);
            println!("webhook: {}", target.webhook_host);
        }
    }

    if !telegram_targets.is_empty() {
        if !agents.is_empty()
            || !subscriptions.is_empty()
            || !feishu_lark_targets.is_empty()
            || !webhook_targets.is_empty()
            || !pushover_targets.is_empty()
            || !slack_targets.is_empty()
            || !discord_targets.is_empty()
        {
            println!();
        }
        println!("Telegram:");
        for target in &telegram_targets {
            println!("provider: {}", target.provider_id);
            println!("chat id: {}", target.chat_id);
        }
    }

    if !whatsapp_targets.is_empty() {
        if !agents.is_empty()
            || !subscriptions.is_empty()
            || !feishu_lark_targets.is_empty()
            || !webhook_targets.is_empty()
            || !pushover_targets.is_empty()
            || !slack_targets.is_empty()
            || !discord_targets.is_empty()
            || !telegram_targets.is_empty()
        {
            println!();
        }
        println!("WhatsApp:");
        for target in &whatsapp_targets {
            println!("provider: {}", target.provider_id);
            println!("recipient: {}", target.recipient_phone_number);
        }
    }

    if !microsoft_teams_targets.is_empty() {
        if !agents.is_empty()
            || !subscriptions.is_empty()
            || !feishu_lark_targets.is_empty()
            || !webhook_targets.is_empty()
            || !pushover_targets.is_empty()
            || !slack_targets.is_empty()
            || !discord_targets.is_empty()
            || !telegram_targets.is_empty()
            || !whatsapp_targets.is_empty()
        {
            println!();
        }
        println!("Microsoft Teams:");
        for target in &microsoft_teams_targets {
            println!("provider: {}", target.provider_id);
            println!("webhook: {}", target.webhook_host);
        }
    }

    if !email_smtp_targets.is_empty() {
        if !agents.is_empty()
            || !subscriptions.is_empty()
            || !feishu_lark_targets.is_empty()
            || !webhook_targets.is_empty()
            || !pushover_targets.is_empty()
            || !slack_targets.is_empty()
            || !discord_targets.is_empty()
            || !telegram_targets.is_empty()
            || !whatsapp_targets.is_empty()
            || !microsoft_teams_targets.is_empty()
        {
            println!();
        }
        println!("Email SMTP:");
        for target in &email_smtp_targets {
            println!("provider: {}", target.provider_id);
            println!("server: {}:{}", target.host, target.port);
            println!("from: {}", target.from);
            println!("to: {}", target.to.join(", "));
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

fn choice_prompt_hint(
    current_choice: Option<&'static str>,
    default_choice: &'static str,
) -> String {
    match current_choice {
        Some(choice) => format!("current: {choice}, press Enter to keep"),
        None => format!("default: {default_choice}"),
    }
}

fn agent_choice(agent: setup::AgentSelection) -> &'static str {
    supported_agent_options()
        .into_iter()
        .find_map(|(choice, candidate)| (candidate == agent).then_some(choice))
        .unwrap_or("?")
}

fn answer_detail_choice(answer_detail: AnswerDetail) -> &'static str {
    match answer_detail {
        AnswerDetail::Preview => "1",
        AnswerDetail::Full => "2",
    }
}

fn prompt_detail_choice(prompt_detail: PromptDetail) -> &'static str {
    match prompt_detail {
        PromptDetail::Off => "1",
        PromptDetail::On => "2",
    }
}

fn provider_choice(provider: InitialProvider) -> &'static str {
    match provider {
        InitialProvider::Ntfy => "1",
        InitialProvider::Slack => "2",
        InitialProvider::Discord => "3",
        InitialProvider::Pushover => "4",
        InitialProvider::FeishuLark => "5",
        InitialProvider::Webhook => "6",
        InitialProvider::Telegram => "7",
        InitialProvider::Whatsapp => "8",
        InitialProvider::MicrosoftTeams => "9",
        InitialProvider::EmailSmtp => "10",
    }
}

fn email_smtp_security_choice(security: EmailSmtpSecurity) -> &'static str {
    match security {
        EmailSmtpSecurity::Starttls => "1",
        EmailSmtpSecurity::ImplicitTls => "2",
    }
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
        Ok(config) => print_notification_targets(&config),
        Err(error) => {
            println!();
            println!("Config status: {error}");
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

    println!("agents-notifier status");
    println!();
    println!("manager: {}", status.platform);
    println!("installed: {}", yes_no(status.installed));
    println!("running: {}", yes_no(status.running));
    if let Some(pid) = status.pid {
        println!("pid: {pid}");
    }
    if let Some(service_file) = service_file {
        println!("service file: {}", service_file.display());
    }
    println!("ingress: {}", endpoint.display());
    println!(
        "ingress ready: {}",
        yes_no(local_ingress_ready_now(&endpoint, status.running))
    );

    let config_path = metadata
        .as_ref()
        .map(|metadata| PathBuf::from(&metadata.config_path))
        .unwrap_or(default_config_file_path()?);
    let log_file = metadata
        .as_ref()
        .map(|metadata| PathBuf::from(&metadata.log_file))
        .unwrap_or(log_file_path()?);

    println!("config: {}", config_path.display());
    println!("log: {}", log_file.display());
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

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
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

async fn finish_guided_setup(setup: GuidedSetup) -> anyhow::Result<()> {
    let endpoint = ingress_endpoint()?;
    wait_for_service(&endpoint).await?;

    match setup {
        GuidedSetup::Ntfy { agent, topic } => {
            println!();
            println!("Next: connect your phone.");
            println!("1. Open the ntfy app.");
            println!("2. Add a subscription.");
            println!("3. Server: https://ntfy.sh");
            println!("4. Topic: {topic}");
            println!();
            wait_for_enter(
                "After your phone is subscribed, press Enter to send a test notification.",
            )?;
            print_agent_setup_note(agent);
        }
        GuidedSetup::FeishuLark { agent } => {
            println!();
            println!("Next: check your Feishu/Lark group.");
            wait_for_enter("Press Enter to send a test notification to the custom bot.")?;
            print_agent_setup_note(agent);
        }
        GuidedSetup::Webhook { agent } => {
            println!();
            println!("Next: check your webhook receiver.");
            wait_for_enter("Press Enter to send a test JSON payload to the webhook.")?;
            print_agent_setup_note(agent);
        }
        GuidedSetup::Pushover { agent } => {
            println!();
            println!("Next: check your Pushover devices.");
            wait_for_enter("Press Enter to send a test notification through Pushover.")?;
            print_agent_setup_note(agent);
        }
        GuidedSetup::Slack { agent } => {
            println!();
            println!("Next: check your Slack channel.");
            wait_for_enter("Press Enter to send a test notification through Slack.")?;
            print_agent_setup_note(agent);
        }
        GuidedSetup::Discord { agent } => {
            println!();
            println!("Next: check your Discord channel.");
            wait_for_enter("Press Enter to send a test notification through Discord.")?;
            print_agent_setup_note(agent);
        }
        GuidedSetup::Telegram { agent } => {
            println!();
            println!("Next: check your Telegram chat.");
            wait_for_enter("Press Enter to send a test notification through Telegram.")?;
            print_agent_setup_note(agent);
        }
        GuidedSetup::Whatsapp { agent } => {
            println!();
            println!("Next: check the recipient WhatsApp chat.");
            wait_for_enter("Press Enter to send a test notification through WhatsApp.")?;
            print_agent_setup_note(agent);
        }
        GuidedSetup::MicrosoftTeams { agent } => {
            println!();
            println!("Next: check your Microsoft Teams chat or channel.");
            wait_for_enter("Press Enter to send a test notification through Microsoft Teams.")?;
            print_agent_setup_note(agent);
        }
        GuidedSetup::EmailSmtp { agent } => {
            println!();
            println!("Next: check the recipient email inbox.");
            wait_for_enter("Press Enter to send a test notification through Email SMTP.")?;
            print_agent_setup_note(agent);
        }
    }

    send_test_notification().await?;

    println!("Test notification sent.");
    if prompt_yes_no("Did it arrive? [Y/n] ")? {
        println!("Setup complete. Agent notifications can now be forwarded.");
    } else {
        println!("The service is still running, but the test notification did not arrive.");
        println!("Check the provider settings in your config, then run this test again:");
        println!(
            "agents-notifier emit --source agents_notifier --title \"Agents Notifier\" --body \"Test notification from your computer.\""
        );
    }

    Ok(())
}

fn print_agent_setup_note(agent: setup::AgentSelection) {
    match agent {
        setup::AgentSelection::CodexDesktop => {
            println!("Agents Notifier will watch Codex Desktop completed jobs on this Mac.");
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

async fn offer_test_notification(config: &Config) -> anyhow::Result<()> {
    if !is_interactive_terminal() || !can_send_test_notification(config) {
        return Ok(());
    }

    if !prompt_yes_no("Send a test notification now? [Y/n] ")? {
        println!("{}", setup::TEST_NOTIFICATION_SKIPPED_MESSAGE);
        return Ok(());
    }

    send_test_notification().await?;
    println!("Test notification sent.");
    if prompt_yes_no("Did it arrive? [Y/n] ")? {
        println!("Agents Notifier is working. Agent notifications can now be forwarded.");
    } else {
        println!("The service is running, but the test notification did not arrive.");
        println!("Check the provider settings printed above.");
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
    println!("{prompt}");
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read input")?;
    Ok(())
}

fn prompt_yes_no(prompt: &str) -> anyhow::Result<bool> {
    loop {
        print!("{prompt}");
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read input")?;
        match input.trim().to_ascii_lowercase().as_str() {
            "" | "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Please answer yes or no."),
        }
    }
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
    #[cfg(not(target_os = "macos"))]
    if config
        .sources
        .iter()
        .any(|source| source.source_type == SourceType::CodexDesktop)
    {
        return None;
    }

    config
        .sources
        .iter()
        .find(|source| source.source_type == SourceType::CodexDesktop)
}

fn ensure_sources_supported_on_current_platform(config: &Config) -> anyhow::Result<()> {
    #[cfg(not(target_os = "macos"))]
    if config
        .sources
        .iter()
        .any(|source| source.source_type == SourceType::CodexDesktop)
    {
        anyhow::bail!("Codex Desktop source is currently supported only on macOS");
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
        let config = setup::build_feishu_lark_config(
            setup::AgentSelection::ClaudeCode,
            AnswerDetail::Full,
            PromptDetail::On,
            "https://open.larksuite.com/open-apis/bot/v2/hook/secret-token",
            Some("signing-secret".to_string()),
        );

        let defaults = SetupDefaults::from_config(&config);

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
        assert_eq!(
            prompt_detail_for_provider(InitialProvider::Ntfy, Some(PromptDetail::On))
                .expect("ntfy prompt detail should resolve"),
            PromptDetail::Off
        );
        assert_eq!(
            prompt_detail_for_provider(InitialProvider::Pushover, Some(PromptDetail::On))
                .expect("Pushover prompt detail should resolve"),
            PromptDetail::Off
        );
        assert_eq!(
            prompt_detail_for_provider(InitialProvider::Slack, Some(PromptDetail::On))
                .expect("Slack prompt detail should resolve"),
            PromptDetail::Off
        );
        assert_eq!(
            prompt_detail_for_provider(InitialProvider::Discord, Some(PromptDetail::On))
                .expect("Discord prompt detail should resolve"),
            PromptDetail::Off
        );
    }

    #[test]
    fn answer_detail_is_forced_to_preview_for_length_limited_providers() {
        assert_eq!(
            answer_detail_for_provider(InitialProvider::Ntfy, Some(AnswerDetail::Full))
                .expect("ntfy answer detail should resolve"),
            AnswerDetail::Preview
        );
        assert_eq!(
            answer_detail_for_provider(InitialProvider::Pushover, Some(AnswerDetail::Full))
                .expect("Pushover answer detail should resolve"),
            AnswerDetail::Preview
        );
        assert_eq!(
            answer_detail_for_provider(InitialProvider::Slack, Some(AnswerDetail::Full))
                .expect("Slack answer detail should resolve"),
            AnswerDetail::Preview
        );
        assert_eq!(
            answer_detail_for_provider(InitialProvider::Discord, Some(AnswerDetail::Full))
                .expect("Discord answer detail should resolve"),
            AnswerDetail::Preview
        );
    }

    #[test]
    fn choice_prompt_hint_distinguishes_defaults_from_current_values() {
        assert_eq!(choice_prompt_hint(None, "1"), "default: 1");
        assert_eq!(
            choice_prompt_hint(Some("2"), "1"),
            "current: 2, press Enter to keep"
        );
    }

    #[test]
    fn safe_url_host_does_not_expose_webhook_token() {
        assert_eq!(
            safe_url_host("https://open.larksuite.com/open-apis/bot/v2/hook/secret-token"),
            "open.larksuite.com"
        );
    }

    fn slack_test_url() -> String {
        format!(
            "https://hooks.slack.com/services/{}/{}/{}",
            "T00000000", "B00000000", "test-token"
        )
    }
}
