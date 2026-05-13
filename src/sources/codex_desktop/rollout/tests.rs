use chrono::{DateTime, Utc};
use std::io::Write;
use tempfile::NamedTempFile;

use crate::config::{AnswerDetail, SourceConfig, SourceType};
use crate::signal::{SignalAnswerKind, SignalEventKind, SignalLifecycleStatus};

use super::*;

#[test]
fn creates_signal_from_task_complete_rollout_event() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("Codex Desktop".to_string()),
        source: Some("vscode".to_string()),
        cwd: Some("/Users/tester/projects/agents-router".to_string()),
        cli_version: Some("0.130.0-alpha.5".to_string()),
        model: Some("gpt-5.2-codex".to_string()),
        branch: Some("main".to_string()),
        ..SessionInfo::default()
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
        Some("agents-router sync report"),
        task,
        AnswerDetail::Preview,
        None,
    )
    .expect("Codex Desktop task should create a signal");

    assert_eq!(signal.source_id(), "codex_desktop");
    assert_eq!(signal.source_type(), "codex_desktop");
    assert_eq!(signal.event.kind, SignalEventKind::TurnCompleted);
    assert_eq!(signal.event.raw_name.as_deref(), Some("task_complete"));
    assert_eq!(signal.title(), "Codex Desktop");

    let workspace = signal.workspace.expect("workspace should be set");
    assert_eq!(workspace.project_name.as_deref(), Some("agents-router"));
    assert_eq!(
        workspace.project_path.as_deref(),
        Some("/Users/tester/projects/agents-router")
    );
    assert_eq!(workspace.branch.as_deref(), Some("main"));

    let conversation = signal.conversation.expect("conversation should be set");
    assert_eq!(conversation.session_id.as_deref(), Some("session-1"));
    assert_eq!(
        conversation.session_title.as_deref(),
        Some("agents-router sync report")
    );
    assert_eq!(conversation.turn_id.as_deref(), Some("turn-1"));
    assert_eq!(conversation.model.as_deref(), Some("gpt-5.2-codex"));
    let answer = conversation.answer.expect("answer should be set");
    assert_eq!(answer.kind, SignalAnswerKind::Preview);
    assert_eq!(
        answer.content,
        "Updated the Codex Desktop notification copy across the default notification, CLI examples, tests, and docs."
    );

    let lifecycle = signal.lifecycle.expect("lifecycle should be set");
    assert_eq!(lifecycle.status, Some(SignalLifecycleStatus::Completed));
    assert_eq!(lifecycle.duration_ms, Some(92_185));
    assert_eq!(
        lifecycle.completed_at,
        Some(
            DateTime::parse_from_rfc3339("2026-05-09T17:35:42Z")
                .unwrap()
                .with_timezone(&Utc)
        )
    );
    assert_eq!(signal.links.len(), 1);
    assert_eq!(signal.links[0].label, "Open in Codex");
    assert_eq!(signal.links[0].url, "codex://threads/session-1");
    assert!(signal.metadata.get("turn_id").is_none());
    assert!(signal.metadata.get("duration_ms").is_none());
}

#[test]
fn creates_signal_with_full_answer_detail() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("Codex Desktop".to_string()),
        cwd: Some("/Users/tester/projects/agents-router".to_string()),
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

    let answer = signal
        .conversation
        .and_then(|conversation| conversation.answer)
        .expect("answer should be set");
    assert_eq!(answer.kind, SignalAnswerKind::Full);
    assert!(answer.content.contains("Fixed the route."));
    assert!(answer.content.contains("Next, run the tests."));
}

#[test]
fn creates_signal_with_prompt_before_answer() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("Codex Desktop".to_string()),
        cwd: Some("/Users/tester/projects/agents-router".to_string()),
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

    let conversation = signal.conversation.expect("conversation should be set");
    assert_eq!(
        conversation.prompt.as_deref(),
        Some("Please fix the route.")
    );
    assert_eq!(
        conversation.answer.expect("answer should be set").content,
        "Fixed the route."
    );
}

#[test]
fn full_answer_detail_omits_codex_app_directives() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("Codex Desktop".to_string()),
        cwd: Some("/Users/tester/projects/agents-router".to_string()),
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

    let answer = signal
        .conversation
        .and_then(|conversation| conversation.answer)
        .expect("answer should be set");
    assert!(answer.content.contains("Implemented the fix."));
    assert!(!answer.content.contains("::git-stage"));
    assert!(!answer.content.contains("::git-commit"));
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
    let line = r#"{"timestamp":"2026-05-09T17:35:42.000Z","type":"session_meta","payload":{"id":"session-1","originator":"Codex Desktop","source":{"kind":"desktop"},"cwd":"/Users/tester/projects/agents-router","cli_version":"0.130.0-alpha.5","model":"gpt-5.2-codex","model_provider":"openai","git":{"branch":"main"}}}"#;

    let Some(RolloutItem::SessionMeta(session)) =
        parse_rollout_line(line, true).expect("line should parse")
    else {
        panic!("expected session_meta");
    };

    assert_eq!(session.id, Some("session-1".to_string()));
    assert_eq!(session.originator, Some("Codex Desktop".to_string()));
    assert_eq!(session.source, None);
    assert_eq!(session.model, Some("gpt-5.2-codex".to_string()));
    assert_eq!(session.model_provider, Some("openai".to_string()));
    assert_eq!(session.branch, Some("main".to_string()));
}

