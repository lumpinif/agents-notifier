use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};
use std::time::{Duration, Instant};

use anyhow::Context;
use clap::{Parser, Subcommand};
use tokio::time::sleep;

use agents_notifier::config::{Config, ConfigError};
use agents_notifier::local_ingress::{self, LocalSignalEvent};
use agents_notifier::paths::{
    default_config_file_path, log_file_path, pid_file_path, socket_file_path,
};
use agents_notifier::process::{
    PidFileState, StopOutcome, SystemProcessManager, inspect_pid_file, stop_with_manager,
};
use agents_notifier::providers::build_providers;
use agents_notifier::router::Provider;
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
    #[command(about = "Start the local notification service in the background")]
    Start {
        #[arg(long, value_name = "PATH", help = "Path to the config file")]
        config: Option<PathBuf>,
    },
    #[command(about = "Run the local notification service in the foreground")]
    Watch {
        #[arg(long, value_name = "PATH", help = "Path to the config file")]
        config: Option<PathBuf>,
        #[arg(long, help = "Start the service in the background")]
        background: bool,
    },
    #[command(about = "Stop the background notification service")]
    Stop,
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

struct FirstStartSetup {
    ntfy_topic: String,
}

struct BackgroundService {
    pid: u32,
    log_file: PathBuf,
}

enum BackgroundStartOutcome {
    Started(BackgroundService),
    AlreadyRunning(BackgroundService),
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
            run_background_service(&config_path, loaded).await
        }
        Command::Watch {
            config: config_path,
            background,
        } => {
            let allow_setup = config_path.is_none() && background;
            let config_path = resolve_config_path(config_path)?;
            let loaded = load_config(&config_path, allow_setup)?;
            if background {
                run_background_service(&config_path, loaded).await
            } else {
                init_tracing(&loaded.config.log.level)?;
                run_watch(&loaded.config).await
            }
        }
        Command::Stop => run_stop(),
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
    let generated_topic = setup::generated_ntfy_topic();

    println!("No agents-notifier config found at `{}`.", path.display());
    println!();
    println!("First setup will use ntfy, the simplest provider for Phase 1.");
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
        first_start: Some(FirstStartSetup { ntfy_topic: topic }),
    })
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

fn resolve_config_path(config: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match config {
        Some(path) => Ok(path),
        None => default_config_file_path(),
    }
}

async fn run_background_service(config_path: &Path, loaded: LoadedConfig) -> anyhow::Result<()> {
    build_providers(&loaded.config).context("provider setup failed")?;

    match start_background_service(config_path)? {
        BackgroundStartOutcome::Started(service) => {
            wait_for_service(&socket_file_path()?).await?;
            print_started_service(&service);
            if let Some(setup) = loaded.first_start {
                finish_first_start_setup(setup).await?;
            } else {
                print_ntfy_subscriptions(&loaded.config);
                offer_test_notification(&loaded.config).await?;
            }
        }
        BackgroundStartOutcome::AlreadyRunning(service) => {
            print_already_running_service(&service);
            print_ntfy_subscriptions(&loaded.config);
            offer_test_notification(&loaded.config).await?;
        }
    }

    Ok(())
}

