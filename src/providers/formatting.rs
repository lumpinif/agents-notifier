use crate::signal::{Signal, SignalAnswer, SignalLink};

pub(super) fn format_signal_message(signal: &Signal, formatted_time: &str) -> String {
    format!(
        "{}\n\n{}",
        signal.title(),
        format_signal_body(signal, formatted_time)
    )
}

pub(super) fn format_signal_body(signal: &Signal, formatted_time: &str) -> String {
    let mut details = Vec::new();
    let has_structured_details = has_structured_details(signal);

    if !has_structured_details {
        push_present(&mut details, signal.summary().to_string());
    }

    if let Some(workspace) = &signal.workspace {
        push_labeled(&mut details, "Project", workspace.project_name.as_deref());
        push_labeled(
            &mut details,
            "Project Path",
            workspace.project_path.as_deref(),
        );
    }

    if let Some(conversation) = &signal.conversation {
        push_labeled(
            &mut details,
            "Session",
            conversation.session_title.as_deref(),
        );
        push_labeled(&mut details, "Model", conversation.model.as_deref());
    }

    for link in &signal.links {
        push_link(&mut details, link);
    }

    if let Some(lifecycle) = &signal.lifecycle {
        if let Some(duration_ms) = lifecycle.duration_ms {
            details.push(format!("Duration: {}", format_duration_ms(duration_ms)));
        }
    }

    if let Some(workspace) = &signal.workspace {
        push_labeled(&mut details, "Branch", workspace.branch.as_deref());
    }

    details.push(format!("Time: {formatted_time}"));

    let mut sections = Vec::new();
    sections.push(details.join("\n"));

    if let Some(conversation) = &signal.conversation {
        if let Some(prompt) = present(conversation.prompt.as_deref()) {
            sections.push(format!("Prompt: {prompt}"));
        }
        if let Some(answer) = present_answer(conversation.answer.as_ref()) {
            sections.push(format!(
                "{}: {}",
                answer.kind.display_label(),
                answer.content.trim()
            ));
        }
    }

    sections.join("\n\n")
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

fn has_structured_details(signal: &Signal) -> bool {
    signal.workspace.is_some()
        || signal.conversation.as_ref().is_some_and(|conversation| {
            conversation.session_title.is_some()
                || conversation.prompt.is_some()
                || conversation.answer.is_some()
                || conversation.model.is_some()
        })
        || signal.lifecycle.is_some()
        || !signal.links.is_empty()
}

fn push_link(lines: &mut Vec<String>, link: &SignalLink) {
    let label = link.label.trim();
    let url = link.url.trim();
    if !label.is_empty() && !url.is_empty() {
        lines.push(format!("{label}: {url}"));
    }
}

fn push_labeled(lines: &mut Vec<String>, label: &str, value: Option<&str>) {
    if let Some(value) = present(value) {
        lines.push(format!("{label}: {value}"));
    }
}

fn push_present(lines: &mut Vec<String>, value: String) {
    if !value.trim().is_empty() {
        lines.push(value);
    }
}

fn present(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn present_answer(answer: Option<&SignalAnswer>) -> Option<&SignalAnswer> {
    answer.filter(|answer| !answer.content.trim().is_empty())
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
            cwd: Some("/Users/tester/projects/agents-notifier".to_string()),
            project_name: Some("agents-notifier".to_string()),
            project_path: Some("/Users/tester/projects/agents-notifier".to_string()),
            branch: Some("main".to_string()),
            worktree: None,
        });
        signal.conversation = Some(SignalConversation {
            session_id: Some("session-1".to_string()),
            session_title: Some("agents-notifier sync report".to_string()),
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
            "Project: agents-notifier\nProject Path: /Users/tester/projects/agents-notifier\nSession: agents-notifier sync report\nModel: gpt-5.2-codex\nOpen in Codex: codex://threads/session-1\nDuration: 1m 32s\nBranch: main\nTime: 2026-05-10 10:31:43 +08:00\n\nPrompt: Please fix the route.\n\nPreview: Fixed the route."
        );
    }

    fn timestamp() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
}
