use std::collections::BTreeMap;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

use crate::config::{AnswerDetail, Config, PromptDetail, SourceType};
use crate::paths::IngressEndpoint;
use crate::router::{Provider, Router};
use crate::runtime::RuntimeState;
use crate::signal::{
    Signal, SignalAnswer, SignalAnswerKind, SignalConversation, SignalEvent, SignalLifecycle,
    SignalLink, SignalWorkspace,
};
use crate::sources::{agent_hook, agents_notifier, claude_code, codex_cli};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSignalEvent {
    pub source_id: String,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "LocalSignalAction::is_route_signal")]
    pub action: LocalSignalAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<SignalEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<SignalWorkspace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<LocalSignalConversation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<SignalLifecycle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<SignalLink>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl LocalSignalEvent {
    pub fn new(
        source_id: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            title: title.into(),
            body: body.into(),
            action: LocalSignalAction::RouteSignal,
            event: None,
            workspace: None,
            conversation: None,
            lifecycle: None,
            links: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalSignalAction {
    #[default]
    RouteSignal,
    StoreSessionContext,
}

impl LocalSignalAction {
    fn is_route_signal(action: &Self) -> bool {
        *action == Self::RouteSignal
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSignalConversation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Default)]
pub struct LocalIngressState {
    session_context: Mutex<BTreeMap<SessionContextKey, LocalSessionContext>>,
}

impl LocalIngressState {
    fn record_session_model(
        &self,
        source_id: String,
        session_id: String,
        model: String,
    ) -> anyhow::Result<()> {
        let mut session_context = self
            .session_context
            .lock()
            .map_err(|_| anyhow::anyhow!("local ingress session context lock is poisoned"))?;
        session_context.insert(
            SessionContextKey {
                source_id,
                session_id,
            },
            LocalSessionContext { model },
        );
        Ok(())
    }

    fn session_model(&self, source_id: &str, session_id: &str) -> anyhow::Result<Option<String>> {
        let session_context = self
            .session_context
            .lock()
            .map_err(|_| anyhow::anyhow!("local ingress session context lock is poisoned"))?;
        Ok(session_context
            .get(&SessionContextKey {
                source_id: source_id.to_string(),
                session_id: session_id.to_string(),
            })
            .map(|context| context.model.clone()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SessionContextKey {
    source_id: String,
    session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalSessionContext {
    model: String,
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

pub async fn serve(runtime: RuntimeState, endpoint: IngressEndpoint) -> anyhow::Result<()> {
    match &endpoint {
        #[cfg(unix)]
        IngressEndpoint::UnixSocket(socket_path) => serve_unix_socket(runtime, socket_path).await,
        #[cfg(windows)]
        IngressEndpoint::WindowsNamedPipe(pipe_name) => {
            serve_windows_named_pipe(runtime, pipe_name).await
        }
    }
}

#[cfg(unix)]
async fn serve_unix_socket(runtime: RuntimeState, socket_path: &Path) -> anyhow::Result<()> {
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

    let state = LocalIngressState::default();
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("failed to accept local ingress connection")?;
        if let Err(error) = handle_unix_connection(stream, &runtime, &state).await {
            warn!(
                error = %error,
                event = "ingress.client.failed",
            );
        }
    }
}

#[cfg(windows)]
async fn serve_windows_named_pipe(runtime: RuntimeState, pipe_name: &str) -> anyhow::Result<()> {
    info!(
        pipe.name = %pipe_name,
        event = "ingress.started",
    );

    let state = LocalIngressState::default();
    loop {
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(pipe_name)
            .with_context(|| format!("failed to create local ingress named pipe `{pipe_name}`"))?;
        server
            .connect()
            .await
            .with_context(|| format!("failed to accept local ingress named pipe `{pipe_name}`"))?;

        if let Err(error) = handle_windows_pipe(server, &runtime, &state).await {
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
    let state = LocalIngressState::default();
    route_event_with_state(config, providers, &state, event).await
}

pub async fn route_event_with_state(
    config: &Config,
    providers: &[&dyn Provider],
    state: &LocalIngressState,
    event: LocalSignalEvent,
) -> anyhow::Result<()> {
    info!(
        source.id = %event.source_id,
        action = ?event.action,
        event = "ingress.received",
    );

    if event.action == LocalSignalAction::StoreSessionContext {
        store_session_context(config, state, event)?;
        return Ok(());
    }

    let mut signal = create_signal(config, event)?;
    apply_session_context(state, &mut signal)?;
    info!(
        signal.id = %signal.id,
        source.id = %signal.source_id(),
        source.type = %signal.source_type(),
        event = "signal.created",
    );

    Router::new(config).route(&signal, providers).await?;
    Ok(())
}

pub async fn route_event_with_runtime(
    runtime: &RuntimeState,
    state: &LocalIngressState,
    event: LocalSignalEvent,
) -> anyhow::Result<()> {
    let snapshot = runtime.current()?;
    let providers = snapshot.provider_refs();
    route_event_with_state(&snapshot.config, &providers, state, event).await
}

fn store_session_context(
    config: &Config,
    state: &LocalIngressState,
    event: LocalSignalEvent,
) -> anyhow::Result<()> {
    let source_id = event.source_id;
    validate_local_ingress_source(config, &source_id)?;
    let conversation = event
        .conversation
        .context("local ingress session context event missing `conversation`")?;
    let session_id = conversation
        .session_id
        .and_then(present_owned)
        .context("local ingress session context event missing `conversation.session_id`")?;
    let model = conversation
        .model
        .and_then(present_owned)
        .context("local ingress session context event missing `conversation.model`")?;

    state.record_session_model(source_id.clone(), session_id.clone(), model.clone())?;
    info!(
        source.id = %source_id,
        conversation.session_id = %session_id,
        conversation.model = %model,
        event = "ingress.session_context.stored",
    );
    Ok(())
}

fn create_signal(config: &Config, event: LocalSignalEvent) -> anyhow::Result<Signal> {
    let source_id = event.source_id.clone();
    let title = event.title.clone();
    let body = event.body.clone();
    let source = validate_local_ingress_source(config, &source_id)?;

    let mut signal = match source.source_type {
        SourceType::AgentsNotifier => {
            agents_notifier::create_signal(config, &source_id, title, body)
        }
        SourceType::AgentHook => agent_hook::create_signal(config, &source_id, title, body),
        SourceType::ClaudeCode => claude_code::create_signal(config, &source_id, title, body),
        SourceType::CodexCli => codex_cli::create_signal(config, &source_id, title, body),
        SourceType::CodexDesktop => {
            unreachable!("codex_desktop is rejected before signal creation")
        }
    }?;

    apply_local_structured_fields(config, &mut signal, event);
    Ok(signal)
}

fn validate_local_ingress_source<'a>(
    config: &'a Config,
    source_id: &str,
) -> anyhow::Result<&'a crate::config::SourceConfig> {
    let source = config
        .source(source_id)
        .with_context(|| format!("source `{source_id}` is not configured"))?;

    if source.source_type == SourceType::CodexDesktop {
        anyhow::bail!(
            "source `{}` has type `codex_desktop`; local ingress only accepts `agents_notifier`, `agent_hook`, `codex_cli`, and `claude_code` events",
            source.id
        );
    }

    Ok(source)
}

fn apply_local_structured_fields(config: &Config, signal: &mut Signal, event: LocalSignalEvent) {
    if let Some(event) = event.event {
        signal.event = event;
    }
    signal.workspace = event.workspace;
    signal.lifecycle = event.lifecycle;
    signal.links = event.links;
    signal.metadata.extend(event.metadata);
    if let Some(conversation) = event.conversation {
        signal.conversation = Some(SignalConversation {
            session_id: conversation.session_id,
            session_title: conversation.session_title,
            turn_id: conversation.turn_id,
            prompt: prompt_for_config(conversation.prompt, config.notification.prompt_detail),
            answer: answer_for_config(conversation.answer, config.notification.answer_detail),
            model: conversation.model,
        });
    }
}

fn apply_session_context(state: &LocalIngressState, signal: &mut Signal) -> anyhow::Result<()> {
    let source_id = signal.source_id().to_string();
    let Some(conversation) = signal.conversation.as_mut() else {
        return Ok(());
    };
    if conversation.model.is_some() {
        return Ok(());
    }
    let Some(session_id) = conversation.session_id.as_deref() else {
        return Ok(());
    };
    if let Some(model) = state.session_model(&source_id, session_id)? {
        conversation.model = Some(model);
    }
    Ok(())
}

fn prompt_for_config(prompt: Option<String>, prompt_detail: PromptDetail) -> Option<String> {
    if prompt_detail != PromptDetail::On {
        return None;
    }

    prompt.and_then(present_owned)
}

fn answer_for_config(answer: Option<String>, answer_detail: AnswerDetail) -> Option<SignalAnswer> {
    let answer = answer.and_then(present_owned)?;
    let (kind, content) = match answer_detail {
        AnswerDetail::Preview => (SignalAnswerKind::Preview, preview_text(&answer)),
        AnswerDetail::Full => (SignalAnswerKind::Full, answer),
    };

    if content.is_empty() {
        None
    } else {
        Some(SignalAnswer { kind, content })
    }
}

fn preview_text(value: &str) -> String {
    const LIMIT: usize = 360;

    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated: String = chars.by_ref().take(LIMIT).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn present_owned(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(unix)]
async fn handle_unix_connection(
    mut stream: UnixStream,
    runtime: &RuntimeState,
    state: &LocalIngressState,
) -> anyhow::Result<()> {
    let mut raw_request = Vec::new();
    stream
        .read_to_end(&mut raw_request)
        .await
        .context("failed to read local ingress request")?;

    let raw_response = handle_request_bytes(runtime, state, &raw_request).await?;

    stream
        .write_all(&raw_response)
        .await
        .context("failed to write local ingress response")?;

    Ok(())
}

#[cfg(windows)]
async fn handle_windows_pipe(
    mut pipe: tokio::net::windows::named_pipe::NamedPipeServer,
    runtime: &RuntimeState,
    state: &LocalIngressState,
) -> anyhow::Result<()> {
    let mut raw_request = Vec::new();
    pipe.read_to_end(&mut raw_request)
        .await
        .context("failed to read local ingress request")?;

    let raw_response = handle_request_bytes(runtime, state, &raw_request).await?;
    pipe.write_all(&raw_response)
        .await
        .context("failed to write local ingress response")?;

    Ok(())
}

async fn handle_request_bytes(
    runtime: &RuntimeState,
    state: &LocalIngressState,
    raw_request: &[u8],
) -> anyhow::Result<Vec<u8>> {
    if is_ping_request(raw_request) {
        return serde_json::to_vec(&LocalIngressResponse::ok())
            .context("failed to serialize local ingress response");
    }

    let result = async {
        let event: LocalSignalEvent = serde_json::from_slice(raw_request)
            .context("failed to parse local ingress event JSON")?;
        route_event_with_runtime(runtime, state, event).await
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
mod tests;
