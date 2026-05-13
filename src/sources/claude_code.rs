use std::collections::BTreeMap;

use anyhow::Context;
use serde::Deserialize;

use crate::config::{Config, SourceType};
use crate::local_ingress::{LocalSignalAction, LocalSignalConversation, LocalSignalEvent};
use crate::signal::{
    Signal, SignalEvent, SignalEventKind, SignalLifecycle, SignalLifecycleStatus, SignalWorkspace,
};
use crate::sources::agent_hook;
use crate::sources::hook_payload::{present, project_name_from_cwd};

const DEFAULT_TITLE: &str = "Claude Code";
const STOP_BODY: &str = "Claude Code finished a task.";

pub fn create_signal(
    config: &Config,
    source_id: &str,
    title: impl Into<String>,
    body: impl Into<String>,
) -> anyhow::Result<Signal> {
    agent_hook::create_signal_for_type(config, source_id, SourceType::ClaudeCode, title, body)
}

#[derive(Debug, Deserialize)]
pub struct ClaudeCodeHookInput {
    session_id: String,
    transcript_path: String,
    cwd: String,
    hook_event_name: String,
    #[serde(default)]
    permission_mode: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    last_assistant_message: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    stop_hook_active: Option<bool>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    notification_type: Option<String>,
}

pub fn local_event_from_hook(
    source_id: impl Into<String>,
    input: ClaudeCodeHookInput,
) -> anyhow::Result<LocalSignalEvent> {
    let hook_event_name =
        present(&input.hook_event_name).context("claude_code hook missing `hook_event_name`")?;

    match hook_event_name.as_str() {
        "SessionStart" => local_event_from_session_start_hook(source_id, input, hook_event_name),
        "Stop" => local_event_from_stop_hook(source_id, input, hook_event_name),
        "Notification" => local_event_from_notification_hook(source_id, input, hook_event_name),
        event_name => {
            anyhow::bail!(
                "claude_code ingest expected `hook_event_name` to be `SessionStart`, `Stop`, or `Notification`, got `{event_name}`"
            )
        }
    }
}

fn local_event_from_session_start_hook(
    source_id: impl Into<String>,
    input: ClaudeCodeHookInput,
    hook_event_name: String,
) -> anyhow::Result<LocalSignalEvent> {
    let common = common_hook_fields(&input)?;
    let model = input
        .model
        .as_deref()
        .and_then(present)
        .context("claude_code SessionStart hook missing `model`")?;

    let mut event = LocalSignalEvent::new(
        source_id,
        DEFAULT_TITLE,
        "Claude Code session context updated.",
    );
    event.action = LocalSignalAction::StoreSessionContext;
    event.event = Some(SignalEvent {
        kind: SignalEventKind::Custom,
        raw_name: Some(hook_event_name),
    });
    event.workspace = Some(workspace_from_cwd(&common.cwd));
    event.conversation = Some(LocalSignalConversation {
        session_id: Some(common.session_id),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: None,
        model: Some(model),
    });
    event.metadata = metadata_from_hook_input(&input);

    Ok(event)
}

fn local_event_from_stop_hook(
    source_id: impl Into<String>,
    input: ClaudeCodeHookInput,
    hook_event_name: String,
) -> anyhow::Result<LocalSignalEvent> {
    let common = common_hook_fields(&input)?;
    let mut event = LocalSignalEvent::new(source_id, DEFAULT_TITLE, STOP_BODY);
    event.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some(hook_event_name),
    });
    event.workspace = Some(workspace_from_cwd(&common.cwd));
    event.conversation = Some(LocalSignalConversation {
        session_id: Some(common.session_id),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: input.last_assistant_message.as_deref().and_then(present),
        model: input.model.as_deref().and_then(present),
    });
    event.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at: None,
        duration_ms: None,
    });
    event.metadata = metadata_from_hook_input(&input);

    Ok(event)
}

