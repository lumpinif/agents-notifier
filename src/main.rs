use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Stdio};

use anyhow::Context;
use clap::{Parser, Subcommand};

use agents_notifier::config::Config;
use agents_notifier::paths::{default_config_file_path, log_file_path, pid_file_path};
use agents_notifier::process::{StopOutcome, SystemProcessManager, stop_with_manager};
use agents_notifier::providers::build_providers;
use agents_notifier::router::{Provider, Router};
use agents_notifier::sources::{codex_cli, codex_desktop};

#[derive(Debug, Parser)]
#[command(name = "agents-notifier")]
#[command(about = "Local-first notification routing for coding agents")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Watch {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        background: bool,
    },
    Stop,
    Emit {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        source: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        body: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Watch {
            config: config_path,
            background,
        } => {
            let config_path = resolve_config_path(config_path)?;
            let config = Config::from_path(&config_path).context("config load failed")?;
            if background {
                start_background_watch(&config_path)?;
                Ok(())
            } else {
                init_tracing(&config.log.level)?;
                run_watch(&config).await
            }
        }
        Command::Stop => run_stop(),
        Command::Emit {
            config,
            source,
            title,
            body,
        } => {
            let config_path = resolve_config_path(config)?;
            let config = Config::from_path(&config_path).context("config load failed")?;
            init_tracing(&config.log.level)?;
            run_emit(&config, &source, title, body).await
        }
    }
}

fn resolve_config_path(config: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match config {
        Some(path) => Ok(path),
        None => default_config_file_path(),
    }
}

fn start_background_watch(config_path: &Path) -> anyhow::Result<()> {
    let pid_file = pid_file_path()?;
    if pid_file.exists() {
        anyhow::bail!(
            "pid file already exists at `{}`; run `agents-notifier stop` first",
            pid_file.display()
        );
    }

    let log_file = log_file_path()?;
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
        .context("failed to start background watcher")?;

    fs::write(&pid_file, child.id().to_string())
        .with_context(|| format!("failed to write pid file `{}`", pid_file.display()))?;

    println!("agents-notifier started in the background.");
    println!("pid: {}", child.id());
    println!("log: {}", log_file.display());
    Ok(())
}

fn run_stop() -> anyhow::Result<()> {
    init_tracing("info")?;
    let pid_file = pid_file_path()?;
    match stop_with_manager(&pid_file, &SystemProcessManager)? {
        StopOutcome::MissingPidFile => {
            println!("agents-notifier is not running.");
        }
        StopOutcome::Stopped { pid } => {
            println!("agents-notifier stopped process {pid}.");
        }
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

    codex_desktop::watch(config, source, &provider_refs).await
}

async fn run_emit(
    config: &Config,
    source_id: &str,
    title: String,
    body: String,
) -> anyhow::Result<()> {
    let signal = codex_cli::create_signal(config, source_id, title, body)?;
    let providers = build_providers(config).context("provider setup failed")?;
    let provider_refs: Vec<&dyn Provider> = providers
        .iter()
        .map(|provider| provider.as_ref() as &dyn Provider)
        .collect();

    Router::new(config).route(&signal, &provider_refs).await?;
    Ok(())
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
