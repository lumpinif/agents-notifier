use anyhow::Context;
use serde::Deserialize;

use crate::config::{Config, SourceType};
use crate::local_ingress::{LocalSignalConversation, LocalSignalEvent};
use crate::signal::{
    Signal, SignalEvent, SignalEventKind, SignalLifecycle, SignalLifecycleStatus, SignalWorkspace,
};
use crate::sources::agent_hook;
use crate::sources::hook_payload::{present, project_name_from_cwd};

const DEFAULT_TITLE: &str = "Codex CLI";
const DEFAULT_BODY: &str = "Codex CLI finished a task.";

pub fn create_signal(
    config: &Config,
    source_id: &str,
    title: impl Into<String>,
    body: impl Into<String>,
) -> anyhow::Result<Signal> {
    agent_hook::create_signal_for_type(config, source_id, SourceType::CodexCli, title, body)
}

#[derive(Debug, Deserialize)]
pub struct CodexCliStopHookInput {
    cwd: String,
    hook_event_name: String,
    #[serde(default)]
    last_assistant_message: Option<String>,
    #[serde(default)]
    model: Option<String>,
    session_id: String,
    turn_id: String,
}

pub fn local_event_from_stop_hook(
    source_id: impl Into<String>,
    input: CodexCliStopHookInput,
) -> anyhow::Result<LocalSignalEvent> {
    if input.hook_event_name != "Stop" {
        anyhow::bail!(
            "codex_cli stop ingest expected `hook_event_name = Stop`, got `{}`",
            input.hook_event_name
        );
    }

    let cwd = present(input.cwd).context("codex_cli stop hook missing `cwd`")?;
    let session_id =
        present(input.session_id).context("codex_cli stop hook missing `session_id`")?;
    let turn_id = present(input.turn_id).context("codex_cli stop hook missing `turn_id`")?;

    let mut event = LocalSignalEvent::new(source_id, DEFAULT_TITLE, DEFAULT_BODY);
    event.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some("Stop".to_string()),
    });
    event.workspace = Some(SignalWorkspace {
        project_name: project_name_from_cwd(&cwd),
        project_path: Some(cwd.clone()),
        cwd: Some(cwd),
        branch: None,
        worktree: None,
    });
    event.conversation = Some(LocalSignalConversation {
        session_id: Some(session_id),
        session_title: None,
        turn_id: Some(turn_id),
        prompt: None,
        answer: input.last_assistant_message.and_then(present),
        model: input.model.and_then(present),
    });
    event.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at: None,
        duration_ms: None,
    });

    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CliConfig, Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType,
        RouteConfig, SourceConfig, SourceType,
    };
    use crate::signal::SignalLifecycleStatus;

    #[test]
    fn creates_signal_from_cli_args() {
        let config = test_config(SourceType::CodexCli);

        let signal = create_signal(&config, "codex_cli", "Codex", "Ready for review.")
            .expect("codex_cli source should create signal");

        assert_eq!(signal.source_id(), "codex_cli");
        assert_eq!(signal.source_type(), "codex_cli");
        assert_eq!(signal.title(), "Codex");
        assert_eq!(signal.summary(), "Ready for review.");
    }

    #[test]
    fn rejects_missing_source() {
        let config = test_config(SourceType::CodexCli);

        let err = create_signal(&config, "missing", "Codex", "Body")
            .expect_err("missing source should fail");

        assert!(
            err.to_string()
                .contains("source `missing` is not configured")
        );
    }

    #[test]
    fn rejects_non_codex_cli_source() {
        let config = test_config(SourceType::CodexDesktop);

        let err = create_signal(&config, "codex_cli", "Codex", "Body")
            .expect_err("wrong source type should fail");

        assert!(err.to_string().contains("expected `codex_cli`"));
    }

    #[test]
    fn creates_local_event_from_stop_hook_input() {
        let event = local_event_from_stop_hook(
            "codex_cli",
            CodexCliStopHookInput {
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                hook_event_name: "Stop".to_string(),
                last_assistant_message: Some("Ready for review.".to_string()),
                model: Some("gpt-5.2-codex".to_string()),
                session_id: "session-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
        )
        .expect("stop hook should create local event");

        assert_eq!(event.source_id, "codex_cli");
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
                .and_then(|conversation| conversation.model.as_deref()),
            Some("gpt-5.2-codex")
        );
        assert_eq!(
            event
                .conversation
                .as_ref()
                .and_then(|conversation| conversation.answer.as_deref()),
            Some("Ready for review.")
        );
        assert_eq!(
            event
                .lifecycle
                .as_ref()
                .and_then(|lifecycle| lifecycle.status),
            Some(SignalLifecycleStatus::Completed)
        );
    }

    #[test]
    fn creates_local_event_without_model_when_stop_hook_omits_model() {
        let event = local_event_from_stop_hook(
            "codex_cli",
            CodexCliStopHookInput {
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                hook_event_name: "Stop".to_string(),
                last_assistant_message: Some("Ready for review.".to_string()),
                model: None,
                session_id: "session-1".to_string(),
                turn_id: "turn-1".to_string(),
            },
        )
        .expect("stop hook without model should still create local event");

        assert_eq!(
            event
                .conversation
                .as_ref()
                .and_then(|conversation| conversation.model.as_deref()),
            None
        );
    }

    fn test_config(source_type: SourceType) -> Config {
        Config {
            schema_version: 1,
            cli: CliConfig::default(),
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: "codex_cli".to_string(),
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
                vec!["codex_cli".to_string()],
                vec!["debug".to_string()],
            )],
        }
    }
}