fn local_event_from_notification_hook(
    source_id: impl Into<String>,
    input: ClaudeCodeHookInput,
    hook_event_name: String,
) -> anyhow::Result<LocalSignalEvent> {
    let common = common_hook_fields(&input)?;
    let title = input
        .title
        .as_deref()
        .and_then(present)
        .unwrap_or_else(|| DEFAULT_TITLE.to_string());
    let message = input
        .message
        .as_deref()
        .and_then(present)
        .context("claude_code Notification hook missing `message`")?;

    let mut event = LocalSignalEvent::new(source_id, title, message);
    event.event = Some(SignalEvent {
        kind: SignalEventKind::Custom,
        raw_name: Some(hook_event_name),
    });
    event.workspace = Some(workspace_from_cwd(&common.cwd));
    event.conversation = Some(LocalSignalConversation {
        session_id: Some(common.session_id),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: None,
        model: input.model.as_deref().and_then(present),
    });
    event.metadata = metadata_from_hook_input(&input);

    Ok(event)
}

struct CommonHookFields {
    session_id: String,
    cwd: String,
}

fn common_hook_fields(input: &ClaudeCodeHookInput) -> anyhow::Result<CommonHookFields> {
    let _transcript_path =
        present(&input.transcript_path).context("claude_code hook missing `transcript_path`")?;
    Ok(CommonHookFields {
        session_id: present(&input.session_id).context("claude_code hook missing `session_id`")?,
        cwd: present(&input.cwd).context("claude_code hook missing `cwd`")?,
    })
}

fn workspace_from_cwd(cwd: &str) -> SignalWorkspace {
    SignalWorkspace {
        cwd: Some(cwd.to_string()),
        project_name: project_name_from_cwd(cwd),
        project_path: Some(cwd.to_string()),
        branch: None,
        worktree: None,
    }
}

