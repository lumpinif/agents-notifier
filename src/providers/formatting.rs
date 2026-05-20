use crate::providers::notification_view::{
    NotificationAction, NotificationFieldKey, NotificationSection, SignalNotificationView,
};
use crate::signal::Signal;

pub(super) fn format_signal_message(signal: &Signal, formatted_time: &str) -> String {
    let view = SignalNotificationView::from_signal(signal, formatted_time);
    format!("{}\n\n{}", view.title, format_notification_view_body(&view))
}

pub(super) fn format_signal_body(signal: &Signal, formatted_time: &str) -> String {
    let view = SignalNotificationView::from_signal(signal, formatted_time);
    format_notification_view_body(&view)
}

fn format_notification_view_body(view: &SignalNotificationView) -> String {
    let mut details = Vec::new();
    if let Some(summary) = &view.summary {
        details.push(summary.to_string());
    }
    push_fields_and_actions(&mut details, view);

    let mut sections = Vec::new();
    sections.push(details.join("\n"));

    for section in &view.sections {
        match section {
            NotificationSection::Prompt(prompt) => sections.push(format!("Prompt: {prompt}")),
            NotificationSection::Preview(answer) => sections.push(format!("Preview: {answer}")),
            NotificationSection::Answer(answer) => sections.push(format!("Answer: {answer}")),
        }
    }

    sections.join("\n\n")
}

fn push_fields_and_actions(lines: &mut Vec<String>, view: &SignalNotificationView) {
    let mut actions_pushed = false;
    for field in &view.fields {
        if !actions_pushed
            && matches!(
                field.key,
                NotificationFieldKey::Duration
                    | NotificationFieldKey::Branch
                    | NotificationFieldKey::Time
                    | NotificationFieldKey::SessionId
            )
        {
            push_actions(lines, &view.actions);
            actions_pushed = true;
        }
        lines.push(format!("{}: {}", field.label, field.value));
    }

    if !actions_pushed {
        push_actions(lines, &view.actions);
    }
}

fn push_actions(lines: &mut Vec<String>, actions: &[NotificationAction]) {
    for action in actions {
        lines.push(format!("{}: {}", action.label, action.url));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};

    use super::*;
    use crate::signal::{
        SignalAnswer, SignalAnswerKind, SignalConversation, SignalLifecycle, SignalLifecycleStatus,
        SignalLink, SignalWorkspace,
    };

    #[test]
    fn formats_minimal_signal_body() {
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex CLI",
            "Ready for review.",
            timestamp(),
            BTreeMap::new(),
        );

        assert_eq!(
            format_signal_body(&signal, "2026-05-10 10:31:43 +08:00"),
            "Ready for review.\nTime: 2026-05-10 10:31:43 +08:00"
        );
    }

    #[test]
    fn formats_summary_when_simple_signal_only_adds_duration() {
        let mut signal = Signal::new_with_timestamp(
            "signal-1",
            "aider",
            "agent_hook",
            "Aider",
            "Aider finished a task.",
            timestamp(),
            BTreeMap::new(),
        );
        signal.lifecycle = Some(SignalLifecycle {
            status: Some(SignalLifecycleStatus::Completed),
            started_at: None,
            completed_at: None,
            duration_ms: Some(420_000),
        });

        assert_eq!(
            format_signal_body(&signal, "2026-05-10 10:31:43 +08:00"),
            "Aider finished a task.\nDuration: 7m 0s\nTime: 2026-05-10 10:31:43 +08:00"
        );
    }

    #[test]
    fn formats_structured_codex_signal_body_in_stable_order() {
        let mut signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_desktop",
            "codex_desktop",
            "Codex Desktop",
            "Codex Desktop finished a task.",
            timestamp(),
            BTreeMap::new(),
        );
        signal.workspace = Some(SignalWorkspace {
            cwd: Some("/Users/tester/projects/agents-router".to_string()),
            project_name: Some("agents-router".to_string()),
            project_path: Some("/Users/tester/projects/agents-router".to_string()),
            branch: Some("main".to_string()),
            worktree: None,
        });
        signal.conversation = Some(SignalConversation {
            session_id: Some("session-1".to_string()),
            session_title: Some("agents-router sync report".to_string()),
            turn_id: Some("turn-1".to_string()),
            prompt: Some("Please fix the route.".to_string()),
            answer: Some(SignalAnswer {
                kind: SignalAnswerKind::Preview,
                content: "Fixed the route.".to_string(),
            }),
            model: Some("gpt-5.2-codex".to_string()),
        });
        signal.lifecycle = Some(SignalLifecycle {
            status: Some(SignalLifecycleStatus::Completed),
            started_at: None,
            completed_at: Some(timestamp()),
            duration_ms: Some(92_185),
        });
        signal.links = vec![SignalLink {
            label: "Open in Codex".to_string(),
            url: "codex://threads/session-1".to_string(),
        }];

        assert_eq!(
            format_signal_body(&signal, "2026-05-10 10:31:43 +08:00"),
            "Project: agents-router\nProject Path: /Users/tester/projects/agents-router\nSession: agents-router sync report\nModel: gpt-5.2-codex\nOpen in Codex: codex://threads/session-1\nDuration: 1m 32s\nBranch: main\nTime: 2026-05-10 10:31:43 +08:00\nSession ID: session-1\n\nPrompt: Please fix the route.\n\nPreview: Fixed the route."
        );
    }

    #[test]
    fn formats_session_id_with_explicit_label_when_session_title_is_absent() {
        let mut signal = Signal::new_with_timestamp(
            "signal-1",
            "claude_code",
            "claude_code",
            "Claude Code",
            "Claude Code finished a task.",
            timestamp(),
            BTreeMap::new(),
        );
        signal.conversation = Some(SignalConversation {
            session_id: Some("session-1".to_string()),
            session_title: None,
            turn_id: None,
            prompt: None,
            answer: None,
            model: Some("claude-sonnet-4-6".to_string()),
        });

        assert_eq!(
            format_signal_body(&signal, "2026-05-10 10:31:43 +08:00"),
            "Model: claude-sonnet-4-6\nTime: 2026-05-10 10:31:43 +08:00\nSession ID: session-1"
        );
    }

    fn timestamp() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
}
