use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use crate::config::SourceConfig;
use crate::signal::Signal;

const PREVIEW_LIMIT_CHARS: usize = 360;
const DEFAULT_TITLE: &str = "Codex Desktop finished a job.";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct SessionInfo {
    pub(super) id: Option<String>,
    originator: Option<String>,
    source: Option<String>,
    cwd: Option<String>,
    cli_version: Option<String>,
    branch: Option<String>,
}

impl SessionInfo {
    pub(super) fn merge(&mut self, next: Self) {
        if next.id.is_some() {
            self.id = next.id;
        }
        if next.originator.is_some() {
            self.originator = next.originator;
        }
        if next.source.is_some() {
            self.source = next.source;
        }
        if next.cwd.is_some() {
            self.cwd = next.cwd;
        }
        if next.cli_version.is_some() {
            self.cli_version = next.cli_version;
        }
        if next.branch.is_some() {
            self.branch = next.branch;
        }
    }

    fn is_codex_desktop(&self) -> bool {
        self.originator.as_deref() == Some("Codex Desktop")
    }

    fn project_name(&self) -> Option<String> {
        self.cwd
            .as_deref()
            .and_then(|cwd| Path::new(cwd).file_name())
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .map(ToOwned::to_owned)
    }
}

#[derive(Debug, Clone)]
pub(super) struct TaskComplete {
    pub(super) turn_id: String,
    timestamp: DateTime<Utc>,
    completed_at: Option<i64>,
    duration_ms: Option<i64>,
    last_agent_message: Option<String>,
}

pub(super) enum RolloutItem {
    SessionMeta(SessionInfo),
    TaskComplete(TaskComplete),
}

pub(super) struct RolloutReadResult {
    pub(super) items: Vec<RolloutItem>,
    pub(super) new_offset: u64,
    pub(super) offset_changed: bool,
}

pub(super) fn read_new_rollout_items(
    path: &Path,
    offset: u64,
) -> anyhow::Result<RolloutReadResult> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open rollout `{}`", path.display()))?;
    let file_len = file
        .metadata()
        .with_context(|| format!("failed to read rollout metadata `{}`", path.display()))?
        .len();
    let offset = if offset > file_len { 0 } else { offset };
    file.seek(SeekFrom::Start(offset))
        .with_context(|| format!("failed to seek rollout `{}`", path.display()))?;

    let mut reader = BufReader::new(file);
    let mut items = Vec::new();
    let mut position = offset;

    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .with_context(|| format!("failed to read rollout `{}`", path.display()))?;
        if bytes == 0 {
            break;
        }
        position += bytes as u64;

        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        match parse_rollout_line(line) {
            Ok(Some(item)) => items.push(item),
            Ok(None) => {}
            Err(error) => {
                warn!(
                    error = %error,
                    event = "watch.line.ignored",
                );
            }
        }
    }

    Ok(RolloutReadResult {
        items,
        new_offset: position,
        offset_changed: position != offset,
    })
}

pub(super) fn read_session_info(path: &Path) -> anyhow::Result<SessionInfo> {
    let file =
        File::open(path).with_context(|| format!("failed to open rollout `{}`", path.display()))?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.with_context(|| format!("failed to read rollout `{}`", path.display()))?;
        if let Some(RolloutItem::SessionMeta(session)) = parse_rollout_line(&line)? {
            return Ok(session);
        }
    }

    Ok(SessionInfo::default())
}

pub(super) fn signal_from_task_complete(
    source: &SourceConfig,
    session: &SessionInfo,
    session_title: Option<&str>,
    task: TaskComplete,
) -> Option<Signal> {
    if !session.is_codex_desktop() {
        return None;
    }

    let project = session.project_name();
    let preview = task
        .last_agent_message
        .as_deref()
        .map(preview_text)
        .filter(|preview| !preview.is_empty());
    let duration = task.duration_ms.map(format_duration);
    let branch = present_owned(session.branch.as_deref());
    let session_title = present_owned(session_title);

    let mut lines = Vec::new();
    if let Some(project) = &project {
        lines.push(format!("Project: {project}"));
    }
    if let Some(session_title) = &session_title {
        lines.push(format!("Session: {session_title}"));
    }
    if let Some(preview) = &preview {
        lines.push(format!("Preview: {preview}"));
    }
    if let Some(duration) = &duration {
        lines.push(format!("Duration: {duration}"));
    }
    if let Some(branch) = &branch {
        lines.push(format!("Branch: {branch}"));
    }

    let mut metadata = BTreeMap::new();
    if let Some(session_id) = &session.id {
        metadata.insert("session_id".to_string(), session_id.clone());
    }
    let turn_id = task.turn_id;
    metadata.insert("turn_id".to_string(), turn_id.clone());
    if let Some(project) = project {
        metadata.insert("project".to_string(), project);
    }
    if let Some(duration_ms) = task.duration_ms {
        metadata.insert("duration_ms".to_string(), duration_ms.to_string());
    }
    if let Some(completed_at) = task.completed_at {
        metadata.insert("completed_at".to_string(), completed_at.to_string());
    }
    if let Some(branch) = branch {
        metadata.insert("branch".to_string(), branch);
    }
    if let Some(originator) = &session.originator {
        metadata.insert("codex_originator".to_string(), originator.clone());
    }
    if let Some(source) = &session.source {
        metadata.insert("codex_source".to_string(), source.clone());
    }
    if let Some(cli_version) = &session.cli_version {
        metadata.insert("codex_cli_version".to_string(), cli_version.clone());
    }

    Some(Signal::new_with_timestamp(
        Uuid::new_v4().to_string(),
        source.id.clone(),
        source.source_type.as_str(),
        DEFAULT_TITLE,
        lines.join("\n"),
        task.timestamp,
        metadata,
    ))
}

