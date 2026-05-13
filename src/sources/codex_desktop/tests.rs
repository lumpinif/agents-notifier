use std::fs::File;
use std::io::Write;

use tempfile::tempdir;

use crate::config::{AnswerDetail, PromptDetail, SourceType};

use super::*;

#[test]
fn first_run_skips_existing_rollout_events_then_emits_new_events() {
    let dir = tempdir().expect("tempdir should be created");
    let sessions_dir = dir.path().join("sessions");
    let day_dir = sessions_dir.join("2026").join("05").join("10");
    fs::create_dir_all(&day_dir).expect("session dir should be created");
    let rollout_path = day_dir.join("rollout-2026-05-10T01-00-00-session-1.jsonl");
    let index_path = dir.path().join("session_index.jsonl");
    let state_path = dir.path().join("source-state.json");

    write_lines(
        &rollout_path,
        &[
            session_meta_line("session-1"),
            task_complete_line("turn-old", "2026-05-09T17:00:00.000Z", "old"),
        ],
    );
    write_lines(
            &index_path,
            &[r#"{"id":"session-1","thread_name":"agents-router sync report","updated_at":"2026-05-09T17:00:00Z"}"#.to_string()],
        );

    let mut watcher = CodexDesktopSessionWatcher::new(
        sessions_dir.clone(),
        index_path.clone(),
        state_path.clone(),
    )
    .expect("watcher should start");
    let first = watcher
        .poll(&source_config(), AnswerDetail::Preview, PromptDetail::Off)
        .expect("first poll should pass");
    assert!(first.is_empty());

    append_line(&rollout_path, &turn_context_line("gpt-5.5"));
    append_line(
        &rollout_path,
        &task_complete_line("turn-new", "2026-05-09T17:01:32.000Z", "new result"),
    );
    let second = watcher
        .poll(&source_config(), AnswerDetail::Preview, PromptDetail::Off)
        .expect("second poll should pass");

    assert_eq!(second.len(), 1);
    assert_eq!(
        second[0]
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.turn_id.as_deref()),
        Some("turn-new")
    );
    assert_eq!(
        second[0]
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.model.as_deref()),
        Some("gpt-5.5")
    );
    assert_eq!(
        second[0]
            .conversation
            .as_ref()
            .and_then(|conversation| conversation.answer.as_ref())
            .map(|answer| answer.content.as_str()),
        Some("new result")
    );
}

#[test]
fn prompt_detail_on_attaches_prompt_without_persisting_it() {
    let dir = tempdir().expect("tempdir should be created");
    let sessions_dir = dir.path().join("sessions");
    let day_dir = sessions_dir.join("2026").join("05").join("10");
    fs::create_dir_all(&day_dir).expect("session dir should be created");
    let rollout_path = day_dir.join("rollout-2026-05-10T01-00-00-session-1.jsonl");
    let index_path = dir.path().join("session_index.jsonl");
    let state_path = dir.path().join("source-state.json");

    write_lines(&rollout_path, &[session_meta_line("session-1")]);
    write_lines(
            &index_path,
            &[r#"{"id":"session-1","thread_name":"agents-router sync report","updated_at":"2026-05-09T17:00:00Z"}"#.to_string()],
        );

    let mut watcher = CodexDesktopSessionWatcher::new(
        sessions_dir.clone(),
        index_path.clone(),
        state_path.clone(),
    )
    .expect("watcher should start");
    watcher
        .poll(&source_config(), AnswerDetail::Full, PromptDetail::On)
        .expect("first poll should pass");

    append_line(&rollout_path, &user_message_line("Please fix the route."));
    append_line(
        &rollout_path,
        &task_complete_line("turn-new", "2026-05-09T17:01:32.000Z", "fixed"),
    );
    let signals = watcher
        .poll(&source_config(), AnswerDetail::Full, PromptDetail::On)
        .expect("second poll should pass");

    assert_eq!(signals.len(), 1);
    let conversation = signals[0]
        .conversation
        .as_ref()
        .expect("conversation should be set");
    assert_eq!(
        conversation.prompt.as_deref(),
        Some("Please fix the route.")
    );
    assert_eq!(
        conversation
            .answer
            .as_ref()
            .map(|answer| answer.content.as_str()),
        Some("fixed")
    );

    let state = fs::read_to_string(&state_path).expect("state file should exist");
    assert!(!state.contains("Please fix the route."));
}

fn source_config() -> SourceConfig {
    SourceConfig {
        id: "codex_desktop".to_string(),
        source_type: SourceType::CodexDesktop,
    }
}

fn session_meta_line(session_id: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-05-09T16:59:00.000Z","type":"session_meta","payload":{{"id":"{session_id}","timestamp":"2026-05-09T16:59:00.000Z","cwd":"/Users/tester/projects/agents-router","originator":"Codex Desktop","cli_version":"0.130.0-alpha.5","source":"vscode","model_provider":"openai","git":{{"branch":"main","commit_hash":"abc123","repository_url":"https://example.com/repo.git"}}}}}}"#
    )
}

fn task_complete_line(turn_id: &str, timestamp: &str, last_agent_message: &str) -> String {
    format!(
        r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"task_complete","turn_id":"{turn_id}","completed_at":1778348142,"duration_ms":92185,"time_to_first_token_ms":1200,"last_agent_message":"{last_agent_message}"}}}}"#
    )
}

fn turn_context_line(model: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-05-09T17:01:20.000Z","type":"turn_context","payload":{{"model":"{model}","collaboration_mode":{{"settings":{{"model":"{model}"}}}}}}}}"#
    )
}

fn user_message_line(message: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-05-09T17:01:00.000Z","type":"event_msg","payload":{{"type":"user_message","message":"{message}","images":[]}}}}"#
    )
}

fn write_lines(path: &Path, lines: &[String]) {
    let mut file = File::create(path).expect("file should be created");
    for line in lines {
        writeln!(file, "{line}").expect("line should be written");
    }
}

fn append_line(path: &Path, line: &str) {
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("file should open");
    writeln!(file, "{line}").expect("line should be appended");
}
