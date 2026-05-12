use chrono::{DateTime, Utc};

use crate::config::{AnswerDetail, SourceConfig, SourceType};

use super::*;

#[test]
fn creates_signal_from_task_complete_rollout_event() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("Codex Desktop".to_string()),
        source: Some("vscode".to_string()),
        cwd: Some("/Users/tester/projects/agents-notifier".to_string()),
        cli_version: Some("0.130.0-alpha.5".to_string()),
        branch: Some("main".to_string()),
    };
    let task = TaskComplete {
            turn_id: "turn-1".to_string(),
            timestamp: DateTime::parse_from_rfc3339("2026-05-09T17:35:42Z")
                .unwrap()
                .with_timezone(&Utc),
            completed_at: Some(1_778_348_142),
            duration_ms: Some(92_185),
            last_agent_message: Some(
                "Updated the Codex Desktop notification copy across the default notification, CLI examples, tests, and docs.".to_string(),
            ),
        };

    let signal = signal_from_task_complete(
        &source,
        &session,
        Some("agents-notifier sync report"),
        task,
        AnswerDetail::Preview,
        None,
    )
    .expect("Codex Desktop task should create a signal");

    assert_eq!(signal.source_id, "codex_desktop");
    assert_eq!(signal.source_type, "codex_desktop");
    assert_eq!(signal.title, "Codex Desktop");
    assert_eq!(
        signal.body,
        "Project: agents-notifier\nProject Path: /Users/tester/projects/agents-notifier\nSession: agents-notifier sync report\nOpen in Codex: codex://threads/session-1\nDuration: 1m 32s\nBranch: main\n\nPreview: Updated the Codex Desktop notification copy across the default notification, CLI examples, tests, and docs."
    );
    assert_eq!(
        signal.metadata.get("session_id"),
        Some(&"session-1".to_string())
    );
    assert_eq!(signal.metadata.get("turn_id"), Some(&"turn-1".to_string()));
    assert_eq!(
        signal.metadata.get("duration_ms"),
        Some(&"92185".to_string())
    );
    assert_eq!(
        signal.metadata.get("project_path"),
        Some(&"/Users/tester/projects/agents-notifier".to_string())
    );
}

#[test]
fn creates_signal_with_full_answer_detail() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("Codex Desktop".to_string()),
        cwd: Some("/Users/tester/projects/agents-notifier".to_string()),
        branch: Some("main".to_string()),
        ..SessionInfo::default()
    };
    let task = TaskComplete {
        turn_id: "turn-1".to_string(),
        timestamp: Utc::now(),
        completed_at: None,
        duration_ms: Some(1_000),
        last_agent_message: Some("Fixed the route.\n\nNext, run the tests.".to_string()),
    };

    let signal = signal_from_task_complete(&source, &session, None, task, AnswerDetail::Full, None)
        .expect("Codex Desktop task should create a signal");

    assert!(signal.body.contains("Answer: Fixed the route."));
    assert!(signal.body.contains("Next, run the tests."));
    assert!(!signal.body.contains("Preview:"));
}

#[test]
fn creates_signal_with_prompt_before_answer() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("Codex Desktop".to_string()),
        cwd: Some("/Users/tester/projects/agents-notifier".to_string()),
        branch: Some("main".to_string()),
        ..SessionInfo::default()
    };
    let task = TaskComplete {
        turn_id: "turn-1".to_string(),
        timestamp: Utc::now(),
        completed_at: None,
        duration_ms: Some(1_000),
        last_agent_message: Some("Fixed the route.".to_string()),
    };

    let signal = signal_from_task_complete(
        &source,
        &session,
        None,
        task,
        AnswerDetail::Full,
        Some("Please fix the route."),
    )
    .expect("Codex Desktop task should create a signal");

    assert!(signal.body.contains("\n\nPrompt: Please fix the route."));
    assert!(signal.body.contains("\n\nAnswer: Fixed the route."));
    assert!(
        signal.body.find("Prompt: Please fix the route.")
            < signal.body.find("Answer: Fixed the route.")
    );
}

#[test]
fn full_answer_detail_omits_codex_app_directives() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("Codex Desktop".to_string()),
        cwd: Some("/Users/tester/projects/agents-notifier".to_string()),
        branch: Some("main".to_string()),
        ..SessionInfo::default()
    };
    let task = TaskComplete {
        turn_id: "turn-1".to_string(),
        timestamp: Utc::now(),
        completed_at: None,
        duration_ms: Some(1_000),
        last_agent_message: Some(
            "Implemented the fix.\n\n::git-stage{cwd=\"/repo\"}\n::git-commit{cwd=\"/repo\"}"
                .to_string(),
        ),
    };

    let signal = signal_from_task_complete(&source, &session, None, task, AnswerDetail::Full, None)
        .expect("Codex Desktop task should create a signal");

    assert!(signal.body.contains("Answer: Implemented the fix."));
    assert!(!signal.body.contains("::git-stage"));
    assert!(!signal.body.contains("::git-commit"));
}

