use std::collections::BTreeMap;

use anyhow::Context;
use serde::Deserialize;

use crate::local_ingress::{LocalSignalConversation, LocalSignalEvent};
use crate::signal::{SignalEvent, SignalLifecycle, SignalLink, SignalWorkspace};

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
                    cwd: Some("/Users/tester/projects/agents-router".to_string()),
                    project_name: Some("agents-router".to_string()),
                    project_path: Some("/Users/tester/projects/agents-router".to_string()),
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
}
