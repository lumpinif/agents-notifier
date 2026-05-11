#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

use crate::config::{Config, SourceType};
use crate::paths::IngressEndpoint;
use crate::router::{Provider, Router};
use crate::signal::Signal;
use crate::sources::{agent_hook, agents_notifier, claude_code, codex_cli};

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

impl LocalIngressResponse {
    fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }
}

pub async fn submit_event(
    endpoint: &IngressEndpoint,
    event: &LocalSignalEvent,
) -> anyhow::Result<()> {
    match endpoint {
        #[cfg(unix)]
        IngressEndpoint::UnixSocket(socket_path) => {
            submit_event_to_unix_socket(socket_path, event).await
        }
        #[cfg(windows)]
        IngressEndpoint::WindowsNamedPipe(pipe_name) => {
            submit_event_to_windows_named_pipe(pipe_name, event).await
        }
    }
}

#[cfg(unix)]
async fn submit_event_to_unix_socket(
    socket_path: &Path,
    event: &LocalSignalEvent,
) -> anyhow::Result<()> {
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

#[cfg(windows)]
async fn submit_event_to_windows_named_pipe(
    pipe_name: &str,
    event: &LocalSignalEvent,
) -> anyhow::Result<()> {
    let mut stream = ClientOptions::new().open(pipe_name).with_context(|| {
        format!(
            "agents-notifier service is not running at `{pipe_name}`; run `agents-notifier start` first"
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
    endpoint: &IngressEndpoint,
) -> anyhow::Result<()> {
    match endpoint {
        #[cfg(unix)]
        IngressEndpoint::UnixSocket(socket_path) => {
            serve_unix_socket(config, providers, socket_path).await
        }
        #[cfg(windows)]
        IngressEndpoint::WindowsNamedPipe(pipe_name) => {
            serve_windows_named_pipe(config, providers, pipe_name).await
        }
    }
}

#[cfg(unix)]
async fn serve_unix_socket(
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
        if let Err(error) = handle_unix_connection(stream, config, providers).await {
            warn!(
                error = %error,
                event = "ingress.client.failed",
            );
        }
    }
}

#[cfg(windows)]
async fn serve_windows_named_pipe(
    config: &Config,
    providers: &[&dyn Provider],
    pipe_name: &str,
) -> anyhow::Result<()> {
    info!(
        pipe.name = %pipe_name,
        event = "ingress.started",
    );

    loop {
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(pipe_name)
            .with_context(|| format!("failed to create local ingress named pipe `{pipe_name}`"))?;
        server
            .connect()
            .await
            .with_context(|| format!("failed to accept local ingress named pipe `{pipe_name}`"))?;

        if let Err(error) = handle_windows_pipe(server, config, providers).await {
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

    let signal = create_signal(config, &event.source_id, event.title, event.body)?;
    info!(
        signal.id = %signal.id,
        source.id = %signal.source_id,
        source.type = %signal.source_type,
        event = "signal.created",
    );

    Router::new(config).route(&signal, providers).await?;
    Ok(())
}

fn create_signal(
    config: &Config,
    source_id: &str,
    title: String,
    body: String,
) -> anyhow::Result<Signal> {
    let source = config
        .source(source_id)
        .with_context(|| format!("source `{source_id}` is not configured"))?;

    match source.source_type {
        SourceType::AgentsNotifier => {
            agents_notifier::create_signal(config, source_id, title, body)
        }
        SourceType::AgentHook => agent_hook::create_signal(config, source_id, title, body),
        SourceType::ClaudeCode => claude_code::create_signal(config, source_id, title, body),
        SourceType::CodexCli => codex_cli::create_signal(config, source_id, title, body),
        SourceType::CodexDesktop => {
            anyhow::bail!(
                "source `{}` has type `codex_desktop`; local ingress only accepts `agents_notifier`, `agent_hook`, `codex_cli`, and `claude_code` events",
                source.id
            )
        }
    }
}

#[cfg(unix)]
async fn handle_unix_connection(
    mut stream: UnixStream,
    config: &Config,
    providers: &[&dyn Provider],
) -> anyhow::Result<()> {
    let mut raw_request = Vec::new();
    stream
        .read_to_end(&mut raw_request)
        .await
        .context("failed to read local ingress request")?;

    let raw_response = handle_request_bytes(config, providers, &raw_request).await?;

    stream
        .write_all(&raw_response)
        .await
        .context("failed to write local ingress response")?;

    Ok(())
}

#[cfg(windows)]
async fn handle_windows_pipe(
    mut pipe: tokio::net::windows::named_pipe::NamedPipeServer,
    config: &Config,
    providers: &[&dyn Provider],
) -> anyhow::Result<()> {
    let mut raw_request = Vec::new();
    pipe.read_to_end(&mut raw_request)
        .await
        .context("failed to read local ingress request")?;

    let raw_response = handle_request_bytes(config, providers, &raw_request).await?;
    pipe.write_all(&raw_response)
        .await
        .context("failed to write local ingress response")?;

    Ok(())
}

async fn handle_request_bytes(
    config: &Config,
    providers: &[&dyn Provider],
    raw_request: &[u8],
) -> anyhow::Result<Vec<u8>> {
    if is_ping_request(raw_request) {
        return serde_json::to_vec(&LocalIngressResponse::ok())
            .context("failed to serialize local ingress response");
    }

    let result = async {
        let event: LocalSignalEvent = serde_json::from_slice(raw_request)
            .context("failed to parse local ingress event JSON")?;
        route_event(config, providers, event).await
    }
    .await;

    let response = match &result {
        Ok(()) => LocalIngressResponse {
            ok: true,
            error: None,
        },
        Err(error) => {
            warn!(
                error = %error,
                event = "ingress.client.failed",
            );
            LocalIngressResponse {
                ok: false,
                error: Some(error.to_string()),
            }
        }
    };

    serde_json::to_vec(&response).context("failed to serialize local ingress response")
}

fn is_ping_request(raw_request: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(raw_request)
        .ok()
        .and_then(|value| {
            value
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .map(|kind| kind == "ping")
        })
        .unwrap_or(false)
}

#[cfg(unix)]
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

pub async fn is_ready(endpoint: &IngressEndpoint) -> bool {
    match endpoint {
        #[cfg(unix)]
        IngressEndpoint::UnixSocket(socket_path) => socket_path.exists(),
        #[cfg(windows)]
        IngressEndpoint::WindowsNamedPipe(pipe_name) => ping_windows_named_pipe(pipe_name).await,
    }
}

#[cfg(windows)]
async fn ping_windows_named_pipe(pipe_name: &str) -> bool {
    let Ok(mut stream) = ClientOptions::new().open(pipe_name) else {
        return false;
    };

    if stream.write_all(br#"{"kind":"ping"}"#).await.is_err() {
        return false;
    }
    if stream.shutdown().await.is_err() {
        return false;
    }

    let mut raw_response = Vec::new();
    if stream.read_to_end(&mut raw_response).await.is_err() {
        return false;
    }

    serde_json::from_slice::<LocalIngressResponse>(&raw_response)
        .map(|response| response.ok)
        .unwrap_or(false)
}

pub fn cleanup_endpoint(endpoint: &IngressEndpoint) -> anyhow::Result<()> {
    match endpoint {
        #[cfg(unix)]
        IngressEndpoint::UnixSocket(socket_file) => {
            if socket_file.exists() {
                fs::remove_file(socket_file).with_context(|| {
                    format!("failed to remove socket file `{}`", socket_file.display())
                })?;
            }
            Ok(())
        }
        #[cfg(windows)]
        IngressEndpoint::WindowsNamedPipe(_) => Ok(()),
    }
}

#[cfg(unix)]
struct SocketFileGuard {
    path: PathBuf,
}

#[cfg(unix)]
impl SocketFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[cfg(unix)]
impl Drop for SocketFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::config::{
        Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType, RouteConfig,
        SourceConfig, SourceType,
    };
    use crate::delivery::ProviderSendResult;
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
                Ok(ProviderSendResult::sent(
                    self.id(),
                    self.provider_type(),
                    signal,
                ))
            })
        }
    }

    #[tokio::test]
    async fn route_event_creates_codex_cli_signal_and_uses_service_router() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let provider = TestProvider {
            calls: Arc::clone(&calls),
        };

        route_event(
            &test_config("codex_cli", SourceType::CodexCli),
            &[&provider],
            LocalSignalEvent {
                source_id: "codex_cli".to_string(),
                title: "Codex".to_string(),
                body: "Ready for review.".to_string(),
            },
        )
        .await
        .expect("event should route through service router");

        assert_eq!(
            *calls.lock().expect("calls lock should not be poisoned"),
            vec!["codex_cli:Ready for review.".to_string()]
        );
    }

    #[tokio::test]
    async fn route_event_creates_agents_notifier_signal_for_service_tests() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let provider = TestProvider {
            calls: Arc::clone(&calls),
        };

        route_event(
            &test_config("agents_notifier", SourceType::AgentsNotifier),
            &[&provider],
            LocalSignalEvent {
                source_id: "agents_notifier".to_string(),
                title: "Agents Notifier".to_string(),
                body: "Test notification from your computer.".to_string(),
            },
        )
        .await
        .expect("service test event should route through service router");

        assert_eq!(
            *calls.lock().expect("calls lock should not be poisoned"),
            vec!["agents_notifier:Test notification from your computer.".to_string()]
        );
    }

    #[tokio::test]
    async fn route_event_creates_claude_code_signal_and_uses_service_router() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let provider = TestProvider {
            calls: Arc::clone(&calls),
        };

        route_event(
            &test_config("claude_code", SourceType::ClaudeCode),
            &[&provider],
            LocalSignalEvent {
                source_id: "claude_code".to_string(),
                title: "Claude Code".to_string(),
                body: "Claude Code finished a task.".to_string(),
            },
        )
        .await
        .expect("Claude Code event should route through service router");

        assert_eq!(
            *calls.lock().expect("calls lock should not be poisoned"),
            vec!["claude_code:Claude Code finished a task.".to_string()]
        );
    }

    #[tokio::test]
    async fn route_event_creates_generic_agent_hook_signal_and_uses_service_router() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let provider = TestProvider {
            calls: Arc::clone(&calls),
        };

        route_event(
            &test_config("opencode_cli", SourceType::AgentHook),
            &[&provider],
            LocalSignalEvent {
                source_id: "opencode_cli".to_string(),
                title: "OpenCode CLI".to_string(),
                body: "OpenCode CLI finished a task.".to_string(),
            },
        )
        .await
        .expect("generic agent hook event should route through service router");

        assert_eq!(
            *calls.lock().expect("calls lock should not be poisoned"),
            vec!["opencode_cli:OpenCode CLI finished a task.".to_string()]
        );
    }

    #[tokio::test]
    async fn route_event_rejects_codex_desktop_local_ingress() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let provider = TestProvider {
            calls: Arc::clone(&calls),
        };

        let err = route_event(
            &test_config("codex_desktop", SourceType::CodexDesktop),
            &[&provider],
            LocalSignalEvent {
                source_id: "codex_desktop".to_string(),
                title: "Codex Desktop".to_string(),
                body: "Body".to_string(),
            },
        )
        .await
        .expect_err("Codex Desktop should not accept local ingress events");

        assert!(err.to_string().contains("local ingress only accepts"));
        assert!(
            calls
                .lock()
                .expect("calls lock should not be poisoned")
                .is_empty()
        );
    }

    fn test_config(source_id: &str, source_type: SourceType) -> Config {
        Config {
            schema_version: 1,
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: source_id.to_string(),
                source_type,
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
            }],
            routes: vec![RouteConfig {
                sources: vec![source_id.to_string()],
                providers: vec!["debug".to_string()],
            }],
        }
    }
}
