#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

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
async fn ingest_submits_structured_codex_cli_stop_event_to_local_service_socket() {
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .args([
            "ingest",
            "--source",
            "codex_cli",
            "--format",
            "codex_cli_stop",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("command should spawn");

    let input = br#"{
        "cwd": "/Users/tester/projects/agents-notifier",
        "hook_event_name": "Stop",
        "last_assistant_message": "Ready for review.",
        "model": "gpt-5.2-codex",
        "session_id": "session-1",
        "turn_id": "turn-1"
    }"#;
    let mut stdin = child.stdin.take().expect("stdin should be piped");
    stdin
        .write_all(input)
        .await
        .expect("hook input should be writable");
    drop(stdin);

    let output = child
        .wait_with_output()
        .await
        .expect("command should finish");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let request = server.await.expect("server task should finish");
    assert_eq!(request["source_id"], "codex_cli");
    assert_eq!(request["title"], "Codex CLI");
    assert_eq!(request["body"], "Codex CLI finished a task.");
    assert_eq!(request["event"]["kind"], "turn_completed");
    assert_eq!(request["event"]["raw_name"], "Stop");
    assert_eq!(request["workspace"]["project_name"], "agents-notifier");
    assert_eq!(
        request["workspace"]["project_path"],
        "/Users/tester/projects/agents-notifier"
    );
    assert_eq!(request["conversation"]["session_id"], "session-1");
    assert_eq!(request["conversation"]["turn_id"], "turn-1");
    assert_eq!(request["conversation"]["answer"], "Ready for review.");
    assert_eq!(request["conversation"]["model"], "gpt-5.2-codex");
    assert_eq!(request["lifecycle"]["status"], "completed");
}

#[tokio::test]
async fn ingest_submits_structured_claude_code_stop_event_to_local_service_socket() {
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .args([
            "ingest",
            "--source",
            "claude_code",
            "--format",
            "claude_code_hook",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("command should spawn");

    let input = br#"{
        "session_id": "session-1",
        "transcript_path": "/Users/tester/.claude/projects/project/session-1.jsonl",
        "cwd": "/Users/tester/projects/agents-notifier",
        "hook_event_name": "Stop",
        "permission_mode": "default",
        "last_assistant_message": "Ready for review.",
        "model": "claude-sonnet-4-6",
        "stop_hook_active": false
    }"#;
    let mut stdin = child.stdin.take().expect("stdin should be piped");
    stdin
        .write_all(input)
        .await
        .expect("hook input should be writable");
    drop(stdin);

    let output = child
        .wait_with_output()
        .await
        .expect("command should finish");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let request = server.await.expect("server task should finish");
    assert_eq!(request["source_id"], "claude_code");
    assert_eq!(request["title"], "Claude Code");
    assert_eq!(request["body"], "Claude Code finished a task.");
    assert_eq!(request["event"]["kind"], "turn_completed");
    assert_eq!(request["event"]["raw_name"], "Stop");
    assert_eq!(request["workspace"]["project_name"], "agents-notifier");
    assert_eq!(request["conversation"]["session_id"], "session-1");
    assert_eq!(request["conversation"]["answer"], "Ready for review.");
    assert_eq!(request["conversation"]["model"], "claude-sonnet-4-6");
    assert_eq!(request["lifecycle"]["status"], "completed");
    assert_eq!(request["metadata"]["permission_mode"], "default");
}

#[tokio::test]
async fn ingest_submits_structured_gemini_cli_after_agent_event_to_local_service_socket() {
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .args([
            "ingest",
            "--source",
            "gemini_cli",
            "--format",
            "gemini_cli_hook",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("command should spawn");

    let input = br#"{
        "session_id": "session-1",
        "transcript_path": "/Users/tester/.gemini/tmp/session-1.json",
        "cwd": "/Users/tester/projects/agents-notifier",
        "hook_event_name": "AfterAgent",
        "timestamp": "2026-05-12T10:15:30Z",
        "prompt": "Review this patch.",
        "prompt_response": "Ready for review.",
        "stop_hook_active": false,
        "model": "gemini-3-pro"
    }"#;
    let mut stdin = child.stdin.take().expect("stdin should be piped");
    stdin
        .write_all(input)
        .await
        .expect("hook input should be writable");
    drop(stdin);

    let output = child
        .wait_with_output()
        .await
        .expect("command should finish");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let request = server.await.expect("server task should finish");
    assert_eq!(request["source_id"], "gemini_cli");
    assert_eq!(request["title"], "Gemini CLI");
    assert_eq!(request["body"], "Gemini CLI finished a task.");
    assert_eq!(request["event"]["kind"], "turn_completed");
    assert_eq!(request["event"]["raw_name"], "AfterAgent");
    assert_eq!(request["workspace"]["project_name"], "agents-notifier");
    assert_eq!(request["conversation"]["session_id"], "session-1");
    assert_eq!(request["conversation"]["prompt"], "Review this patch.");
    assert_eq!(request["conversation"]["answer"], "Ready for review.");
    assert_eq!(request["conversation"]["model"], "gemini-3-pro");
    assert_eq!(request["lifecycle"]["status"], "completed");
    assert_eq!(request["lifecycle"]["completed_at"], "2026-05-12T10:15:30Z");
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
