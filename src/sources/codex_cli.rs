use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

use crate::config::{SourceType, ValidatedConfig};
use crate::local_ingress::{LocalSignalConversation, LocalSignalEvent};
use crate::signal::{
    Signal, SignalEvent, SignalEventKind, SignalLifecycle, SignalLifecycleStatus, SignalWorkspace,
};
use crate::sources::agent_hook;
use crate::sources::codex_desktop::{self, CodexSessionOrigin};
use crate::sources::hook_payload::{present, project_name_from_cwd};

const DEFAULT_TITLE: &str = "Codex CLI";
const DEFAULT_BODY: &str = "Codex CLI finished a task.";

pub fn create_signal(
    config: &ValidatedConfig,
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

pub fn stop_hook_input_is_shadowed_by_codex_desktop(
    config: &ValidatedConfig,
    input: &CodexCliStopHookInput,
    sessions_dir: &Path,
) -> anyhow::Result<bool> {
    if !has_codex_desktop_source(config) {
        return Ok(false);
    }

    let session_id = input.session_id.trim();
    if session_id.is_empty() {
        return Ok(false);
    }

    Ok(matches!(
        codex_desktop::codex_session_origin(sessions_dir, session_id)?,
        CodexSessionOrigin::Desktop
    ))
}

pub fn local_stop_event_is_shadowed_by_codex_desktop(
    config: &ValidatedConfig,
    event: &LocalSignalEvent,
    sessions_dir: &Path,
) -> anyhow::Result<bool> {
    if event.source_id != SourceType::CodexCli.as_str() || !has_codex_desktop_source(config) {
        return Ok(false);
    }
    if !matches!(
        event.event.as_ref(),
        Some(SignalEvent {
            kind: SignalEventKind::TurnCompleted,
            raw_name: Some(raw_name),
        }) if raw_name == "Stop"
    ) {
        return Ok(false);
    }

    let Some(session_id) = event
        .conversation
        .as_ref()
        .and_then(|conversation| conversation.session_id.as_deref())
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())
    else {
        return Ok(false);
    };

    Ok(matches!(
        codex_desktop::codex_session_origin(sessions_dir, session_id)?,
        CodexSessionOrigin::Desktop
    ))
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

fn has_codex_desktop_source(config: &ValidatedConfig) -> bool {
    config
        .sources
        .iter()
        .any(|source| source.source_type == SourceType::CodexDesktop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use crate::config::{
        CliConfig, LogConfig, NotificationConfig, ProviderType, RawConfig, RawProviderConfig,
        RouteConfig, SourceConfig, SourceType, ValidatedConfig,
    };
    use crate::signal::SignalLifecycleStatus;
    use tempfile::tempdir;

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
                cwd: "/Users/tester/projects/agents-router".to_string(),
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
            Some("agents-router")
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
                cwd: "/Users/tester/projects/agents-router".to_string(),
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

    #[test]
    fn desktop_origin_stop_hook_is_shadowed_when_desktop_source_is_enabled() {
        let sessions_dir = tempdir().expect("sessions dir should be created");
        write_session_meta(sessions_dir.path(), "session-1", "Codex Desktop");
        let input = stop_hook_input("session-1");
        let mut config = test_config(SourceType::CodexCli);
        config.sources.push(SourceConfig {
            id: "codex_desktop".to_string(),
            source_type: SourceType::CodexDesktop,
        });

        let shadowed =
            stop_hook_input_is_shadowed_by_codex_desktop(&config, &input, sessions_dir.path())
                .expect("origin should be classified");

        assert!(shadowed);
    }

    #[test]
    fn unknown_origin_stop_hook_is_not_shadowed() {
        let sessions_dir = tempdir().expect("sessions dir should be created");
        let input = stop_hook_input("session-1");
        let mut config = test_config(SourceType::CodexCli);
        config.sources.push(SourceConfig {
            id: "codex_desktop".to_string(),
            source_type: SourceType::CodexDesktop,
        });

        let shadowed =
            stop_hook_input_is_shadowed_by_codex_desktop(&config, &input, sessions_dir.path())
                .expect("missing session should be unknown");

        assert!(!shadowed);
    }

    #[test]
    fn cli_origin_stop_hook_is_not_shadowed() {
        let sessions_dir = tempdir().expect("sessions dir should be created");
        write_session_meta(sessions_dir.path(), "session-1", "codex_cli_rs");
        let input = stop_hook_input("session-1");
        let mut config = test_config(SourceType::CodexCli);
        config.sources.push(SourceConfig {
            id: "codex_desktop".to_string(),
            source_type: SourceType::CodexDesktop,
        });

        let shadowed =
            stop_hook_input_is_shadowed_by_codex_desktop(&config, &input, sessions_dir.path())
                .expect("CLI origin should be classified");

        assert!(!shadowed);
    }

    #[test]
    fn desktop_origin_stop_hook_is_not_shadowed_without_desktop_source() {
        let sessions_dir = tempdir().expect("sessions dir should be created");
        write_session_meta(sessions_dir.path(), "session-1", "Codex Desktop");
        let input = stop_hook_input("session-1");
        let config = test_config(SourceType::CodexCli);

        let shadowed =
            stop_hook_input_is_shadowed_by_codex_desktop(&config, &input, sessions_dir.path())
                .expect("origin should be ignored when desktop source is disabled");

        assert!(!shadowed);
    }

    #[test]
    fn desktop_origin_local_stop_event_is_shadowed_when_desktop_source_is_enabled() {
        let sessions_dir = tempdir().expect("sessions dir should be created");
        write_session_meta(sessions_dir.path(), "session-1", "Codex Desktop");
        let input = stop_hook_input("session-1");
        let event = local_event_from_stop_hook("codex_cli", input)
            .expect("stop hook should create local event");
        let mut config = test_config(SourceType::CodexCli);
        config.sources.push(SourceConfig {
            id: "codex_desktop".to_string(),
            source_type: SourceType::CodexDesktop,
        });

        let shadowed =
            local_stop_event_is_shadowed_by_codex_desktop(&config, &event, sessions_dir.path())
                .expect("origin should be classified");

        assert!(shadowed);
    }

    fn test_config(source_type: SourceType) -> ValidatedConfig {
        let mut provider = RawProviderConfig::new("debug", ProviderType::Webhook);
        provider.url = Some("https://example.com/hook".to_string());

        RawConfig {
            schema_version: 1,
            cli: CliConfig::default(),
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: "codex_cli".to_string(),
                source_type,
            }],
            providers: vec![provider],
            routes: vec![RouteConfig::new(
                vec!["codex_cli".to_string()],
                vec!["debug".to_string()],
            )],
        }
        .validate()
        .expect("test config should validate")
    }

    fn stop_hook_input(session_id: &str) -> CodexCliStopHookInput {
        CodexCliStopHookInput {
            cwd: "/Users/tester/projects/agents-router".to_string(),
            hook_event_name: "Stop".to_string(),
            last_assistant_message: Some("Ready for review.".to_string()),
            model: Some("gpt-5.2-codex".to_string()),
            session_id: session_id.to_string(),
            turn_id: "turn-1".to_string(),
        }
    }

    fn write_session_meta(sessions_dir: &Path, session_id: &str, originator: &str) {
        let path = sessions_dir.join(format!("rollout-2026-05-14T00-00-00-{session_id}.jsonl"));
        fs::write(
            path,
            format!(
                r#"{{"timestamp":"2026-05-14T00:00:00.000Z","type":"session_meta","payload":{{"id":"{session_id}","originator":"{originator}","cwd":"/Users/tester/projects/agents-router","model":"gpt-5.2-codex"}}}}
"#
            ),
        )
        .expect("session meta should be written");
    }
}
