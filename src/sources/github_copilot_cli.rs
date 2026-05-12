use std::collections::BTreeMap;

use anyhow::Context;
use serde::Deserialize;

use crate::local_ingress::{LocalSignalConversation, LocalSignalEvent};
use crate::signal::{SignalEvent, SignalEventKind, SignalWorkspace};
use crate::sources::hook_payload::{present, project_name_from_cwd};

const DEFAULT_TITLE: &str = "GitHub Copilot CLI";

#[derive(Debug, Deserialize)]
pub struct GithubCopilotCliNotificationInput {
    #[serde(rename = "sessionId", alias = "session_id")]
    session_id: String,
    timestamp: i64,
    cwd: String,
    hook_event_name: String,
    message: String,
    #[serde(default)]
    title: Option<String>,
    notification_type: String,
}

pub fn local_event_from_notification_hook(
    source_id: impl Into<String>,
    input: GithubCopilotCliNotificationInput,
) -> anyhow::Result<LocalSignalEvent> {
    let hook_event_name = present(&input.hook_event_name)
        .context("github_copilot_cli hook missing `hook_event_name`")?;
    if hook_event_name != "Notification" {
        anyhow::bail!(
            "github_copilot_cli ingest expected `hook_event_name = Notification`, got `{hook_event_name}`"
        );
    }

    let session_id =
        present(&input.session_id).context("github_copilot_cli hook missing `sessionId`")?;
    let cwd = present(&input.cwd).context("github_copilot_cli hook missing `cwd`")?;
    let message = present(&input.message).context("github_copilot_cli hook missing `message`")?;
    let notification_type = present(&input.notification_type)
        .context("github_copilot_cli hook missing `notification_type`")?;
    let title = input
        .title
        .as_deref()
        .and_then(present)
        .unwrap_or_else(|| DEFAULT_TITLE.to_string());

    let mut event = LocalSignalEvent::new(source_id, title, message);
    event.event = Some(SignalEvent {
        kind: SignalEventKind::Custom,
        raw_name: Some(hook_event_name),
    });
    event.workspace = Some(SignalWorkspace {
        cwd: Some(cwd.clone()),
        project_name: project_name_from_cwd(&cwd),
        project_path: Some(cwd),
        branch: None,
        worktree: None,
    });
    event.conversation = Some(LocalSignalConversation {
        session_id: Some(session_id),
        session_title: None,
        turn_id: None,
        prompt: None,
        answer: None,
        model: None,
    });
    event.metadata = metadata_from_notification_input(input.timestamp, notification_type);

    Ok(event)
}

fn metadata_from_notification_input(
    timestamp: i64,
    notification_type: String,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert("timestamp_ms".to_string(), timestamp.to_string());
    metadata.insert("notification_type".to_string(), notification_type);
    metadata
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::SignalEventKind;

    #[test]
    fn creates_local_event_from_notification_hook_input() {
        let event = local_event_from_notification_hook(
            "github_copilot_cli",
            GithubCopilotCliNotificationInput {
                session_id: "session-1".to_string(),
                timestamp: 1775907268684,
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                hook_event_name: "Notification".to_string(),
                message: "GitHub Copilot CLI emitted a notification.".to_string(),
                title: Some("GitHub Copilot CLI".to_string()),
                notification_type: "agent_idle".to_string(),
            },
        )
        .expect("notification hook should create local event");

        assert_eq!(event.source_id, "github_copilot_cli");
        assert_eq!(event.title, "GitHub Copilot CLI");
        assert_eq!(event.body, "GitHub Copilot CLI emitted a notification.");
        assert_eq!(
            event.event.as_ref().map(|event| &event.kind),
            Some(&SignalEventKind::Custom)
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
            event.metadata.get("notification_type").map(String::as_str),
            Some("agent_idle")
        );
        assert_eq!(
            event.metadata.get("timestamp_ms").map(String::as_str),
            Some("1775907268684")
        );
    }

    #[test]
    fn rejects_non_notification_hook_input() {
        let err = local_event_from_notification_hook(
            "github_copilot_cli",
            GithubCopilotCliNotificationInput {
                session_id: "session-1".to_string(),
                timestamp: 1775907268684,
                cwd: "/Users/tester/projects/agents-notifier".to_string(),
                hook_event_name: "SessionStart".to_string(),
                message: "Started.".to_string(),
                title: None,
                notification_type: "agent_idle".to_string(),
            },
        )
        .expect_err("non-notification hook should fail");

        assert!(err.to_string().contains("Notification"));
    }
}
