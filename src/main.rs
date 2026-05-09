use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::{Parser, Subcommand};
use tokio::time::sleep;

use agents_notifier::config::{Config, ConfigError};
use agents_notifier::local_ingress::{self, LocalSignalEvent};
use agents_notifier::paths::{
    default_config_file_path, launch_agent_plist_path, log_file_path, pid_file_path,
    service_metadata_path, socket_file_path,
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

#[derive(Debug, Parser)]
#[command(name = "agents-notifier")]
#[command(about = "Local-first notification routing for coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Set up and start the local notification service")]
    Start {
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
    #[command(about = "Show the local notification service status")]
    Status,
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
    first_start: Option<FirstStartSetup>,
}

enum FirstStartSetup {
    Ntfy { topic: String },
    FeishuLark,
}

enum InitialProvider {
    Ntfy,
    FeishuLark,
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
            run_start_service(&config_path, loaded).await
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
        Command::Status => run_status(),
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
            first_start: None,
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
    println!("First setup connects one notification provider.");
    println!();

    match prompt_for_initial_provider()? {
        InitialProvider::Ntfy => run_first_start_ntfy_setup(path),
        InitialProvider::FeishuLark => run_first_start_feishu_lark_setup(path),
    }
}

fn run_first_start_ntfy_setup(path: &Path) -> anyhow::Result<LoadedConfig> {
    let generated_topic = setup::generated_ntfy_topic();

    println!();
    println!("ntfy sends notifications to your phone through a topic subscription.");
    println!("Install the ntfy app on your phone, then subscribe to the topic below.");
    println!();

    let topic = prompt_for_ntfy_topic(&generated_topic)?;
    let config = setup::build_ntfy_config(&topic);
    setup::write_config(path, &config)?;

    println!();
    println!("Created config: {}", path.display());
    println!("ntfy server: https://ntfy.sh");
    println!("ntfy topic: {topic}");
    println!("Starting agents-notifier now.");

    Ok(LoadedConfig {
        config,
        first_start: Some(FirstStartSetup::Ntfy { topic }),
    })
}

fn run_first_start_feishu_lark_setup(path: &Path) -> anyhow::Result<LoadedConfig> {
    println!();
    println!("Feishu/Lark sends notifications to a group through a custom bot webhook.");
    println!("Add a custom bot to the target group, then paste its webhook URL.");
    println!(
        "If you enable security, use Signature Verification. Keyword security can block notifications unless every message contains your keyword."
    );
    println!();

    let webhook_url = prompt_for_feishu_lark_webhook_url()?;
    let secret = prompt_for_feishu_lark_secret()?;
    let config = setup::build_feishu_lark_config(&webhook_url, secret);
    setup::write_config(path, &config)?;

    println!();
    println!("Created config: {}", path.display());
    println!("Feishu/Lark custom bot: configured");
    println!("Starting agents-notifier now.");

    Ok(LoadedConfig {
        config,
        first_start: Some(FirstStartSetup::FeishuLark),
    })
}

fn prompt_for_initial_provider() -> anyhow::Result<InitialProvider> {
    loop {
        println!("Where should Agents Notifier send notifications?");
        println!("1. ntfy");
        println!("2. Feishu/Lark custom bot");
        print!("Choose a provider [1/2]: ");
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read provider choice")?;

        match input.trim() {
            "" | "1" => return Ok(InitialProvider::Ntfy),
            "2" => return Ok(InitialProvider::FeishuLark),
            _ => println!("Please enter 1 or 2."),
        }
        println!();
    }
}

fn prompt_for_ntfy_topic(generated_topic: &str) -> anyhow::Result<String> {
    loop {
        print!("Press Enter to use topic `{generated_topic}`, or type a different topic: ");
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read topic")?;

        match setup::resolve_ntfy_topic(&input, generated_topic) {
            Ok(topic) => return Ok(topic),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_feishu_lark_webhook_url() -> anyhow::Result<String> {
    loop {
        print!("Webhook URL: ");
        io::stdout().flush().context("failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("failed to read webhook URL")?;

        match setup::resolve_feishu_lark_webhook_url(&input) {
            Ok(url) => return Ok(url),
            Err(error) => {
                println!("{error}");
            }
        }
    }
}

fn prompt_for_feishu_lark_secret() -> anyhow::Result<Option<String>> {
    print!("Signing secret, or press Enter if you did not enable Signature Verification: ");
    io::stdout().flush().context("failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("failed to read signing secret")?;

    Ok(setup::resolve_feishu_lark_secret(&input))
}

fn resolve_config_path(config: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match config {
        Some(path) => Ok(path),
        None => default_config_file_path(),
    }
}

async fn run_start_service(config_path: &Path, loaded: LoadedConfig) -> anyhow::Result<()> {
    build_providers(&loaded.config).context("provider setup failed")?;
    cleanup_legacy_background_service()?;

    let manager = LaunchAgentManager::system()?;
    let definition = build_service_definition(config_path)?;
    let plist_path = launch_agent_plist_path()?;
    let metadata_path = service_metadata_path()?;
    let outcome = manager.install_or_update(&definition, &plist_path, &metadata_path)?;
    wait_for_service(&socket_file_path()?).await?;

    let status = manager.status(&plist_path)?;
    print_service_start_outcome(outcome, status.pid, &definition.log_file);

    if let Some(setup) = loaded.first_start {
        finish_first_start_setup(setup).await?;
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
    let subscriptions = setup::ntfy_subscriptions(config);
    let feishu_lark_targets = setup::feishu_lark_targets(config);
    if !subscriptions.is_empty() || !feishu_lark_targets.is_empty() {
        println!();
    }

    if !subscriptions.is_empty() {
        println!("Phone subscription:");
        for subscription in subscriptions {
            println!("server: {}", subscription.server);
            println!("topic: {}", subscription.topic);
        }
    }

    if !feishu_lark_targets.is_empty() {
        println!("Feishu/Lark custom bot:");
        for target in feishu_lark_targets {
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

async fn finish_first_start_setup(setup: FirstStartSetup) -> anyhow::Result<()> {
    let socket_path = socket_file_path()?;
    wait_for_service(&socket_path).await?;

    match setup {
        FirstStartSetup::Ntfy { topic } => {
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
        }
        FirstStartSetup::FeishuLark => {
            println!();
            println!("Next: check your Feishu/Lark group.");
            wait_for_enter("Press Enter to send a test notification to the custom bot.")?;
        }
    }

    send_test_notification().await?;

    println!("Test notification sent.");
    if prompt_yes_no("Did it arrive? [Y/n] ")? {
        println!("Setup complete. Codex Desktop notifications can now be forwarded.");
    } else {
        println!("The service is still running, but the test notification did not arrive.");
        println!("Check the provider settings in your config, then run this test again:");
        println!(
            "agents-notifier emit --source codex_cli --title \"Agents Notifier\" --body \"Test notification from your Mac.\""
        );
    }

    Ok(())
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
        println!("Agents Notifier is working. Codex Desktop notifications can now be forwarded.");
    } else {
        println!("The service is running, but the test notification did not arrive.");
        println!("Check the provider settings printed above.");
    }

    Ok(())
}

fn can_send_test_notification(config: &Config) -> bool {
    config.source("codex_cli").is_some()
        && config.routes.iter().any(|route| {
            route.sources.iter().any(|source| source == "codex_cli") && !route.providers.is_empty()
        })
}

async fn send_test_notification() -> anyhow::Result<()> {
    let socket_path = socket_file_path()?;
    wait_for_service(&socket_path).await?;

    local_ingress::submit_event(
        &socket_path,
        &LocalSignalEvent {
            source_id: "codex_cli".to_string(),
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

fn remove_socket_file_if_exists() -> anyhow::Result<()> {
    let socket_file = socket_file_path()?;
    if socket_file.exists() {
        fs::remove_file(&socket_file)
            .with_context(|| format!("failed to remove socket file `{}`", socket_file.display()))?;
    }

    Ok(())
}

async fn run_watch(config: &Config) -> anyhow::Result<()> {
    let source = config
        .sources
        .iter()
        .find(|source| source.source_type == agents_notifier::config::SourceType::CodexDesktop)
        .context("no `codex_desktop` source is configured")?;
    let providers = build_providers(config).context("provider setup failed")?;
    let provider_refs: Vec<&dyn Provider> = providers
        .iter()
        .map(|provider| provider.as_ref() as &dyn Provider)
        .collect();
    let socket_path = socket_file_path()?;

    tokio::select! {
        result = codex_desktop::watch(config, source, &provider_refs) => result,
        result = local_ingress::serve(config, &provider_refs, &socket_path) => result,
    }
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