fn start_background_service(config_path: &Path) -> anyhow::Result<BackgroundStartOutcome> {
    let pid_file = pid_file_path()?;
    let log_file = log_file_path()?;
    match inspect_pid_file(&pid_file, &SystemProcessManager)? {
        PidFileState::Missing => {}
        PidFileState::Stale { pid } => {
            remove_stale_runtime_files(&pid_file, &socket_file_path()?).with_context(|| {
                format!("failed to clean stale service files for exited process `{pid}`")
            })?;
        }
        PidFileState::AgentsNotifier { pid } => {
            return Ok(BackgroundStartOutcome::AlreadyRunning(BackgroundService {
                pid,
                log_file,
            }));
        }
        PidFileState::OtherProcess { pid } => {
            anyhow::bail!(
                "pid file already exists at `{}`, but pid `{pid}` is not an agents-notifier process; the pid file may be stale",
                pid_file.display()
            );
        }
    }

    if let Some(parent) = pid_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create pid directory `{}`", parent.display()))?;
    }
    if let Some(parent) = log_file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log directory `{}`", parent.display()))?;
    }

    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .with_context(|| format!("failed to open log file `{}`", log_file.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("failed to clone log file `{}`", log_file.display()))?;

    let child = StdCommand::new(std::env::current_exe().context("failed to locate current exe")?)
        .args(["watch", "--config"])
        .arg(config_path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .context("failed to start background service")?;

    fs::write(&pid_file, child.id().to_string())
        .with_context(|| format!("failed to write pid file `{}`", pid_file.display()))?;

    Ok(BackgroundStartOutcome::Started(BackgroundService {
        pid: child.id(),
        log_file,
    }))
}

fn print_started_service(service: &BackgroundService) {
    println!("agents-notifier service is running in the background.");
    println!("pid: {}", service.pid);
    println!("log: {}", service.log_file.display());
}

fn print_already_running_service(service: &BackgroundService) {
    println!("agents-notifier service is already running.");
    println!("pid: {}", service.pid);
    println!("log: {}", service.log_file.display());
}

fn print_ntfy_subscriptions(config: &Config) {
    let subscriptions = setup::ntfy_subscriptions(config);
    if subscriptions.is_empty() {
        return;
    }

    println!();
    println!("Phone subscription:");
    for subscription in subscriptions {
        println!("server: {}", subscription.server);
        println!("topic: {}", subscription.topic);
    }
}

async fn finish_first_start_setup(setup: FirstStartSetup) -> anyhow::Result<()> {
    let socket_path = socket_file_path()?;
    wait_for_service(&socket_path).await?;

    println!();
    println!("Next: connect your phone.");
    println!("1. Open the ntfy app.");
    println!("2. Add a subscription.");
    println!("3. Server: https://ntfy.sh");
    println!("4. Topic: {}", setup.ntfy_topic);
    println!();
    wait_for_enter("After your phone is subscribed, press Enter to send a test notification.")?;

    send_test_notification().await?;

    println!("Test notification sent.");
    if prompt_yes_no("Did it arrive on your phone? [Y/n] ")? {
        println!("Setup complete. Codex Desktop notifications can now be forwarded to your phone.");
    } else {
        println!("The service is still running, but the phone did not receive the test.");
        println!("Check that the ntfy app is subscribed to exactly this topic:");
        println!("{}", setup.ntfy_topic);
        println!("Then run this test again:");
        println!(
            "agents-notifier emit --source codex_cli --title \"Agents Notifier\" --body \"Test notification from your Mac.\""
        );
    }

    Ok(())
}

async fn offer_test_notification(config: &Config) -> anyhow::Result<()> {
    if !is_interactive_terminal()
        || setup::ntfy_subscriptions(config).is_empty()
        || config.source("codex_cli").is_none()
    {
        return Ok(());
    }

    if !prompt_yes_no("Send a test notification now? [Y/n] ")? {
        return Ok(());
    }

    send_test_notification().await?;
    println!("Test notification sent.");
    if prompt_yes_no("Did it arrive on your phone? [Y/n] ")? {
        println!(
            "ntfy is working. Codex Desktop notifications can now be forwarded to your phone."
        );
    } else {
        println!("The service is running, but the phone did not receive the test.");
        println!("Check that the ntfy app is subscribed to exactly the topic printed above.");
    }

    Ok(())
}

async fn send_test_notification() -> anyhow::Result<()> {
    let socket_path = socket_file_path()?;
    wait_for_service(&socket_path).await?;

    local_ingress::submit_event(
        &socket_path,
        &LocalSignalEvent {
            source_id: "codex_cli".to_string(),
            title: "Agents Notifier".to_string(),
            body:
                "Test notification from your Mac. If this arrived on your phone, ntfy is working."
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
    let pid_file = pid_file_path()?;
    match stop_with_manager(&pid_file, &SystemProcessManager)? {
        StopOutcome::MissingPidFile => {
            println!("agents-notifier is not running.");
        }
        StopOutcome::RemovedStalePidFile { pid } => {
            remove_socket_file_if_exists()?;
            println!("agents-notifier is not running.");
            println!("removed stale pid file for process {pid}.");
        }
        StopOutcome::Stopped { pid } => {
            remove_socket_file_if_exists()?;
            println!("agents-notifier stopped process {pid}.");
        }
    }
    Ok(())
}

fn remove_stale_runtime_files(pid_file: &Path, socket_file: &Path) -> anyhow::Result<()> {
    if pid_file.exists() {
        fs::remove_file(pid_file)
            .with_context(|| format!("failed to remove stale pid file `{}`", pid_file.display()))?;
    }

    if socket_file.exists() {
        fs::remove_file(socket_file).with_context(|| {
            format!(
                "failed to remove stale socket file `{}`",
                socket_file.display()
            )
        })?;
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
