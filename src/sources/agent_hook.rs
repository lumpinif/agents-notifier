use std::collections::BTreeMap;

use anyhow::{Context, anyhow};
use serde::Deserialize;

use crate::config::{Config, SourceType};
use crate::local_ingress::{LocalSignalConversation, LocalSignalEvent};
use crate::signal::{Signal, SignalEvent, SignalLifecycle, SignalLink, SignalWorkspace};

pub fn create_signal(
    config: &Config,
    source_id: &str,
    title: impl Into<String>,
    body: impl Into<String>,
) -> anyhow::Result<Signal> {
    create_signal_for_type(config, source_id, SourceType::AgentHook, title, body)
}

pub fn create_signal_for_type(
    config: &Config,
    source_id: &str,
    expected_source_type: SourceType,
    title: impl Into<String>,
    body: impl Into<String>,
) -> anyhow::Result<Signal> {
    let source = config
        .source(source_id)
        .with_context(|| format!("source `{source_id}` is not configured"))?;

    if source.source_type != expected_source_type {
        return Err(anyhow!(
            "source `{}` has type `{}`; expected `{}`",
            source.id,
            source.source_type.as_str(),
            expected_source_type.as_str()
        ));
    }

    Ok(Signal::new(
        source.id.clone(),
        source.source_type.as_str(),
        title,
        body,
        BTreeMap::new(),
    ))
}

#[derive(Debug, Deserialize)]
pub struct StructuredAgentHookInput {
    pub display: StructuredAgentHookDisplay,
    pub event: SignalEvent,
    #[serde(default)]
    pub workspace: Option<SignalWorkspace>,
    #[serde(default)]
    pub conversation: Option<LocalSignalConversation>,
    #[serde(default)]
    pub lifecycle: Option<SignalLifecycle>,
    #[serde(default)]
    pub links: Vec<SignalLink>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct StructuredAgentHookDisplay {
    pub title: String,
    pub summary: String,
}

pub fn local_event_from_structured_hook(
    source_id: impl Into<String>,
    input: StructuredAgentHookInput,
) -> anyhow::Result<LocalSignalEvent> {
    let title = present(input.display.title).context("agent_hook event missing `display.title`")?;
    let summary =
        present(input.display.summary).context("agent_hook event missing `display.summary`")?;

    let mut event = LocalSignalEvent::new(source_id, title, summary);
    event.event = Some(input.event);
    event.workspace = input.workspace;
    event.conversation = input.conversation;
    event.lifecycle = input.lifecycle;
    event.links = input.links;
    event.metadata = input.metadata;
    Ok(event)
}

fn present(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CliConfig, Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType,
        RouteConfig, SourceConfig, SourceType,
    };

    #[test]
    fn creates_signal_for_expected_hook_source_type() {
        let config = test_config("opencode_cli", SourceType::AgentHook);

        let signal = create_signal(&config, "opencode_cli", "OpenCode CLI", "Ready for review.")
            .expect("expected hook source should create signal");

        assert_eq!(signal.source_id(), "opencode_cli");
        assert_eq!(signal.source_type(), "agent_hook");
        assert_eq!(signal.title(), "OpenCode CLI");
        assert_eq!(signal.summary(), "Ready for review.");
    }

    #[test]
    fn rejects_missing_hook_source() {
        let config = test_config("codex_cli", SourceType::CodexCli);

        let err = create_signal_for_type(&config, "missing", SourceType::CodexCli, "Codex", "Body")
            .expect_err("missing source should fail");

        assert!(
            err.to_string()
                .contains("source `missing` is not configured")
        );
    }

    #[test]
    fn rejects_unexpected_hook_source_type() {
        let config = test_config("claude_code", SourceType::ClaudeCode);

        let err = create_signal_for_type(
            &config,
            "claude_code",
            SourceType::CodexCli,
            "Codex",
            "Body",
        )
        .expect_err("wrong source type should fail");

        assert!(
            err.to_string()
                .contains("source `claude_code` has type `claude_code`; expected `codex_cli`")
        );
    }

    #[test]
    fn creates_local_event_from_structured_hook_input() {
        let event = local_event_from_structured_hook(
            "cursor_cli",
            StructuredAgentHookInput {
                display: StructuredAgentHookDisplay {
                    title: "Cursor CLI".to_string(),
                    summary: "Cursor CLI finished a task.".to_string(),
                },
                event: crate::signal::SignalEvent {
                    kind: crate::signal::SignalEventKind::TurnCompleted,
                    raw_name: Some("wrapper.completed".to_string()),
                },
                workspace: Some(SignalWorkspace {
                    cwd: Some("/Users/tester/projects/agents-notifier".to_string()),
                    project_name: Some("agents-notifier".to_string()),
                    project_path: Some("/Users/tester/projects/agents-notifier".to_string()),
                    branch: None,
                    worktree: None,
                }),
                conversation: Some(LocalSignalConversation {
                    session_id: None,
                    session_title: None,
                    turn_id: None,
                    prompt: None,
                    answer: Some("Ready for review.".to_string()),
                    model: Some("cursor-model".to_string()),
                }),
                lifecycle: None,
                links: Vec::new(),
                metadata: BTreeMap::new(),
            },
        )
        .expect("structured hook should create local event");

        assert_eq!(event.source_id, "cursor_cli");
        assert_eq!(event.title, "Cursor CLI");
        assert_eq!(event.body, "Cursor CLI finished a task.");
        assert_eq!(
            event
                .conversation
                .as_ref()
                .and_then(|conversation| conversation.model.as_deref()),
            Some("cursor-model")
        );
    }

    fn test_config(source_id: &str, source_type: SourceType) -> Config {
        Config {
            schema_version: 1,
            cli: CliConfig::default(),
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: source_id.to_string(),
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
            routes: vec![RouteConfig {
                sources: vec![source_id.to_string()],
                providers: vec!["debug".to_string()],
            }],
        }
    }
}
