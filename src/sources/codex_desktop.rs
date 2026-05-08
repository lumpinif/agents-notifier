use std::collections::BTreeMap;
use std::process::Stdio;
use std::sync::OnceLock;

use anyhow::Context;
use regex::Regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::config::{Config, SourceConfig};
use crate::router::{Provider, Router};
use crate::signal::Signal;

pub const LOG_PREDICATE: &str = r#"process == "usernoted" AND eventMessage CONTAINS[c] "com.openai.codex" AND eventMessage CONTAINS[c] "Delivering <NotificationRecord""#;

pub async fn watch(
    config: &Config,
    source: &SourceConfig,
    providers: &[&dyn Provider],
) -> anyhow::Result<()> {
    let mut child = Command::new("/usr/bin/log")
        .args(["stream", "--style", "compact", "--predicate", LOG_PREDICATE])
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .spawn()
        .context("failed to start macOS unified log stream")?;

    let stdout = child
        .stdout
        .take()
        .context("failed to capture macOS unified log stream stdout")?;
    let mut lines = BufReader::new(stdout).lines();

    info!(
        source.id = %source.id,
        source.type = %source.source_type.as_str(),
        event = "watch.started",
    );

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line.context("failed to read macOS unified log stream line")? else {
                    break;
                };

                let Some(signal) = signal_from_log_line(source, &line) else {
                    debug!(
                        source.id = %source.id,
                        source.type = %source.source_type.as_str(),
                        event = "watch.line.ignored",
                    );
                    continue;
                };

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
            signal = tokio::signal::ctrl_c() => {
                signal.context("failed to listen for shutdown signal")?;
                info!(event = "shutdown.requested");
                let _ = child.kill().await;
                info!(event = "shutdown.completed");
                return Ok(());
            }
        }
    }

    Ok(())
}

pub fn signal_from_log_line(source: &SourceConfig, line: &str) -> Option<Signal> {
    if !line.contains("usernoted")
        || !line.contains("Delivering <NotificationRecord")
        || !line.contains("com.openai.codex")
    {
        return None;
    }

    let mut metadata = BTreeMap::new();
    if let Some(raw_notification_id) = raw_notification_id(line) {
        metadata.insert("raw_notification_id".to_string(), raw_notification_id);
    }

    Some(Signal::new(
        source.id.clone(),
        source.source_type.as_str(),
        "Codex",
        "Codex sent a notification.",
        metadata,
    ))
}

fn raw_notification_id(line: &str) -> Option<String> {
    static IDENT_RE: OnceLock<Regex> = OnceLock::new();
    let regex = IDENT_RE.get_or_init(|| Regex::new(r#"ident:"([^"]+)""#).unwrap());

    regex
        .captures(line)
        .and_then(|captures| captures.get(1))
        .map(|matched| matched.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use crate::config::SourceType;

    use super::*;

    #[test]
    fn creates_signal_from_codex_notification_log_line() {
        let line = include_str!("../../tests/fixtures/codex_desktop_notification.log");
        let source = source_config();

        let signal = signal_from_log_line(&source, line).expect("Codex notification should match");

        assert_eq!(signal.source_id, "codex_desktop");
        assert_eq!(signal.source_type, "codex_desktop");
        assert_eq!(signal.title, "Codex");
        assert_eq!(signal.body, "Codex sent a notification.");
        assert_eq!(
            signal.metadata.get("raw_notification_id"),
            Some(&"ABCD-1234".to_string())
        );
    }

    #[test]
    fn ignores_non_codex_log_line() {
        let source = source_config();

        let signal = signal_from_log_line(
            &source,
            r#"2026-05-08 usernoted Delivering <NotificationRecord app:"com.example.other">"#,
        );

        assert!(signal.is_none());
    }

    fn source_config() -> SourceConfig {
        SourceConfig {
            id: "codex_desktop".to_string(),
            source_type: SourceType::CodexDesktop,
        }
    }
}
