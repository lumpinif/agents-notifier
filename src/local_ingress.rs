use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

use crate::config::Config;
use crate::router::{Provider, Router};
use crate::sources::codex_cli;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSignalEvent {
    pub source_id: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LocalIngressResponse {
    ok: bool,
    error: Option<String>,
}

pub async fn submit_event(socket_path: &Path, event: &LocalSignalEvent) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(socket_path).await.with_context(|| {
        format!(
            "agents-notifier service is not running at `{}`; run `agents-notifier start` first",
            socket_path.display()
        )
    })?;

    let request = serde_json::to_vec(event).context("failed to serialize local ingress event")?;
    stream
        .write_all(&request)
        .await
        .context("failed to submit local ingress event")?;
    stream
        .shutdown()
        .await
        .context("failed to finish local ingress request")?;

    let mut raw_response = Vec::new();
    stream
        .read_to_end(&mut raw_response)
        .await
        .context("failed to read local ingress response")?;
    let response: LocalIngressResponse =
        serde_json::from_slice(&raw_response).context("failed to parse local ingress response")?;

    if response.ok {
        Ok(())
    } else {
        anyhow::bail!(
            "agents-notifier service rejected local ingress event: {}",
            response
                .error
                .unwrap_or_else(|| "unknown error".to_string())
        )
    }
}

pub async fn serve(
    config: &Config,
    providers: &[&dyn Provider],
    socket_path: &Path,
) -> anyhow::Result<()> {
    prepare_socket_path(socket_path).await?;
    let listener = UnixListener::bind(socket_path).with_context(|| {
        format!(
            "failed to bind local ingress socket `{}`",
            socket_path.display()
        )
    })?;
    let _guard = SocketFileGuard::new(socket_path.to_path_buf());

    info!(
        socket.path = %socket_path.display(),
        event = "ingress.started",
    );

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("failed to accept local ingress connection")?;
        if let Err(error) = handle_connection(stream, config, providers).await {
            warn!(
                error = %error,
                event = "ingress.client.failed",
            );
        }
    }
}

pub async fn route_event(
    config: &Config,
    providers: &[&dyn Provider],
    event: LocalSignalEvent,
) -> anyhow::Result<()> {
    info!(
        source.id = %event.source_id,
        event = "ingress.received",
    );

    let signal = codex_cli::create_signal(config, &event.source_id, event.title, event.body)?;
    info!(
        signal.id = %signal.id,
        source.id = %signal.source_id,
        source.type = %signal.source_type,
        event = "signal.created",
    );

    Router::new(config).route(&signal, providers).await?;
    Ok(())
}

async fn handle_connection(
    mut stream: UnixStream,
    config: &Config,
    providers: &[&dyn Provider],
) -> anyhow::Result<()> {
    let mut raw_request = Vec::new();
    stream
        .read_to_end(&mut raw_request)
        .await
        .context("failed to read local ingress request")?;

    let result = async {
        let event: LocalSignalEvent = serde_json::from_slice(&raw_request)
            .context("failed to parse local ingress event JSON")?;
        route_event(config, providers, event).await
    }
    .await;

    let response = match &result {
        Ok(()) => LocalIngressResponse {
            ok: true,
            error: None,
        },
        Err(error) => LocalIngressResponse {
            ok: false,
            error: Some(error.to_string()),
        },
    };
    let raw_response =
        serde_json::to_vec(&response).context("failed to serialize local ingress response")?;
    stream
        .write_all(&raw_response)
        .await
        .context("failed to write local ingress response")?;

    result
}

async fn prepare_socket_path(socket_path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create local ingress socket directory `{}`",
                parent.display()
            )
        })?;
    }

    if socket_path.exists() {
        if UnixStream::connect(socket_path).await.is_ok() {
            anyhow::bail!(
                "local ingress socket `{}` is already accepting connections; agents-notifier may already be running",
                socket_path.display()
            );
        }
        fs::remove_file(socket_path).with_context(|| {
            format!(
                "failed to remove stale local ingress socket `{}`",
                socket_path.display()
            )
        })?;
    }

    Ok(())
}

struct SocketFileGuard {
    path: PathBuf,
}

impl SocketFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for SocketFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::config::{
        Config, LogConfig, ProviderConfig, ProviderType, RouteConfig, SourceConfig, SourceType,
    };
    use crate::router::{Provider, ProviderFuture};
    use crate::signal::Signal;

    use super::*;

    struct TestProvider {
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl Provider for TestProvider {
        fn id(&self) -> &str {
            "debug"
        }

        fn provider_type(&self) -> &str {
            "test"
        }

        fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
            Box::pin(async move {
                self.calls
                    .lock()
                    .expect("calls lock should not be poisoned")
                    .push(format!("{}:{}", signal.source_id, signal.body));
                Ok(())
            })
        }
    }

    #[tokio::test]
    async fn route_event_creates_signal_and_uses_service_router() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let provider = TestProvider {
            calls: Arc::clone(&calls),
        };

        route_event(
            &test_config(),
            &[&provider],
            LocalSignalEvent {
                source_id: "codex_cli".to_string(),
                title: "Codex".to_string(),
                body: "Codex finished a job.".to_string(),
            },
        )
        .await
        .expect("event should route through service router");

        assert_eq!(
            *calls.lock().expect("calls lock should not be poisoned"),
            vec!["codex_cli:Codex finished a job.".to_string()]
        );
    }

    fn test_config() -> Config {
        Config {
            schema_version: 1,
            log: LogConfig::default(),
            sources: vec![SourceConfig {
                id: "codex_cli".to_string(),
                source_type: SourceType::CodexCli,
            }],
            providers: vec![ProviderConfig {
                id: "debug".to_string(),
                provider_type: ProviderType::Webhook,
                server: None,
                topic: None,
                url: Some("https://example.com/hook".to_string()),
                url_env: None,
                secret: None,
                secret_env: None,
            }],
            routes: vec![RouteConfig {
                sources: vec!["codex_cli".to_string()],
                providers: vec!["debug".to_string()],
            }],
        }
    }
}
