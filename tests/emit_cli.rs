use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::tempdir;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn emit_sends_signal_to_configured_webhook() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .and(body_partial_json(serde_json::json!({
            "schema_version": 1,
            "source_id": "codex_cli",
            "source_type": "codex_cli",
            "title": "Codex",
            "body": "Codex sent a notification.",
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let config_path = write_config(&server.uri());

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .args([
            "emit",
            "--config",
            config_path.to_str().unwrap(),
            "--source",
            "codex_cli",
            "--title",
            "Codex",
            "--body",
            "Codex sent a notification.",
        ])
        .output()
        .expect("command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn emit_uses_default_config_path_when_config_is_omitted() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .and(body_partial_json(serde_json::json!({
            "schema_version": 1,
            "source_id": "codex_cli",
            "source_type": "codex_cli",
            "title": "Codex",
            "body": "Codex sent a notification.",
        })))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let home = tempdir().expect("tempdir should be created");
    let config_path = home
        .path()
        .join(".config")
        .join("agents-notifier")
        .join("config.toml");
    write_config_at(&config_path, &server.uri());

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .args([
            "emit",
            "--source",
            "codex_cli",
            "--title",
            "Codex",
            "--body",
            "Codex sent a notification.",
        ])
        .output()
        .expect("command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn emit_fails_for_missing_source() {
    let config_path = write_config("https://example.com");

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .args([
            "emit",
            "--config",
            config_path.to_str().unwrap(),
            "--source",
            "missing",
            "--title",
            "Codex",
            "--body",
            "Body",
        ])
        .output()
        .expect("command should run");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("source `missing` is not configured"));
}

fn write_config(webhook_server: &str) -> PathBuf {
    let dir = tempdir().expect("tempdir should be created").keep();
    let path = dir.join("config.toml");
    write_config_at(&path, webhook_server);
    path
}

fn write_config_at(path: &Path, webhook_server: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("config directory should be created");
    }

    fs::write(
        path,
        format!(
            r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "debug"
type = "webhook"
url = "{webhook_server}/hook"

[[routes]]
sources = ["codex_cli"]
providers = ["debug"]
"#
        ),
    )
    .expect("config should be written");
}
