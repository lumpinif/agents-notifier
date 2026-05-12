use std::collections::BTreeMap;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::local_ingress::{LocalSignalConversation, LocalSignalEvent};
use crate::signal::{
    SignalEvent, SignalEventKind, SignalLifecycle, SignalLifecycleStatus, SignalWorkspace,
};
use crate::sources::hook_payload::{present, project_name_from_cwd};

const DEFAULT_TITLE: &str = "OpenCode CLI";
const DEFAULT_BODY: &str = "OpenCode CLI finished a task.";

#[derive(Debug, Deserialize)]
pub struct OpenCodeCliSessionInput {
    event: OpenCodeEventInput,
    cwd: String,
    #[serde(default)]
    worktree: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeEventInput {
    #[serde(rename = "type")]
    event_type: String,
    properties: OpenCodeEventPropertiesInput,
}

#[derive(Debug, Deserialize)]
struct OpenCodeEventPropertiesInput {
    #[serde(rename = "sessionID", alias = "session_id")]
    session_id: String,
    #[serde(default)]
    status: Option<OpenCodeStatusInput>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeStatusInput {
    #[serde(rename = "type")]
    status_type: String,
}

pub fn local_event_from_session_input(
    source_id: impl Into<String>,
    input: OpenCodeCliSessionInput,
) -> anyhow::Result<LocalSignalEvent> {
    let event_type =
        present(&input.event.event_type).context("opencode_cli hook missing `event.type`")?;
    let status_type = status_type_from_event(&event_type, input.event.properties.status.as_ref())?;
    if status_type != "idle" {
        anyhow::bail!(
            "opencode_cli ingest expected an idle session event, got status `{status_type}`"
        );
    }

    let session_id = present(&input.event.properties.session_id)
        .context("opencode_cli hook missing `event.properties.sessionID`")?;
    let cwd = present(&input.cwd).context("opencode_cli hook missing `cwd`")?;
    let completed_at = timestamp_from_input(input.timestamp.as_deref())?;

    let mut event = LocalSignalEvent::new(source_id, DEFAULT_TITLE, DEFAULT_BODY);
    event.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some(event_type),
    });
    event.workspace = Some(SignalWorkspace {
        cwd: Some(cwd.clone()),
        project_name: project_name_from_cwd(&cwd),
        project_path: Some(cwd),
        branch: None,
        worktree: input.worktree.as_deref().and_then(present),
    });
    event.conversation = Some(LocalSignalConversation {
        session_id: Some(session_id),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: None,
        model: input.model.as_deref().and_then(present),
    });
    event.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at,
        duration_ms: None,
    });
    event.metadata = metadata_from_session_event(status_type);

    Ok(event)
}

fn status_type_from_event(
    event_type: &str,
    status: Option<&OpenCodeStatusInput>,
) -> anyhow::Result<String> {
    match event_type {
        "session.idle" => Ok("idle".to_string()),
        "session.status" => status
            .and_then(|status| present(&status.status_type))
            .context("opencode_cli session.status event missing `event.properties.status.type`"),
        _ => anyhow::bail!(
            "opencode_cli ingest expected `event.type` to be `session.status` or `session.idle`, got `{event_type}`"
        ),
    }
}

fn timestamp_from_input(raw_timestamp: Option<&str>) -> anyhow::Result<Option<DateTime<Utc>>> {
    let Some(raw_timestamp) = raw_timestamp.and_then(present) else {
        return Ok(None);
    };

    let timestamp = DateTime::parse_from_rfc3339(&raw_timestamp)
        .with_context(|| format!("opencode_cli hook has invalid `timestamp`: `{raw_timestamp}`"))?
        .with_timezone(&Utc);
    Ok(Some(timestamp))
}

fn metadata_from_session_event(status_type: String) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert("status_type".to_string(), status_type);
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::{SignalEventKind, SignalLifecycleStatus};

    #[test]
    fn creates_local_event_from_session_status_idle_input() {
        let event = local_event_from_session_input(
            "opencode_cli",
            OpenCodeCliSessionInput {
                event: OpenCodeEventInput {
                    event_type: "session.status".to_string(),
                    properties: OpenCodeEventPropertiesInput {
                        session_id: "session-1".to_string(),
                        status: Some(OpenCodeStatusInput {
                            status_type: "idle".to_string(),
                        }),
                    },
                },
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                worktree: Some("main".to_string()),
                timestamp: Some("2026-05-12T10:15:30Z".to_string()),
                model: Some("opencode-model".to_string()),
            },
        )
        .expect("idle session status should create local event");

        assert_eq!(event.source_id, "opencode_cli");
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
            Some("opencode-model")
        );
        assert_eq!(
            event
                .lifecycle
                .as_ref()
                .and_then(|lifecycle| lifecycle.status),
            Some(SignalLifecycleStatus::Completed)
        );
        assert_eq!(
            event.metadata.get("status_type").map(String::as_str),
            Some("idle")
        );
    }

    #[test]
    fn rejects_non_idle_session_status_input() {
        let err = local_event_from_session_input(
            "opencode_cli",
            OpenCodeCliSessionInput {
                event: OpenCodeEventInput {
                    event_type: "session.status".to_string(),
                    properties: OpenCodeEventPropertiesInput {
                        session_id: "session-1".to_string(),
                        status: Some(OpenCodeStatusInput {
                            status_type: "active".to_string(),
                        }),
                    },
                },
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                worktree: None,
                timestamp: None,
                model: None,
            },
        )
        .expect_err("active session status should fail");

        assert!(err.to_string().contains("idle session event"));
    }
}
