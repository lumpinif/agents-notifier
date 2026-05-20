use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::SourceConfig;
use crate::signal::{
    SignalEvent, SignalEventKind, SignalLifecycle, SignalLifecycleStatus, SignalLink,
    SignalWorkspace,
};
use crate::signal_builder::{SignalConversationDraft, SignalDraft};

const DEFAULT_TITLE: &str = "Codex Desktop";
const DEFAULT_SUMMARY: &str = "Codex Desktop finished a task.";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct SessionInfo {
    pub(super) id: Option<String>,
    originator: Option<String>,
    source: Option<String>,
    cwd: Option<String>,
    cli_version: Option<String>,
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model_source: Option<SessionModelSource>,
    model_provider: Option<String>,
    branch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SessionModelSource {
    SessionMeta,
    TurnContext,
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
        if next.model.is_some() {
            self.model = next.model;
            self.model_source = next.model_source;
        }
        if next.model_provider.is_some() {
            self.model_provider = next.model_provider;
        }
        if next.branch.is_some() {
            self.branch = next.branch;
        }
    }

    pub(super) fn merge_turn_context(&mut self, next: Self) {
        if next.model.is_some() && self.model_source != Some(SessionModelSource::SessionMeta) {
            self.model = next.model;
            self.model_source = next.model_source;
        }
    }

    pub(super) fn is_codex_desktop(&self) -> bool {
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

    fn project_path(&self) -> Option<String> {
        present_owned(self.cwd.as_deref())
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
    TurnContext(SessionInfo),
    UserMessage(String),
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
    include_user_messages: bool,
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
        if !line.ends_with('\n') {
            break;
        }
        position += bytes as u64;

        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        match parse_rollout_line(line, include_user_messages) {
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

pub(super) fn completed_rollout_offset(path: &Path) -> anyhow::Result<u64> {
    let file =
        File::open(path).with_context(|| format!("failed to open rollout `{}`", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut position = 0;

    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .with_context(|| format!("failed to read rollout `{}`", path.display()))?;
        if bytes == 0 {
            break;
        }
        if !line.ends_with('\n') {
            break;
        }
        position += bytes as u64;
    }

    Ok(position)
}

pub(super) fn read_session_info(path: &Path) -> anyhow::Result<SessionInfo> {
    let file =
        File::open(path).with_context(|| format!("failed to open rollout `{}`", path.display()))?;
    let reader = BufReader::new(file);

    let mut session = SessionInfo::default();
    for line in reader.lines() {
        let line = line.with_context(|| format!("failed to read rollout `{}`", path.display()))?;
        match parse_rollout_session_context_line(&line) {
            Some(RolloutItem::SessionMeta(next)) => session.merge(next),
            Some(RolloutItem::TurnContext(next)) => session.merge_turn_context(next),
            _ => {}
        }

        if initial_session_info_is_complete(&session) {
            break;
        }
    }

    Ok(session)
}

pub(super) fn draft_from_task_complete(
    source: &SourceConfig,
    session: &SessionInfo,
    session_title: Option<&str>,
    task: TaskComplete,
    prompt: Option<&str>,
) -> Option<SignalDraft> {
    if !session.is_codex_desktop() {
        return None;
    }

    let project = session.project_name();
    let answer = task
        .last_agent_message
        .as_deref()
        .and_then(answer_candidate);
    let prompt = prompt.and_then(prompt_block).map(ToOwned::to_owned);
    let branch = present_owned(session.branch.as_deref());
    let session_title = present_owned(session_title);
    let thread_link = session.id.as_deref().and_then(codex_thread_deep_link);
    let cwd = present_owned(session.cwd.as_deref());
    let project_path = session.project_path();

    let mut metadata = BTreeMap::new();
    let turn_id = task.turn_id;
    if let Some(originator) = &session.originator {
        metadata.insert("codex_originator".to_string(), originator.clone());
    }
    if let Some(source) = &session.source {
        metadata.insert("codex_source".to_string(), source.clone());
    }
    if let Some(cli_version) = &session.cli_version {
        metadata.insert("codex_cli_version".to_string(), cli_version.clone());
    }
    if let Some(model_provider) = &session.model_provider {
        metadata.insert("codex_model_provider".to_string(), model_provider.clone());
    }

    let mut draft = SignalDraft::new(
        source.id.clone(),
        source.source_type,
        DEFAULT_TITLE,
        DEFAULT_SUMMARY,
    );

    draft.event = SignalEvent {
        kind: SignalEventKind::TurnCompleted,
        raw_name: Some("task_complete".to_string()),
    };
    draft.timestamp = Some(task.timestamp);
    draft.metadata = metadata;
    draft.workspace = workspace_if_present(SignalWorkspace {
        cwd,
        project_name: project,
        project_path,
        branch,
        worktree: None,
    });
    draft.conversation = Some(SignalConversationDraft {
        session_id: session.id.clone(),
        session_title,
        turn_id: Some(turn_id),
        prompt,
        answer,
        model: present_owned(session.model.as_deref()),
    });
    draft.lifecycle = Some(SignalLifecycle {
        status: Some(SignalLifecycleStatus::Completed),
        started_at: None,
        completed_at: task.completed_at.and_then(timestamp_from_epoch_seconds),
        duration_ms: task.duration_ms.map(|duration_ms| duration_ms as u64),
    });
    if let Some(thread_link) = thread_link {
        draft.links.push(SignalLink {
            label: "Open in Codex".to_string(),
            url: thread_link,
        });
    }

    Some(draft)
}

fn workspace_if_present(workspace: SignalWorkspace) -> Option<SignalWorkspace> {
    if workspace.cwd.is_some()
        || workspace.project_name.is_some()
        || workspace.project_path.is_some()
        || workspace.branch.is_some()
        || workspace.worktree.is_some()
    {
        Some(workspace)
    } else {
        None
    }
}

fn answer_candidate(value: &str) -> Option<String> {
    let value = strip_codex_app_directive_lines(value);
    present_owned(Some(&value))
}

fn prompt_block(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() { None } else { Some(value) }
}

fn strip_codex_app_directive_lines(value: &str) -> String {
    let mut in_fenced_code = false;
    let mut lines = Vec::new();

    for line in value.lines() {
        let trimmed = line.trim_start();
        if is_markdown_fence(trimmed) {
            in_fenced_code = !in_fenced_code;
            lines.push(line);
            continue;
        }

        if !in_fenced_code && is_codex_app_directive_line(line) {
            continue;
        }

        lines.push(line);
    }

    lines.join("\n")
}

fn is_markdown_fence(trimmed_line: &str) -> bool {
    trimmed_line.starts_with("```") || trimmed_line.starts_with("~~~")
}

fn is_codex_app_directive_line(line: &str) -> bool {
    let line = line.trim();
    if !line.starts_with("::") || !line.ends_with('}') {
        return false;
    }

    let Some(open_brace) = line.find('{') else {
        return false;
    };

    matches!(
        &line[2..open_brace],
        "archive"
            | "code-comment"
            | "git-commit"
            | "git-create-branch"
            | "git-create-pr"
            | "git-push"
            | "git-stage"
    )
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

        let record: SessionIndexRecord = match serde_json::from_str(&line) {
            Ok(record) => record,
            Err(error) => {
                warn!(
                    error = %error,
                    index.path = %path.display(),
                    event = "session_index.line.ignored",
                );
                continue;
            }
        };
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

fn parse_rollout_line(
    line: &str,
    include_user_messages: bool,
) -> anyhow::Result<Option<RolloutItem>> {
    if !include_user_messages && rollout_line_is_user_message(line).unwrap_or(false) {
        return Ok(None);
    }

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
        "turn_context" => {
            Ok(turn_context_info_from_payload(&payload).map(RolloutItem::TurnContext))
        }
        "event_msg" => {
            let Some(payload_type) = payload.get("type").and_then(serde_json::Value::as_str) else {
                return Ok(None);
            };

            if include_user_messages && payload_type == "user_message" {
                return Ok(parse_user_message(&payload).map(RolloutItem::UserMessage));
            }

            if payload_type == "task_complete" {
                return Ok(Some(RolloutItem::TaskComplete(parse_task_complete(
                    &value, &payload,
                )?)));
            }

            Ok(None)
        }
        _ => Ok(None),
    }
}

fn rollout_line_is_user_message(line: &str) -> anyhow::Result<bool> {
    let envelope: RolloutLineKind =
        serde_json::from_str(line).context("failed to parse rollout JSON line")?;
    Ok(envelope.record_type.as_deref() == Some("event_msg")
        && envelope.payload.payload_type.as_deref() == Some("user_message"))
}

#[derive(Debug, Deserialize)]
struct RolloutLineKind {
    #[serde(rename = "type")]
    record_type: Option<String>,
    #[serde(default)]
    payload: RolloutPayloadKind,
}

#[derive(Debug, Default, Deserialize)]
struct RolloutPayloadKind {
    #[serde(rename = "type")]
    payload_type: Option<String>,
}

fn parse_rollout_session_context_line(line: &str) -> Option<RolloutItem> {
    let value: serde_json::Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            warn!(
                error = %error,
                event = "watch.line.ignored",
            );
            return None;
        }
    };
    let record_type = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let payload = value.get("payload").cloned().unwrap_or_default();

    match record_type {
        "session_meta" => Some(RolloutItem::SessionMeta(session_info_from_payload(
            &payload,
        ))),
        "turn_context" => turn_context_info_from_payload(&payload).map(RolloutItem::TurnContext),
        _ => None,
    }
}

fn initial_session_info_is_complete(session: &SessionInfo) -> bool {
    session.id.is_some() && (!session.is_codex_desktop() || session.model.is_some())
}

fn parse_user_message(payload: &serde_json::Value) -> Option<String> {
    payload
        .get("message")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(ToOwned::to_owned)
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

    let duration_ms = payload
        .get("duration_ms")
        .and_then(serde_json::Value::as_i64);
    if duration_ms.is_some_and(|duration_ms| duration_ms < 0) {
        return Err(anyhow!("task_complete duration_ms is negative"));
    }

    Ok(TaskComplete {
        turn_id,
        timestamp,
        completed_at: payload
            .get("completed_at")
            .and_then(serde_json::Value::as_i64),
        duration_ms,
        last_agent_message: payload
            .get("last_agent_message")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
    })
}

fn session_info_from_payload(payload: &serde_json::Value) -> SessionInfo {
    let model = string_field(payload, "model");
    let model_source = model.as_ref().map(|_| SessionModelSource::SessionMeta);

    SessionInfo {
        id: string_field(payload, "id"),
        originator: string_field(payload, "originator"),
        source: string_field(payload, "source"),
        cwd: string_field(payload, "cwd"),
        cli_version: string_field(payload, "cli_version"),
        model,
        model_source,
        model_provider: string_field(payload, "model_provider"),
        branch: payload
            .get("git")
            .and_then(|git| git.get("branch"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    }
}

fn turn_context_info_from_payload(payload: &serde_json::Value) -> Option<SessionInfo> {
    turn_context_model(payload).map(|model| SessionInfo {
        model: Some(model),
        model_source: Some(SessionModelSource::TurnContext),
        ..SessionInfo::default()
    })
}

fn turn_context_model(payload: &serde_json::Value) -> Option<String> {
    string_field(payload, "model").or_else(|| {
        payload
            .get("collaboration_mode")
            .and_then(|collaboration_mode| collaboration_mode.get("settings"))
            .and_then(|settings| string_field(settings, "model"))
    })
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

fn present_owned(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn codex_thread_deep_link(session_id: &str) -> Option<String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return None;
    }

    Some(format!("codex://threads/{session_id}"))
}

#[derive(Debug, Deserialize)]
struct SessionIndexRecord {
    id: String,
    thread_name: String,
}

#[cfg(test)]
mod tests;
