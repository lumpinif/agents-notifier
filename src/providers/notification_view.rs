use crate::signal::{Signal, SignalAnswerKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SignalNotificationView {
    pub(super) title: String,
    pub(super) summary: Option<String>,
    pub(super) fields: Vec<NotificationField>,
    pub(super) actions: Vec<NotificationAction>,
    pub(super) sections: Vec<NotificationSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NotificationField {
    pub(super) key: NotificationFieldKey,
    pub(super) label: &'static str,
    pub(super) value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NotificationFieldKey {
    Project,
    ProjectPath,
    Session,
    Model,
    Duration,
    Branch,
    Time,
    SessionId,
}

impl NotificationFieldKey {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::ProjectPath => "Project Path",
            Self::Session => "Session",
            Self::Model => "Model",
            Self::Duration => "Duration",
            Self::Branch => "Branch",
            Self::Time => "Time",
            Self::SessionId => "Session ID",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NotificationSection {
    Prompt(String),
    Preview(String),
    Answer(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NotificationAction {
    pub(super) label: String,
    pub(super) url: String,
    pub(super) role: NotificationActionRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NotificationActionRole {
    Primary,
    Secondary,
}

impl SignalNotificationView {
    pub(super) fn from_signal(signal: &Signal, formatted_time: &str) -> Self {
        let mut fields = Vec::new();
        let has_structured_details = has_structured_details(signal);

        if let Some(workspace) = &signal.workspace {
            push_field(
                &mut fields,
                NotificationFieldKey::Project,
                workspace.project_name.as_deref(),
            );
            push_field(
                &mut fields,
                NotificationFieldKey::ProjectPath,
                workspace.project_path.as_deref(),
            );
        }

        if let Some(conversation) = &signal.conversation {
            push_field(
                &mut fields,
                NotificationFieldKey::Session,
                conversation.session_title.as_deref(),
            );
            push_field(
                &mut fields,
                NotificationFieldKey::Model,
                conversation.model.as_deref(),
            );
        }

        if let Some(lifecycle) = &signal.lifecycle
            && let Some(duration_ms) = lifecycle.duration_ms
        {
            fields.push(NotificationField {
                key: NotificationFieldKey::Duration,
                label: NotificationFieldKey::Duration.label(),
                value: format_duration_ms(duration_ms),
            });
        }

        if let Some(workspace) = &signal.workspace {
            push_field(
                &mut fields,
                NotificationFieldKey::Branch,
                workspace.branch.as_deref(),
            );
        }

        fields.push(NotificationField {
            key: NotificationFieldKey::Time,
            label: NotificationFieldKey::Time.label(),
            value: formatted_time.to_string(),
        });

        if let Some(conversation) = &signal.conversation {
            push_field(
                &mut fields,
                NotificationFieldKey::SessionId,
                conversation.session_id.as_deref(),
            );
        }

        let mut actions = Vec::new();
        for link in &signal.links {
            let Some(label) = present(Some(link.label.as_str())) else {
                continue;
            };
            let Some(url) = present(Some(link.url.as_str())) else {
                continue;
            };
            let role = if actions.is_empty() {
                NotificationActionRole::Primary
            } else {
                NotificationActionRole::Secondary
            };
            actions.push(NotificationAction {
                label: label.to_string(),
                url: url.to_string(),
                role,
            });
        }

        let mut sections = Vec::new();
        if let Some(conversation) = &signal.conversation {
            if let Some(prompt) = present(conversation.prompt.as_deref()) {
                sections.push(NotificationSection::Prompt(prompt.to_string()));
            }
            if let Some(answer) = conversation
                .answer
                .as_ref()
                .filter(|answer| !answer.content.trim().is_empty())
            {
                sections.push(notification_answer_section(
                    answer.kind,
                    answer.content.trim().to_string(),
                ));
            }
        }

        Self {
            title: signal.title().to_string(),
            summary: (!has_structured_details)
                .then(|| signal.summary().to_string())
                .filter(|summary| !summary.trim().is_empty()),
            fields,
            actions,
            sections,
        }
    }

    pub(super) fn field_value(&self, key: NotificationFieldKey) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.value.as_str())
    }
}

pub(super) fn format_duration_ms(duration_ms: u64) -> String {
    let total_seconds = duration_ms / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn notification_answer_section(kind: SignalAnswerKind, content: String) -> NotificationSection {
    match kind {
        SignalAnswerKind::Preview => NotificationSection::Preview(content),
        SignalAnswerKind::Full => NotificationSection::Answer(content),
    }
}

fn has_structured_details(signal: &Signal) -> bool {
    signal.workspace.is_some()
        || signal.conversation.as_ref().is_some_and(|conversation| {
            conversation.session_title.is_some()
                || conversation.session_id.is_some()
                || conversation.prompt.is_some()
                || conversation.answer.is_some()
                || conversation.model.is_some()
        })
        || !signal.links.is_empty()
}

fn push_field(fields: &mut Vec<NotificationField>, key: NotificationFieldKey, value: Option<&str>) {
    if let Some(value) = present(value) {
        fields.push(NotificationField {
            key,
            label: key.label(),
            value: value.to_string(),
        });
    }
}

fn present(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};

    use super::*;
    use crate::signal::{
        SignalAnswer, SignalConversation, SignalLifecycle, SignalLifecycleStatus, SignalLink,
        SignalWorkspace,
    };

    #[test]
    fn builds_minimal_signal_view() {
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
            SignalNotificationView::from_signal(&signal, "2026-05-10 10:31:43 +08:00"),
            SignalNotificationView {
                title: "Codex CLI".to_string(),
                summary: Some("Ready for review.".to_string()),
                fields: vec![NotificationField {
                    key: NotificationFieldKey::Time,
                    label: "Time",
                    value: "2026-05-10 10:31:43 +08:00".to_string(),
                }],
                actions: Vec::new(),
                sections: Vec::new(),
            }
        );
    }

    #[test]
    fn keeps_summary_when_simple_signal_only_adds_duration() {
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

        let view = SignalNotificationView::from_signal(&signal, "2026-05-10 10:31:43 +08:00");

        assert_eq!(view.summary.as_deref(), Some("Aider finished a task."));
        assert_eq!(
            view.fields,
            vec![
                NotificationField {
                    key: NotificationFieldKey::Duration,
                    label: "Duration",
                    value: "7m 0s".to_string(),
                },
                NotificationField {
                    key: NotificationFieldKey::Time,
                    label: "Time",
                    value: "2026-05-10 10:31:43 +08:00".to_string(),
                },
            ]
        );
    }

    #[test]
    fn builds_structured_codex_signal_view_in_stable_order() {
        let view =
            SignalNotificationView::from_signal(&structured_signal(), "2026-05-10 10:31:43 +08:00");

        assert_eq!(
            view.fields,
            vec![
                field(NotificationFieldKey::Project, "Project", "agents-router"),
                field(
                    NotificationFieldKey::ProjectPath,
                    "Project Path",
                    "/Users/tester/projects/agents-router"
                ),
                field(
                    NotificationFieldKey::Session,
                    "Session",
                    "agents-router sync report"
                ),
                field(NotificationFieldKey::Model, "Model", "gpt-5.2-codex"),
                field(NotificationFieldKey::Duration, "Duration", "1m 32s"),
                field(NotificationFieldKey::Branch, "Branch", "main"),
                field(
                    NotificationFieldKey::Time,
                    "Time",
                    "2026-05-10 10:31:43 +08:00"
                ),
                field(NotificationFieldKey::SessionId, "Session ID", "session-1"),
            ]
        );
        assert_eq!(view.summary, None);
        assert_eq!(
            view.actions,
            vec![
                NotificationAction {
                    label: "Open in Codex".to_string(),
                    url: "codex://threads/session-1".to_string(),
                    role: NotificationActionRole::Primary,
                },
                NotificationAction {
                    label: "Logs".to_string(),
                    url: "https://example.com/logs".to_string(),
                    role: NotificationActionRole::Secondary,
                },
            ]
        );
        assert_eq!(
            view.sections,
            vec![
                NotificationSection::Prompt("Please fix the route.".to_string()),
                NotificationSection::Preview("Fixed the route.".to_string()),
            ]
        );
    }

    #[test]
    fn trims_empty_fields_and_section_content() {
        let mut signal = structured_signal();
        let workspace = signal.workspace.as_mut().expect("workspace should exist");
        workspace.project_name = Some("  ".to_string());
        workspace.project_path = Some(" /tmp/project ".to_string());
        let conversation = signal
            .conversation
            .as_mut()
            .expect("conversation should exist");
        conversation.prompt = Some("  Fix it.  ".to_string());
        conversation.answer = Some(SignalAnswer {
            kind: SignalAnswerKind::Full,
            content: "  Done.  ".to_string(),
        });

        let view = SignalNotificationView::from_signal(&signal, "2026-05-10 10:31:43 +08:00");

        assert_eq!(
            view.field_value(NotificationFieldKey::ProjectPath),
            Some("/tmp/project")
        );
        assert_eq!(view.field_value(NotificationFieldKey::Project), None);
        assert_eq!(
            view.sections,
            vec![
                NotificationSection::Prompt("Fix it.".to_string()),
                NotificationSection::Answer("Done.".to_string()),
            ]
        );
    }

    fn structured_signal() -> Signal {
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
        signal.links = vec![
            SignalLink {
                label: "Open in Codex".to_string(),
                url: "codex://threads/session-1".to_string(),
            },
            SignalLink {
                label: "Logs".to_string(),
                url: "https://example.com/logs".to_string(),
            },
        ];
        signal
    }

    fn field(key: NotificationFieldKey, label: &'static str, value: &str) -> NotificationField {
        NotificationField {
            key,
            label,
            value: value.to_string(),
        }
    }

    fn timestamp() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
}
