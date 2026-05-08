use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};

use agents_notifier::config::Config;
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
        config: PathBuf,
    },
    Emit {
        #[arg(long)]
        config: PathBuf,
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
        Command::Watch { config } => {
            let config = Config::from_path(&config).context("config load failed")?;
            init_tracing(&config.log.level)?;
            run_watch(&config).await
        }
        Command::Emit {
            config,
            source,
            title,
            body,
        } => {
            let config = Config::from_path(&config).context("config load failed")?;
            init_tracing(&config.log.level)?;
            run_emit(&config, &source, title, body).await
        }
    }
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
