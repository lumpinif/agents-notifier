#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tempfile::{TempDir, tempdir_in};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::process::Command;

#[tokio::test]
async fn emit_submits_event_to_local_service_socket() {
    let home = short_home();
    let socket_path = socket_path_for_home(home.path());
    create_parent(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("test socket should bind");

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("client should connect");
        let mut raw_request = Vec::new();
        stream
            .read_to_end(&mut raw_request)
            .await
            .expect("request should be readable");
        stream
            .write_all(br#"{"ok":true,"error":null}"#)
            .await
            .expect("response should be writable");
        serde_json::from_slice::<Value>(&raw_request).expect("request should be JSON")
    });

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .args([
            "emit",
            "--source",
            "codex_cli",
            "--title",
            "Codex",
            "--body",
            "Ready for review.",
        ])
        .output()
        .await
        .expect("command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let request = server.await.expect("server task should finish");
    assert_eq!(request["source_id"], "codex_cli");
    assert_eq!(request["title"], "Codex");
    assert_eq!(request["body"], "Ready for review.");
}

#[tokio::test]
async fn emit_fails_when_local_service_rejects_event() {
    let home = short_home();
    let socket_path = socket_path_for_home(home.path());
    create_parent(&socket_path);
    let listener = UnixListener::bind(&socket_path).expect("test socket should bind");

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("client should connect");
        let mut raw_request = Vec::new();
        stream
            .read_to_end(&mut raw_request)
            .await
            .expect("request should be readable");
        stream
            .write_all(br#"{"ok":false,"error":"source `codex_cli` is not configured"}"#)
            .await
            .expect("response should be writable");
    });

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .args([
            "emit",
            "--source",
            "codex_cli",
            "--title",
            "Codex",
            "--body",
            "Ready for review.",
        ])
        .output()
        .await
        .expect("command should run");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("agents-notifier service rejected local ingress event")
    );

    server.await.expect("server task should finish");
}

#[tokio::test]
async fn emit_fails_when_local_service_is_not_running() {
    let home = short_home();

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .args([
            "emit",
            "--source",
            "codex_cli",
            "--title",
            "Codex",
            "--body",
            "Ready for review.",
        ])
        .output()
        .await
        .expect("command should run");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("run `agents-notifier start` first"));
}

fn socket_path_for_home(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return home
        .join("Library")
        .join("Application Support")
        .join("agents-notifier")
        .join("agents-notifier.sock");

    #[cfg(all(unix, not(target_os = "macos")))]
    home.join(".local")
        .join("state")
        .join("agents-notifier")
        .join("agents-notifier.sock")
}

fn create_parent(path: &Path) {
    fs::create_dir_all(path.parent().expect("path should have parent"))
        .expect("parent directory should be created");
}

fn short_home() -> TempDir {
    tempdir_in("/tmp").expect("short temp home should be created")
}
