mod rollout;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tokio::time;
use tracing::{debug, info, warn};

use crate::config::{AnswerDetail, Config, SourceConfig};
use crate::paths::{
    codex_desktop_source_state_path, codex_session_index_path, codex_sessions_dir_path,
};
use crate::router::{Provider, Router};
use crate::signal::Signal;
use rollout::{
    RolloutItem, SessionInfo, discover_rollout_paths, load_session_titles, path_key,
    read_new_rollout_items, read_session_info, signal_from_task_complete,
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub async fn watch(
    config: &Config,
    source: &SourceConfig,
    providers: &[&dyn Provider],
) -> anyhow::Result<()> {
    let mut watcher = CodexDesktopSessionWatcher::new(
        codex_sessions_dir_path()?,
        codex_session_index_path()?,
        codex_desktop_source_state_path()?,
    )?;
    let mut interval = time::interval(POLL_INTERVAL);

    info!(
        source.id = %source.id,
        source.type = %source.source_type.as_str(),
        event = "watch.started",
    );

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let signals = watcher.poll(source, config.notification.answer_detail)?;
                for signal in signals {
                    info!(
                        signal.id = %signal.id,
                        source.id = %signal.source_id,
                        source.type = %signal.source_type,
                        event = "signal.created",
                    );

                    if let Err(error) = Router::new(config).route(&signal, providers).await {
                        warn!(
                            signal.id = %signal.id,
                            source.id = %signal.source_id,
                            source.type = %signal.source_type,
                            error = %error,
                            event = "provider.send.failed",
                        );
                    }
                }
            }
            signal = tokio::signal::ctrl_c() => {
                signal.context("failed to listen for shutdown signal")?;
                info!(event = "shutdown.requested");
                watcher.save_state()?;
                info!(event = "shutdown.completed");
                return Ok(());
            }
        }
    }
}

struct CodexDesktopSessionWatcher {
    sessions_dir: PathBuf,
    session_index_path: PathBuf,
    state_path: PathBuf,
    state: WatchState,
    bootstrap_existing_files: bool,
}

impl CodexDesktopSessionWatcher {
    fn new(
        sessions_dir: PathBuf,
        session_index_path: PathBuf,
        state_path: PathBuf,
    ) -> anyhow::Result<Self> {
        let state = WatchState::load(&state_path)?;
        let bootstrap_existing_files = !state_path.exists();
        let mut watcher = Self {
            sessions_dir,
            session_index_path,
            state_path,
            state,
            bootstrap_existing_files,
        };
        watcher.bootstrap()?;
        Ok(watcher)
    }

    fn poll(
        &mut self,
        source: &SourceConfig,
        answer_detail: AnswerDetail,
    ) -> anyhow::Result<Vec<Signal>> {
        let titles = load_session_titles(&self.session_index_path)?;
        let mut signals = Vec::new();
        let mut changed = false;

        for path in discover_rollout_paths(&self.sessions_dir)? {
            let path_key = path_key(&path);
            if !self.state.files.contains_key(&path_key) {
                let offset = if self.bootstrap_existing_files {
                    path.metadata()
                        .with_context(|| {
                            format!("failed to read rollout metadata `{}`", path.display())
                        })?
                        .len()
                } else {
                    0
                };
                let session = read_session_info(&path)?;
                self.state
                    .files
                    .insert(path_key.clone(), FileWatchState { offset, session });
                changed = true;
            }

            let mut result = read_new_rollout_items(
                &path,
                self.state
                    .files
                    .get(&path_key)
                    .expect("file state should exist after insertion")
                    .offset,
            )?;

            if result.offset_changed {
                let file_state = self
                    .state
                    .files
                    .get_mut(&path_key)
                    .expect("file state should exist before update");
                file_state.offset = result.new_offset;
                changed = true;
            }

            for item in result.items.drain(..) {
                match item {
                    RolloutItem::SessionMeta(session) => {
                        self.state
                            .files
                            .get_mut(&path_key)
                            .expect("file state should exist before metadata update")
                            .session
                            .merge(session);
                        changed = true;
                    }
                    RolloutItem::TaskComplete(task) => {
                        let session = self
                            .state
                            .files
                            .get(&path_key)
                            .expect("file state should exist before task processing")
                            .session
                            .clone();
                        let Some(session_id) = session.id.as_deref() else {
                            debug!(
                                source.id = %source.id,
                                source.type = %source.source_type.as_str(),
                                event = "watch.line.ignored",
                            );
                            continue;
                        };
                        let delivery_key = format!("{}:{}", session_id, task.turn_id);
                        if self.state.delivered_turns.contains(&delivery_key) {
                            continue;
                        }

                        if let Some(signal) = signal_from_task_complete(
                            source,
                            &session,
                            titles.get(session_id).map(String::as_str),
                            task,
                            answer_detail,
                        ) {
                            self.state.delivered_turns.insert(delivery_key);
                            self.state.prune_delivered_turns();
                            signals.push(signal);
                            changed = true;
                        }
                    }
                }
            }
        }

        self.bootstrap_existing_files = false;
        if changed {
            self.save_state()?;
        }

        Ok(signals)
    }