#[test]
fn parses_user_message_without_logging_or_reading_extra_fields() {
    let line = r#"{"timestamp":"2026-05-09T17:35:42.000Z","type":"event_msg","payload":{"type":"user_message","message":"Please fix the route.","images":[]}}"#;

    let Some(RolloutItem::UserMessage(message)) =
        parse_rollout_line(line, true).expect("line should parse")
    else {
        panic!("expected user_message");
    };

    assert_eq!(message, "Please fix the route.");
}

#[test]
fn ignores_user_message_when_prompt_detail_is_off() {
    let line = r#"{"timestamp":"2026-05-09T17:35:42.000Z","type":"event_msg","payload":{"type":"user_message","message":"Please fix the route.","images":[]}}"#;

    assert!(
        parse_rollout_line(line, false)
            .expect("line should parse")
            .is_none()
    );
}

#[test]
fn preview_detail_omits_codex_app_directives() {
    let block = answer_block(
        "Implemented the fix.\n\n::git-stage{cwd=\"/repo\"}",
        AnswerDetail::Preview,
    )
    .expect("answer block should be present");

    assert_eq!(block.content, "Implemented the fix.");
}

#[test]
fn directive_filter_preserves_directive_examples_inside_code_fences() {
    let answer = "Use this example:\n\n```text\n::git-stage{cwd=\"/repo\"}\n```\n\n::git-commit{cwd=\"/repo\"}";

    let filtered = strip_codex_app_directive_lines(answer);

    assert!(filtered.contains("::git-stage"));
    assert!(!filtered.contains("::git-commit"));
}

#[test]
fn ignores_non_desktop_codex_originator() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("codex_exec".to_string()),
        ..SessionInfo::default()
    };
    let task = TaskComplete {
        turn_id: "turn-1".to_string(),
        timestamp: Utc::now(),
        completed_at: None,
        duration_ms: Some(1_000),
        last_agent_message: Some("done".to_string()),
    };

    assert!(
        signal_from_task_complete(&source, &session, None, task, AnswerDetail::Preview, None)
            .is_none()
    );
}

#[test]
fn parses_rollout_line_without_reading_unneeded_content() {
    let line = r#"{"timestamp":"2026-05-09T17:35:42.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","completed_at":1778348142,"duration_ms":92185,"last_agent_message":"done"}}"#;

    let Some(RolloutItem::TaskComplete(task)) =
        parse_rollout_line(line, true).expect("line should parse")
    else {
        panic!("expected task_complete");
    };

    assert_eq!(task.turn_id, "turn-1");
    assert_eq!(task.duration_ms, Some(92_185));
    assert_eq!(
        task.timestamp,
        DateTime::parse_from_rfc3339("2026-05-09T17:35:42Z")
            .unwrap()
            .with_timezone(&Utc)
    );
}

#[test]
fn parses_session_metadata_with_structured_optional_fields() {
    let line = r#"{"timestamp":"2026-05-09T17:35:42.000Z","type":"session_meta","payload":{"id":"session-1","originator":"Codex Desktop","source":{"kind":"desktop"},"cwd":"/Users/tester/projects/agents-notifier","cli_version":"0.130.0-alpha.5","git":{"branch":"main"}}}"#;

    let Some(RolloutItem::SessionMeta(session)) =
        parse_rollout_line(line, true).expect("line should parse")
    else {
        panic!("expected session_meta");
    };

    assert_eq!(session.id, Some("session-1".to_string()));
    assert_eq!(session.originator, Some("Codex Desktop".to_string()));
    assert_eq!(session.source, None);
    assert_eq!(session.branch, Some("main".to_string()));
}

#[test]
fn truncates_preview_on_character_boundary() {
    assert_eq!(truncate_chars("abcdef", 3), "abc...");
    assert_eq!(preview_text("hello\n\nworld"), "hello world");
}

#[test]
fn limits_preview_to_useful_but_bounded_context() {
    let preview = preview_text(&"a".repeat(PREVIEW_LIMIT_CHARS + 1));

    assert_eq!(preview.chars().count(), PREVIEW_LIMIT_CHARS + 3);
    assert!(preview.ends_with("..."));
}

fn source_config() -> SourceConfig {
    SourceConfig {
        id: "codex_desktop".to_string(),
        source_type: SourceType::CodexDesktop,
    }
}
