use std::collections::BTreeMap;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::local_ingress::{LocalSignalConversation, LocalSignalEvent};
use crate::signal::{
    SignalEvent, SignalEventKind, SignalLifecycle, SignalLifecycleStatus, SignalWorkspace,
};
use crate::sources::hook_payload::{present, project_name_from_cwd};

const DEFAULT_TITLE: &str = "Gemini CLI";
const AFTER_AGENT_BODY: &str = "Gemini CLI finished a task.";

#[derive(Debug, Deserialize)]
pub struct GeminiCliHookInput {
    session_id: String,
    transcript_path: String,
    cwd: String,
    hook_event_name: String,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    prompt_response: Option<String>,
    #[serde(default)]
    stop_hook_active: Option<bool>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    notification_type: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

pub fn local_event_from_hook(
    source_id: impl Into<String>,
    input: GeminiCliHookInput,
) -> anyhow::Result<LocalSignalEvent> {
    let hook_event_name =
        present(&input.hook_event_name).context("gemini_cli hook missing `hook_event_name`")?;

    match hook_event_name.as_str() {
        "AfterAgent" => local_event_from_after_agent_hook(source_id, input, hook_event_name),
        "Notification" => local_event_from_notification_hook(source_id, input, hook_event_name),
        event_name => {
            anyhow::bail!(
                "gemini_cli ingest expected `hook_event_name` to be `AfterAgent` or `Notification`, got `{event_name}`"
            )
        }
    }
}

fn local_event_from_after_agent_hook(
    source_id: impl Into<String>,
    input: GeminiCliHookInput,
    hook_event_name: String,
) -> anyhow::Result<LocalSignalEvent> {
    let common = common_hook_fields(&input)?;
    let completed_at = timestamp_from_input(input.timestamp.as_deref())?;

    let mut event = LocalSignalEvent::new(source_id, DEFAULT_TITLE, AFTER_AGENT_BODY);
    event.event = Some(SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some(hook_event_name),
    });
    event.workspace = Some(workspace_from_cwd(&common.cwd));
    event.conversation = Some(LocalSignalConversation {
        session_id: Some(common.session_id),
        session_title: None,
        turn_id: None,
        prompt: input.prompt.as_deref().and_then(present),
        answer: input.prompt_response.as_deref().and_then(present),
        model: input.model.as_deref().and_then(present),
    });
    event.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at,
        duration_ms: None,
    });
    event.metadata = metadata_from_hook_input(&input);

    Ok(event)
}

fn local_event_from_notification_hook(
    source_id: impl Into<String>,
    input: GeminiCliHookInput,
    hook_event_name: String,
) -> anyhow::Result<LocalSignalEvent> {
    let common = common_hook_fields(&input)?;
    let message = input
        .message
        .as_deref()
        .and_then(present)
        .context("gemini_cli Notification hook missing `message`")?;

    let mut event = LocalSignalEvent::new(source_id, DEFAULT_TITLE, message);
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

fn common_hook_fields(input: &GeminiCliHookInput) -> anyhow::Result<CommonHookFields> {
    let _transcript_path =
        present(&input.transcript_path).context("gemini_cli hook missing `transcript_path`")?;
    Ok(CommonHookFields {
        session_id: present(&input.session_id).context("gemini_cli hook missing `session_id`")?,
        cwd: present(&input.cwd).context("gemini_cli hook missing `cwd`")?,
    })
}

fn timestamp_from_input(raw_timestamp: Option<&str>) -> anyhow::Result<Option<DateTime<Utc>>> {
    let Some(raw_timestamp) = raw_timestamp.and_then(present) else {
        return Ok(None);
    };

    let timestamp = DateTime::parse_from_rfc3339(&raw_timestamp)
        .with_context(|| format!("gemini_cli hook has invalid `timestamp`: `{raw_timestamp}`"))?
        .with_timezone(&Utc);
    Ok(Some(timestamp))
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

fn metadata_from_hook_input(input: &GeminiCliHookInput) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
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
    use crate::signal::{SignalEventKind, SignalLifecycleStatus};

    #[test]
    fn creates_local_event_from_after_agent_hook_input() {
        let event = local_event_from_hook(
            "gemini_cli",
            GeminiCliHookInput {
                session_id: "session-1".to_string(),
                transcript_path: "/Users/tester/.gemini/tmp/session-1.json".to_string(),
                cwd: "/Users/tester/projects/agents-router".to_string(),
                hook_event_name: "AfterAgent".to_string(),
                timestamp: Some("2026-05-12T10:15:30Z".to_string()),
                prompt: Some("Review this patch.".to_string()),
                prompt_response: Some("Ready for review.".to_string()),
                stop_hook_active: Some(false),
                model: Some("gemini-3-pro".to_string()),
                notification_type: None,
                message: None,
            },
        )
        .expect("AfterAgent hook should create local event");

        assert_eq!(event.source_id, "gemini_cli");
        assert_eq!(
            event.event.as_ref().map(|event| &event.kind),
            Some(&SignalEventKind::TurnCompleted)
        );
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
                .and_then(|conversation| conversation.prompt.as_deref()),
            Some("Review this patch.")
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
                .conversation
                .as_ref()
                .and_then(|conversation| conversation.model.as_deref()),
            Some("gemini-3-pro")
        );
        assert_eq!(
            event
                .lifecycle
                .as_ref()
                .and_then(|lifecycle| lifecycle.status),
            Some(SignalLifecycleStatus::Completed)
        );
        assert_eq!(
            event
                .lifecycle
                .as_ref()
                .and_then(|lifecycle| lifecycle.completed_at)
                .map(|timestamp| timestamp.to_rfc3339()),
            Some("2026-05-12T10:15:30+00:00".to_string())
        );
        assert!(
            !event.metadata.contains_key("transcript_path"),
            "transcript path should not leave the source adapter"
        );
    }

    #[test]
    fn creates_local_event_from_notification_hook_input() {
        let event = local_event_from_hook(
            "gemini_cli",
            GeminiCliHookInput {
                session_id: "session-1".to_string(),
                transcript_path: "/Users/tester/.gemini/tmp/session-1.json".to_string(),
                cwd: "/Users/tester/projects/agents-router".to_string(),
                hook_event_name: "Notification".to_string(),
                timestamp: Some("2026-05-12T10:15:30Z".to_string()),
                prompt: None,
                prompt_response: None,
                stop_hook_active: None,
                model: None,
                notification_type: Some("ToolPermission".to_string()),
                message: Some("Gemini CLI needs your attention.".to_string()),
            },
        )
        .expect("Notification hook should create local event");

        assert_eq!(event.title, "Gemini CLI");
        assert_eq!(event.body, "Gemini CLI needs your attention.");
        assert_eq!(
            event.event.as_ref().map(|event| &event.kind),
            Some(&SignalEventKind::Custom)
        );
        assert_eq!(
            event.metadata.get("notification_type").map(String::as_str),
            Some("ToolPermission")
        );
    }

    #[test]
    fn rejects_unsupported_hook_event_name() {
        let err = local_event_from_hook(
            "gemini_cli",
            GeminiCliHookInput {
                session_id: "session-1".to_string(),
                transcript_path: "/Users/tester/.gemini/tmp/session-1.json".to_string(),
                cwd: "/Users/tester/projects/agents-router".to_string(),
                hook_event_name: "BeforeTool".to_string(),
                timestamp: None,
                prompt: None,
                prompt_response: None,
                stop_hook_active: None,
                model: None,
                notification_type: None,
                message: None,
            },
        )
        .expect_err("unsupported hook should fail");

        assert!(err.to_string().contains("AfterAgent` or `Notification"));
    }
}