    fn bootstrap(&mut self) -> anyhow::Result<()> {
        if !self.bootstrap_existing_files {
            for path in discover_rollout_paths(&self.sessions_dir)? {
                let path_key = path_key(&path);
                if let Some(file_state) = self.state.files.get_mut(&path_key) {
                    file_state.session = read_session_info(&path)?;
                }
            }
            return Ok(());
        }

        let mut changed = false;
        for path in discover_rollout_paths(&self.sessions_dir)? {
            let path_key = path_key(&path);
            if self.state.files.contains_key(&path_key) {
                continue;
            }

            let offset = path
                .metadata()
                .with_context(|| format!("failed to read rollout metadata `{}`", path.display()))?
                .len();
            self.state.files.insert(
                path_key,
                FileWatchState {
                    offset,
                    session: read_session_info(&path)?,
                },
            );
            changed = true;
        }

        if changed {
            self.save_state()?;
        }

        Ok(())
    }

    fn save_state(&self) -> anyhow::Result<()> {
        self.state.save(&self.state_path)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct WatchState {
    files: BTreeMap<String, FileWatchState>,
    delivered_turns: BTreeSet<String>,
}

impl WatchState {
    fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read Codex Desktop source state `{}`",
                path.display()
            )
        })?;
        serde_json::from_str(&raw).with_context(|| {
            format!(
                "failed to parse Codex Desktop source state `{}`",
                path.display()
            )
        })
    }

    fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create Codex Desktop source state directory `{}`",
                    parent.display()
                )
            })?;
        }

        let raw = serde_json::to_string_pretty(self)
            .context("failed to serialize Codex Desktop source state")?;
        fs::write(path, raw).with_context(|| {
            format!(
                "failed to write Codex Desktop source state `{}`",
                path.display()
            )
        })
    }

    fn prune_delivered_turns(&mut self) {
        const MAX_DELIVERED_TURNS: usize = 2_000;
        while self.delivered_turns.len() > MAX_DELIVERED_TURNS {
            let Some(first) = self.delivered_turns.iter().next().cloned() else {
                break;
            };
            self.delivered_turns.remove(&first);
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileWatchState {
    offset: u64,
    #[serde(default)]
    session: SessionInfo,
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use tempfile::tempdir;

    use crate::config::{AnswerDetail, SourceType};

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
            &[r#"{"id":"session-1","thread_name":"agents-notifier sync report","updated_at":"2026-05-09T17:00:00Z"}"#.to_string()],
        );

        let mut watcher = CodexDesktopSessionWatcher::new(
            sessions_dir.clone(),
            index_path.clone(),
            state_path.clone(),
        )
        .expect("watcher should start");
        let first = watcher
            .poll(&source_config(), AnswerDetail::Preview)
            .expect("first poll should pass");
        assert!(first.is_empty());

        append_line(
            &rollout_path,
            &task_complete_line("turn-new", "2026-05-09T17:01:32.000Z", "new result"),
        );
        let second = watcher
            .poll(&source_config(), AnswerDetail::Preview)
            .expect("second poll should pass");

        assert_eq!(second.len(), 1);
        assert_eq!(
            second[0].metadata.get("turn_id"),
            Some(&"turn-new".to_string())
        );
        assert!(second[0].body.contains("Preview: new result"));
    }

    fn source_config() -> SourceConfig {
        SourceConfig {
            id: "codex_desktop".to_string(),
            source_type: SourceType::CodexDesktop,
        }
    }

    fn session_meta_line(session_id: &str) -> String {
        format!(
            r#"{{"timestamp":"2026-05-09T16:59:00.000Z","type":"session_meta","payload":{{"id":"{session_id}","timestamp":"2026-05-09T16:59:00.000Z","cwd":"/Users/tester/projects/agents-notifier","originator":"Codex Desktop","cli_version":"0.130.0-alpha.5","source":"vscode","model_provider":"openai","git":{{"branch":"main","commit_hash":"abc123","repository_url":"https://example.com/repo.git"}}}}}}"#
        )
    }

    fn task_complete_line(turn_id: &str, timestamp: &str, last_agent_message: &str) -> String {
        format!(
            r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"task_complete","turn_id":"{turn_id}","completed_at":1778348142,"duration_ms":92185,"time_to_first_token_ms":1200,"last_agent_message":"{last_agent_message}"}}}}"#
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
}