#[test]
fn parses_turn_context_model_without_reading_provider_as_model() {
    let line = r#"{"timestamp":"2026-05-12T17:35:42.000Z","type":"turn_context","payload":{"model":"gpt-5.5","model_provider":"openai","base_instructions":"private instructions stay ignored"}}"#;

    let Some(RolloutItem::TurnContext(session)) =
        parse_rollout_line(line, true).expect("line should parse")
    else {
        panic!("expected turn_context");
    };

    assert_eq!(session.model, Some("gpt-5.5".to_string()));
    assert_eq!(session.model_provider, None);
}

#[test]
fn parses_turn_context_model_from_collaboration_mode_settings() {
    let line = r#"{"timestamp":"2026-05-12T17:35:42.000Z","type":"turn_context","payload":{"collaboration_mode":{"settings":{"model":"gpt-5.4","reasoning_effort":"high"}}}}"#;

    let Some(RolloutItem::TurnContext(session)) =
        parse_rollout_line(line, true).expect("line should parse")
    else {
        panic!("expected turn_context");
    };

    assert_eq!(session.model, Some("gpt-5.4".to_string()));
}

#[test]
fn read_session_info_merges_turn_context_model() {
    let mut rollout = NamedTempFile::new().expect("tempfile should be created");
    writeln!(
        rollout,
        r#"{{"timestamp":"2026-05-09T17:35:42.000Z","type":"session_meta","payload":{{"id":"session-1","originator":"Codex Desktop","cwd":"/Users/tester/projects/agents-router","model_provider":"openai","git":{{"branch":"main"}}}}}}"#
    )
    .expect("session_meta should be written");
    writeln!(
        rollout,
        r#"{{"timestamp":"2026-05-09T17:35:42.500Z","type":"event_msg","payload":{{"type":"task_complete"}}}}"#
    )
    .expect("unneeded event should be written");
    writeln!(
        rollout,
        r#"{{"timestamp":"2026-05-09T17:35:43.000Z","type":"turn_context","payload":{{"model":"gpt-5.5"}}}}"#
    )
    .expect("turn_context should be written");

    let session = read_session_info(rollout.path()).expect("session info should read");

    assert_eq!(session.id, Some("session-1".to_string()));
    assert_eq!(session.model, Some("gpt-5.5".to_string()));
    assert_eq!(session.model_provider, Some("openai".to_string()));
    assert_eq!(session.branch, Some("main".to_string()));
}

#[test]
fn later_turn_context_updates_model_when_session_meta_has_no_model() {
    let mut session = SessionInfo::default();

    session.merge_turn_context(SessionInfo {
        model: Some("gpt-5.4".to_string()),
        model_source: Some(SessionModelSource::TurnContext),
        ..SessionInfo::default()
    });
    session.merge_turn_context(SessionInfo {
        model: Some("gpt-5.5".to_string()),
        model_source: Some(SessionModelSource::TurnContext),
        ..SessionInfo::default()
    });

    assert_eq!(session.model, Some("gpt-5.5".to_string()));
}

#[test]
fn session_meta_model_takes_precedence_over_turn_context_model() {
    let mut rollout = NamedTempFile::new().expect("tempfile should be created");
    writeln!(
        rollout,
        r#"{{"timestamp":"2026-05-09T17:35:42.000Z","type":"session_meta","payload":{{"id":"session-1","originator":"Codex Desktop","model":"gpt-5.2-codex"}}}}"#
    )
    .expect("session_meta should be written");
    writeln!(
        rollout,
        r#"{{"timestamp":"2026-05-09T17:35:43.000Z","type":"turn_context","payload":{{"model":"gpt-5.5"}}}}"#
    )
    .expect("turn_context should be written");

    let session = read_session_info(rollout.path()).expect("session info should read");

    assert_eq!(session.model, Some("gpt-5.2-codex".to_string()));
}

#[test]
fn model_provider_does_not_become_conversation_model() {
    let source = source_config();
    let session = SessionInfo {
        id: Some("session-1".to_string()),
        originator: Some("Codex Desktop".to_string()),
        model_provider: Some("openai".to_string()),
        ..SessionInfo::default()
    };
    let task = TaskComplete {
        turn_id: "turn-1".to_string(),
        timestamp: Utc::now(),
        completed_at: None,
        duration_ms: Some(1_000),
        last_agent_message: Some("done".to_string()),
    };

    let signal =
        signal_from_task_complete(&source, &session, None, task, AnswerDetail::Preview, None)
            .expect("Codex Desktop task should create a signal");
    let conversation = signal.conversation.expect("conversation should be set");

    assert_eq!(conversation.model, None);
    assert_eq!(
        signal.metadata.get("codex_model_provider"),
        Some(&"openai".to_string())
    );
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