fn metadata_from_hook_input(input: &ClaudeCodeHookInput) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    if let Some(permission_mode) = input.permission_mode.as_deref().and_then(present) {
        metadata.insert("permission_mode".to_string(), permission_mode);
    }
    if let Some(source) = input.source.as_deref().and_then(present) {
        metadata.insert("session_start_source".to_string(), source);
    }
    if let Some(stop_hook_active) = input.stop_hook_active {
        metadata.insert("stop_hook_active".to_string(), stop_hook_active.to_string());
    }
    if let Some(notification_type) = input.notification_type.as_deref().and_then(present) {
        metadata.insert("notification_type".to_string(), notification_type);
    }
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CliConfig, Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType,
        RouteConfig, SourceConfig, SourceType,
    };

    #[test]
    fn creates_signal_from_claude_code_hook_event() {
        let config = test_config(SourceType::ClaudeCode);

        let signal = create_signal(
            &config,
            "claude_code",
            "Claude Code",
            "Claude Code finished a task.",
        )
        .expect("claude_code source should create signal");

        assert_eq!(signal.source_id(), "claude_code");
        assert_eq!(signal.source_type(), "claude_code");
        assert_eq!(signal.title(), "Claude Code");
        assert_eq!(signal.summary(), "Claude Code finished a task.");
    }

    #[test]
    fn rejects_non_claude_code_source() {
        let config = test_config(SourceType::CodexCli);

        let err = create_signal(&config, "claude_code", "Claude Code", "Body")
            .expect_err("wrong source type should fail");

        assert!(err.to_string().contains("expected `claude_code`"));
    }

    #[test]
    fn creates_local_event_from_stop_hook_input() {
        let event = local_event_from_hook(
            "claude_code",
            ClaudeCodeHookInput {
                session_id: "session-1".to_string(),
                transcript_path: "/Users/tester/.claude/projects/project/session-1.jsonl"
                    .to_string(),
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                hook_event_name: "Stop".to_string(),
                permission_mode: Some("default".to_string()),
                source: None,
                last_assistant_message: Some("Ready for review.".to_string()),
                model: Some("claude-sonnet-4-6".to_string()),
                stop_hook_active: Some(false),
                title: None,
                message: None,
                notification_type: None,
            },
        )
        .expect("stop hook should create local event");

        assert_eq!(event.source_id, "claude_code");
        assert_eq!(
            event.event.as_ref().map(|event| &event.kind),
            Some(&SignalEventKind::TurnCompleted)
        );
        assert_eq!(
            event
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.project_name.as_deref()),
            Some("agents-notifier")
        );
        assert_eq!(
            event
                .conversation
                .as_ref()
                .and_then(|conversation| conversation.session_id.as_deref()),
            Some("session-1")
        );
        assert_eq!(
            event
                .conversation
                .as_ref()
                .and_then(|conversation| conversation.model.as_deref()),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            event
                .conversation
                .as_ref()
                .and_then(|conversation| conversation.answer.as_deref()),
            Some("Ready for review.")
        );
        assert_eq!(
            event.metadata.get("permission_mode").map(String::as_str),
            Some("default")
        );
        assert!(
            !event.metadata.contains_key("transcript_path"),
            "transcript path should not leave the source adapter"
        );
    }

    #[test]
    fn creates_local_session_context_from_session_start_hook_input() {
        let event = local_event_from_hook(
            "claude_code",
            ClaudeCodeHookInput {
                session_id: "session-1".to_string(),
                transcript_path: "/Users/tester/.claude/projects/project/session-1.jsonl"
                    .to_string(),
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                hook_event_name: "SessionStart".to_string(),
                permission_mode: None,
                source: Some("startup".to_string()),
                last_assistant_message: None,
                model: Some("claude-sonnet-4-6".to_string()),
                stop_hook_active: None,
                title: None,
                message: None,
                notification_type: None,
            },
        )
        .expect("SessionStart hook should create session context event");

        assert_eq!(event.source_id, "claude_code");
        assert_eq!(event.action, LocalSignalAction::StoreSessionContext);
        assert_eq!(
            event
                .event
                .as_ref()
                .and_then(|event| event.raw_name.as_deref()),
            Some("SessionStart")
        );
        assert_eq!(
            event
                .conversation
                .as_ref()
                .and_then(|conversation| conversation.session_id.as_deref()),
            Some("session-1")
        );
        assert_eq!(
            event
                .conversation
                .as_ref()
                .and_then(|conversation| conversation.model.as_deref()),
            Some("claude-sonnet-4-6")
        );
        assert_eq!(
            event
                .metadata
                .get("session_start_source")
                .map(String::as_str),
            Some("startup")
        );
    }

    #[test]
    fn creates_local_event_from_notification_hook_input() {
        let event = local_event_from_hook(
            "claude_code",
            ClaudeCodeHookInput {
                session_id: "session-1".to_string(),
                transcript_path: "/Users/tester/.claude/projects/project/session-1.jsonl"
                    .to_string(),
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                hook_event_name: "Notification".to_string(),
                permission_mode: Some("default".to_string()),
                source: None,
                last_assistant_message: None,
                model: None,
                stop_hook_active: None,
                title: Some("Claude Code".to_string()),
                message: Some("Claude Code needs your attention.".to_string()),
                notification_type: Some("permission_request".to_string()),
            },
        )
        .expect("notification hook should create local event");

        assert_eq!(event.title, "Claude Code");
        assert_eq!(event.body, "Claude Code needs your attention.");
        assert_eq!(
            event.event.as_ref().map(|event| &event.kind),
            Some(&SignalEventKind::Custom)
        );
        assert_eq!(
            event.metadata.get("notification_type").map(String::as_str),
            Some("permission_request")
        );
    }

    #[test]
    fn rejects_unsupported_hook_event_name() {
        let err = local_event_from_hook(
            "claude_code",
            ClaudeCodeHookInput {
                session_id: "session-1".to_string(),
                transcript_path: "/Users/tester/.claude/projects/project/session-1.jsonl"
                    .to_string(),
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                hook_event_name: "PreToolUse".to_string(),
                permission_mode: Some("default".to_string()),
                source: None,
                last_assistant_message: None,
                model: None,
                stop_hook_active: None,
                title: None,
                message: None,
                notification_type: None,
            },
        )
        .expect_err("unsupported hook should fail");

        assert!(err.to_string().contains("SessionStart"));
    }

    fn test_config(source_type: SourceType) -> Config {
        Config {
            schema_version: 1,
            cli: CliConfig::default(),
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: "claude_code".to_string(),
                source_type,
            }],
            providers: vec![ProviderConfig {
                id: "debug".to_string(),
                provider_type: ProviderType::Webhook,
                base_url: None,
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
                host: None,
                port: None,
                security: None,
                username: None,
                username_env: None,
                password: None,
                password_env: None,
                from: None,
                to: None,
                reply_to: None,
                token: None,
                token_env: None,
                recipient_user_id: None,
                context_token: None,
                context_token_env: None,
                route_tag: None,
            }],
            routes: vec![RouteConfig::new(
                vec!["claude_code".to_string()],
                vec!["debug".to_string()],
            )],
        }
    }
}
