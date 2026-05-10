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
    AnswerDetail, Config, ConfigError, PromptDetail, ProviderConfig, ProviderType, SourceConfig,
    SourceType,
};
use agents_notifier::local_ingress::{self, LocalSignalEvent};
use agents_notifier::local_open_bridge;
use agents_notifier::paths::{
    app_support_dir_path, default_config_file_path, launch_agent_plist_path, log_file_path,
    pid_file_path, service_metadata_path, socket_file_path,
};
use agents_notifier::process::{StopOutcome, SystemProcessManager, stop_with_manager};
use agents_notifier::providers::build_providers;
use agents_notifier::router::Provider;
use agents_notifier::service::{
    LaunchAgentManager, ServiceDefinition, ServiceStartOutcome, ServiceStopOutcome, load_metadata,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitialProvider {
    Ntfy,
    FeishuLark,
    Webhook,
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
    let answer_detail = prompt_for_answer_detail(defaults.answer_detail)?;
    let prompt_detail = prompt_for_prompt_detail(defaults.prompt_detail)?;
    match prompt_for_initial_provider(defaults.provider)? {
        InitialProvider::Ntfy => {
            run_ntfy_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::FeishuLark => {
            run_feishu_lark_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
        }
        InitialProvider::Webhook => {
            run_webhook_setup(path, mode, agent, answer_detail, prompt_detail, &defaults)
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

fn prompt_for_agent(
    default: Option<setup::AgentSelection>,
) -> anyhow::Result<setup::AgentSelection> {
    loop {
        println!("Which agent should Agents Notifier watch?");
        println!("1. Codex Desktop");
        println!("2. Codex CLI");
        println!("3. Claude Code");
        if let Some(default) = default {
            println!("Current: {}", default.display_name());
        }
        print!(
            "Choose an agent [1/2/3, default: {}]: ",
            agent_choice(default)
        );
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read agent choice")?;

        match input.trim() {
            "" => return Ok(default.unwrap_or(setup::AgentSelection::CodexDesktop)),
            "1" => return Ok(setup::AgentSelection::CodexDesktop),
            "2" => return Ok(setup::AgentSelection::CodexCli),
            "3" => return Ok(setup::AgentSelection::ClaudeCode),
            _ => println!("Please enter 1, 2, or 3."),
        }
        println!();
    }
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
            "Choose answer detail [1/2, default: {}]: ",
            answer_detail_choice(effective_default)
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
            "Choose prompt detail [1/2, default: {}]: ",
            prompt_detail_choice(effective_default)
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
        println!("Where should Agents Notifier send notifications?");
        println!("1. ntfy");
        println!("2. Feishu/Lark custom bot");
        println!("3. Webhook");
        if let Some(default) = default {
            println!("Current: {}", default.display_name());
        }
        print!(
            "Choose a provider [1/2/3, default: {}]: ",
            provider_choice(default)
        );
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read provider choice")?;

        match input.trim() {
            "" => return Ok(default.unwrap_or(InitialProvider::Ntfy)),
            "1" => return Ok(InitialProvider::Ntfy),
            "2" => return Ok(InitialProvider::FeishuLark),
            "3" => return Ok(InitialProvider::Webhook),
            _ => println!("Please enter 1, 2, or 3."),
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
    build_providers(&loaded.config).context("provider setup failed")?;
    let progress = ProgressLine::start("Starting agents-notifier")?;
    let start_result = async {
        cleanup_legacy_background_service()?;

        let manager = LaunchAgentManager::system()?;
        let definition = build_service_definition(config_path)?;
        let plist_path = launch_agent_plist_path()?;
        let metadata_path = service_metadata_path()?;
        let outcome = if force_restart {
            manager.stop(&plist_path)?;
            remove_socket_file_if_exists()?;
            manager.install_or_update(&definition, &plist_path, &metadata_path)?
        } else {
            manager.install_or_update(&definition, &plist_path, &metadata_path)?
        };
        wait_for_service(&socket_file_path()?).await?;

        let status = manager.status(&plist_path)?;
        anyhow::Ok((outcome, status.pid, definition.log_file))
    }
    .await;

    let (outcome, pid, log_file) = match start_result {
        Ok(started) => {
            progress.finish(ProgressOutcome::Ready)?;
            started
        }
        Err(error) => {
            progress.finish(ProgressOutcome::Failed)?;
            return Err(error);
        }
    };

    print_service_start_outcome(outcome, pid, &log_file);

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
    let log_file = log_file_path()?;
    let home = std::env::var("HOME").context("HOME is not set")?;
    let env_path = std::env::var("PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin:/opt/homebrew/bin".to_string());

    Ok(ServiceDefinition {
        binary_path,
        config_path: config_path.to_path_buf(),
        working_dir,
        log_file,
        home,
        env_path,
    })
}

fn resolved_current_exe() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    Ok(fs::canonicalize(&exe).unwrap_or(exe))
}

fn print_service_start_outcome(outcome: ServiceStartOutcome, pid: Option<u32>, log_file: &Path) {
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
    println!("manager: LaunchAgent");
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
}

fn configured_agents(config: &Config) -> Vec<&'static str> {
    config
        .sources
        .iter()
        .filter_map(|source| match source.source_type {
            SourceType::CodexDesktop => Some("Codex Desktop"),
            SourceType::CodexCli => Some("Codex CLI"),
            SourceType::ClaudeCode => Some("Claude Code"),
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

fn agent_choice(agent: Option<setup::AgentSelection>) -> &'static str {
    match agent.unwrap_or(setup::AgentSelection::CodexDesktop) {
        setup::AgentSelection::CodexDesktop => "1",
        setup::AgentSelection::CodexCli => "2",
        setup::AgentSelection::ClaudeCode => "3",
    }
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

fn provider_choice(provider: Option<InitialProvider>) -> &'static str {
    match provider.unwrap_or(InitialProvider::Ntfy) {
        InitialProvider::Ntfy => "1",
        InitialProvider::FeishuLark => "2",
        InitialProvider::Webhook => "3",
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
    let manager = LaunchAgentManager::system()?;
    let plist_path = launch_agent_plist_path()?;
    let metadata_path = service_metadata_path()?;
    let socket_path = socket_file_path()?;
    let status = manager.status(&plist_path)?;
    let metadata = load_metadata(&metadata_path).ok();

    println!("agents-notifier status");
    println!();
    println!("manager: {}", status.platform);
    println!("installed: {}", yes_no(status.installed));
    println!("running: {}", yes_no(status.running));
    if let Some(pid) = status.pid {
        println!("pid: {pid}");
    }
    println!("plist: {}", plist_path.display());
    println!("socket ready: {}", yes_no(socket_path.exists()));

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

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn cleanup_legacy_background_service() -> anyhow::Result<()> {
    let pid_file = pid_file_path()?;
    match stop_with_manager(&pid_file, &SystemProcessManager)? {
        StopOutcome::MissingPidFile => {}
        StopOutcome::RemovedStalePidFile { .. } | StopOutcome::Stopped { .. } => {
            remove_socket_file_if_exists()?;
        }
    }

    Ok(())
}

async fn finish_guided_setup(setup: GuidedSetup) -> anyhow::Result<()> {
    let socket_path = socket_file_path()?;
    wait_for_service(&socket_path).await?;

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
    }

    send_test_notification().await?;

    println!("Test notification sent.");
    if prompt_yes_no("Did it arrive? [Y/n] ")? {
        println!("Setup complete. Agent notifications can now be forwarded.");
    } else {
        println!("The service is still running, but the test notification did not arrive.");
        println!("Check the provider settings in your config, then run this test again:");
        println!(
            "agents-notifier emit --source agents_notifier --title \"Agents Notifier\" --body \"Test notification from your Mac.\""
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
    let socket_path = socket_file_path()?;
    wait_for_service(&socket_path).await?;

    local_ingress::submit_event(
        &socket_path,
        &LocalSignalEvent {
            source_id: "agents_notifier".to_string(),
            title: "Agents Notifier".to_string(),
            body: "Test notification from your Mac. If this arrived, Agents Notifier is working."
                .to_string(),
        },
    )
    .await
    .context("failed to send test notification through the local service")
}

async fn wait_for_service(socket_path: &Path) -> anyhow::Result<()> {
    let started = Instant::now();
    loop {
        if socket_path.exists() {
            return Ok(());
        }

        if started.elapsed() >= SERVICE_READY_TIMEOUT {
            anyhow::bail!(
                "agents-notifier service did not become ready at `{}`; check the log file",
                socket_path.display()
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
    let manager = LaunchAgentManager::system()?;
    let plist_path = launch_agent_plist_path()?;
    match manager.stop(&plist_path)? {
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

    remove_socket_file_if_exists()?;
    match stop_with_manager(&pid_file_path()?, &SystemProcessManager)? {
        StopOutcome::MissingPidFile => {}
        StopOutcome::RemovedStalePidFile { pid } => {
            println!("removed stale pid file for process {pid}.");
        }
        StopOutcome::Stopped { pid } => {
            println!("stopped legacy background process {pid}.");
        }
    }

    Ok(())
}

fn run_uninstall() -> anyhow::Result<()> {
    run_stop()?;

    remove_path_if_exists(&launch_agent_plist_path()?)?;
    remove_path_if_exists(&app_support_dir_path()?)?;
    if let Some(log_dir) = log_file_path()?.parent() {
        remove_path_if_exists(log_dir)?;
    }
    if let Some(config_dir) = default_config_file_path()?.parent() {
        remove_path_if_exists(config_dir)?;
    }

    let current_binary = std::env::current_exe().context("failed to locate current binary")?;
    if should_remove_current_binary(&current_binary) {
        remove_path_if_exists(&current_binary)?;
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

fn should_remove_current_binary(path: &Path) -> bool {
    if path.file_name().and_then(|name| name.to_str()) != Some("agents-notifier") {
        return false;
    }

    !path
        .components()
        .any(|component| component.as_os_str() == "target")
}

fn remove_socket_file_if_exists() -> anyhow::Result<()> {
    let socket_file = socket_file_path()?;
    if socket_file.exists() {
        fs::remove_file(&socket_file)
            .with_context(|| format!("failed to remove socket file `{}`", socket_file.display()))?;
    }

    Ok(())
}

async fn run_watch(config: &Config) -> anyhow::Result<()> {
    let providers = build_providers(config).context("provider setup failed")?;
    let provider_refs: Vec<&dyn Provider> = providers
        .iter()
        .map(|provider| provider.as_ref() as &dyn Provider)
        .collect();
    let socket_path = socket_file_path()?;
    let codex_desktop_source = codex_desktop_source(config);
    let needs_local_open_bridge = config
        .providers
        .iter()
        .any(|provider| provider.provider_type == ProviderType::FeishuLark);

    if let (Some(source), true) = (codex_desktop_source, needs_local_open_bridge) {
        tokio::select! {
            result = codex_desktop::watch(config, source, &provider_refs) => result,
            result = local_ingress::serve(config, &provider_refs, &socket_path) => result,
            result = local_open_bridge::serve() => result,
        }
    } else if let Some(source) = codex_desktop_source {
        tokio::select! {
            result = codex_desktop::watch(config, source, &provider_refs) => result,
            result = local_ingress::serve(config, &provider_refs, &socket_path) => result,
        }
    } else if needs_local_open_bridge {
        tokio::select! {
            result = local_ingress::serve(config, &provider_refs, &socket_path) => result,
            result = local_open_bridge::serve() => result,
        }
    } else {
        local_ingress::serve(config, &provider_refs, &socket_path).await
    }
}

fn codex_desktop_source(config: &Config) -> Option<&SourceConfig> {
    config
        .sources
        .iter()
        .find(|source| source.source_type == SourceType::CodexDesktop)
}

async fn run_emit(source_id: &str, title: String, body: String) -> anyhow::Result<()> {
    let socket_path = socket_file_path()?;
    let event = LocalSignalEvent {
        source_id: source_id.to_string(),
        title,
        body,
    };
    local_ingress::submit_event(&socket_path, &event).await
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
    fn safe_url_host_does_not_expose_webhook_token() {
        assert_eq!(
            safe_url_host("https://open.larksuite.com/open-apis/bot/v2/hook/secret-token"),
            "open.larksuite.com"
        );
    }
}