pub(super) fn load_session_titles(path: &Path) -> anyhow::Result<HashMap<String, String>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let file = File::open(path)
        .with_context(|| format!("failed to open Codex session index `{}`", path.display()))?;
    let reader = BufReader::new(file);
    let mut titles = HashMap::new();

    for line in reader.lines() {
        let line = line
            .with_context(|| format!("failed to read Codex session index `{}`", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }

        let record: SessionIndexRecord = serde_json::from_str(&line)
            .with_context(|| format!("failed to parse Codex session index `{}`", path.display()))?;
        if !record.id.trim().is_empty() && !record.thread_name.trim().is_empty() {
            titles.insert(record.id, record.thread_name);
        }
    }

    Ok(titles)
}

pub(super) fn discover_rollout_paths(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    collect_rollout_paths(root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

pub(super) fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn parse_rollout_line(line: &str) -> anyhow::Result<Option<RolloutItem>> {
    let value: serde_json::Value =
        serde_json::from_str(line).context("failed to parse rollout JSON line")?;
    let record_type = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let payload = value.get("payload").cloned().unwrap_or_default();

    match record_type {
        "session_meta" => Ok(Some(RolloutItem::SessionMeta(session_info_from_payload(
            &payload,
        )))),
        "event_msg" => {
            let Some(payload_type) = payload.get("type").and_then(serde_json::Value::as_str) else {
                return Ok(None);
            };
            if payload_type != "task_complete" {
                return Ok(None);
            }

            Ok(Some(RolloutItem::TaskComplete(parse_task_complete(
                &value, &payload,
            )?)))
        }
        _ => Ok(None),
    }
}

fn parse_task_complete(
    record: &serde_json::Value,
    payload: &serde_json::Value,
) -> anyhow::Result<TaskComplete> {
    let turn_id = payload
        .get("turn_id")
        .and_then(serde_json::Value::as_str)
        .filter(|turn_id| !turn_id.trim().is_empty())
        .ok_or_else(|| anyhow!("task_complete missing turn_id"))?
        .to_string();
    let timestamp = record
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_rfc3339_timestamp)
        .or_else(|| {
            payload
                .get("completed_at")
                .and_then(serde_json::Value::as_i64)
                .and_then(timestamp_from_epoch_seconds)
        })
        .ok_or_else(|| anyhow!("task_complete missing timestamp"))?;

    Ok(TaskComplete {
        turn_id,
        timestamp,
        completed_at: payload
            .get("completed_at")
            .and_then(serde_json::Value::as_i64),
        duration_ms: payload
            .get("duration_ms")
            .and_then(serde_json::Value::as_i64),
        last_agent_message: payload
            .get("last_agent_message")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
    })
}

fn session_info_from_payload(payload: &serde_json::Value) -> SessionInfo {
    SessionInfo {
        id: string_field(payload, "id"),
        originator: string_field(payload, "originator"),
        source: string_field(payload, "source"),
        cwd: string_field(payload, "cwd"),
        cli_version: string_field(payload, "cli_version"),
        branch: payload
            .get("git")
            .and_then(|git| git.get("branch"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    }
}

fn string_field(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn collect_rollout_paths(path: &Path, paths: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in
        fs::read_dir(path).with_context(|| format!("failed to read `{}`", path.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in `{}`", path.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to read file type `{}`", path.display()))?;
        if file_type.is_dir() {
            collect_rollout_paths(&path, paths)?;
        } else if is_rollout_jsonl(&path) {
            paths.push(path);
        }
    }

    Ok(())
}

fn is_rollout_jsonl(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
}

fn parse_rfc3339_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn timestamp_from_epoch_seconds(value: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(value, 0).single()
}

fn preview_text(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&compact, PREVIEW_LIMIT_CHARS)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn format_duration(duration_ms: i64) -> String {
    let total_seconds = (duration_ms.max(0) / 1_000) as u64;
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

fn present_owned(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[derive(Debug, Deserialize)]
struct SessionIndexRecord {
    id: String,
    thread_name: String,
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use crate::config::{SourceConfig, SourceType};

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
                "Updated the Codex finished a job copy across the Codex Desktop default notification, CLI examples, tests, and docs.".to_string(),
            ),
        };

        let signal =
            signal_from_task_complete(&source, &session, Some("agents-notifier sync report"), task)
                .expect("Codex Desktop task should create a signal");

        assert_eq!(signal.source_id, "codex_desktop");
        assert_eq!(signal.source_type, "codex_desktop");
        assert_eq!(signal.title, "Codex Desktop finished a job.");
        assert_eq!(
            signal.body,
            "Project: agents-notifier\nSession: agents-notifier sync report\nPreview: Updated the Codex finished a job copy across the Codex Desktop default notification, CLI examples, tests, and docs.\nDuration: 1m 32s\nBranch: main"
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

        assert!(signal_from_task_complete(&source, &session, None, task).is_none());
    }

    #[test]
    fn parses_rollout_line_without_reading_unneeded_content() {
        let line = r#"{"timestamp":"2026-05-09T17:35:42.000Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","completed_at":1778348142,"duration_ms":92185,"last_agent_message":"done"}}"#;

        let Some(RolloutItem::TaskComplete(task)) =
            parse_rollout_line(line).expect("line should parse")
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
            parse_rollout_line(line).expect("line should parse")
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
}
