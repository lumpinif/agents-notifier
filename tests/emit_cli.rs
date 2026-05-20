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

    let output = Command::new(env!("CARGO_BIN_EXE_agents-router"))
        .env("HOME", home.path())
        .args([
            "emit",
            "--source",
            "codex_cli",
            "--title",
            "Codex",
            "--body",
            "Ready for review.",
            "--duration-ms",
            "420000",
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
    assert_eq!(request["lifecycle"]["status"], "completed");
    assert_eq!(request["lifecycle"]["duration_ms"], 420000);
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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

    let input = include_bytes!("fixtures/sources/codex_cli/stop.json");
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
    assert_eq!(request["workspace"]["project_name"], "agents-router");
    assert_eq!(
        request["workspace"]["project_path"],
        "/Users/tester/projects/agents-router"
    );
    assert_eq!(request["conversation"]["session_id"], "codex-session-1");
    assert_eq!(request["conversation"]["turn_id"], "turn-1");
    assert_eq!(request["conversation"]["answer"], "Ready for review.");
    assert_eq!(request["conversation"]["model"], "gpt-5.2-codex");
    assert_eq!(request["lifecycle"]["status"], "completed");
}

#[tokio::test]
async fn ingest_ignores_codex_desktop_stop_hook_when_desktop_source_is_enabled() {
    let home = short_home();
    write_codex_desktop_and_cli_config(home.path());
    write_codex_rollout_session_meta(home.path(), "desktop-session-1", "Codex Desktop");

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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

    let mut stdin = child.stdin.take().expect("stdin should be piped");
    stdin
        .write_all(codex_stop_payload("desktop-session-1").as_bytes())
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
}

#[tokio::test]
async fn ingest_uses_service_metadata_config_for_codex_desktop_stop_hook_noop() {
    let home = short_home();
    let config_path = home.path().join("custom").join("agents-router.toml");
    write_codex_desktop_and_cli_config_at(&config_path);
    write_service_metadata(home.path(), &config_path);
    write_codex_rollout_session_meta(home.path(), "desktop-session-1", "Codex Desktop");

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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

    let mut stdin = child.stdin.take().expect("stdin should be piped");
    stdin
        .write_all(codex_stop_payload("desktop-session-1").as_bytes())
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
}

#[tokio::test]
async fn ingest_does_not_ignore_codex_desktop_stop_hook_for_noncanonical_source_id() {
    let home = short_home();
    write_codex_desktop_and_cli_config(home.path());
    write_codex_rollout_session_meta(home.path(), "desktop-session-1", "Codex Desktop");

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
        .env("HOME", home.path())
        .args([
            "ingest",
            "--source",
            "my_codex",
            "--format",
            "codex_cli_stop",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("command should spawn");

    let mut stdin = child.stdin.take().expect("stdin should be piped");
    stdin
        .write_all(codex_stop_payload("desktop-session-1").as_bytes())
        .await
        .expect("hook input should be writable");
    drop(stdin);

    let output = child
        .wait_with_output()
        .await
        .expect("command should finish");

    assert!(
        !output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("agents-router service is not running"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn ingest_keeps_codex_cli_stop_hook_when_origin_is_unknown() {
    let home = short_home();
    write_codex_desktop_and_cli_config(home.path());
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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

    let mut stdin = child.stdin.take().expect("stdin should be piped");
    stdin
        .write_all(codex_stop_payload("unknown-session-1").as_bytes())
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
    assert_eq!(request["conversation"]["session_id"], "unknown-session-1");
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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

    let input = include_bytes!("fixtures/sources/claude_code/stop.json");
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
    assert_eq!(request["workspace"]["project_name"], "agents-router");
    assert_eq!(request["conversation"]["session_id"], "claude-session-1");
    assert_eq!(
        request["conversation"]["answer"],
        "Implemented the requested change."
    );
    assert_eq!(request["conversation"]["model"], Value::Null);
    assert_eq!(request["lifecycle"]["status"], "completed");
    assert_eq!(request["metadata"]["permission_mode"], "default");
}

#[tokio::test]
async fn ingest_submits_claude_code_session_start_context_to_local_service_socket() {
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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

    let input = include_bytes!("fixtures/sources/claude_code/session_start.json");
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
    assert_eq!(request["action"], "store_session_context");
    assert_eq!(request["event"]["raw_name"], "SessionStart");
    assert_eq!(request["workspace"]["project_name"], "agents-router");
    assert_eq!(request["conversation"]["session_id"], "claude-session-1");
    assert_eq!(request["conversation"]["model"], "claude-sonnet-4-6");
    assert_eq!(request["metadata"]["session_start_source"], "startup");
    assert_eq!(request["metadata"]["transcript_path"], Value::Null);
}

#[tokio::test]
async fn ingest_submits_claude_code_user_prompt_submit_context_to_local_service_socket() {
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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

    let input = include_bytes!("fixtures/sources/claude_code/user_prompt_submit.json");
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
    assert_eq!(request["action"], "store_turn_start");
    assert_eq!(request["event"]["raw_name"], "UserPromptSubmit");
    assert_eq!(request["workspace"]["project_name"], "agents-router");
    assert_eq!(request["conversation"]["session_id"], "claude-session-1");
    assert_eq!(request["conversation"]["prompt"], Value::Null);
    assert!(request["lifecycle"]["started_at"].is_string());
    assert_eq!(request["metadata"]["permission_mode"], "default");
    assert_eq!(request["metadata"]["transcript_path"], Value::Null);
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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

    let input = include_bytes!("fixtures/sources/gemini_cli/after_agent.json");
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
    assert_eq!(request["workspace"]["project_name"], "agents-router");
    assert_eq!(request["conversation"]["session_id"], "gemini-session-1");
    assert_eq!(request["conversation"]["prompt"], "Review this patch.");
    assert_eq!(request["conversation"]["answer"], "Ready for review.");
    assert_eq!(request["conversation"]["model"], Value::Null);
    assert_eq!(request["lifecycle"]["status"], "completed");
    assert_eq!(request["lifecycle"]["completed_at"], "2026-05-12T10:15:30Z");
}

#[tokio::test]
async fn ingest_submits_structured_github_copilot_cli_notification_event_to_local_service_socket() {
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
        .env("HOME", home.path())
        .args([
            "ingest",
            "--source",
            "github_copilot_cli",
            "--format",
            "github_copilot_cli_notification",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("command should spawn");

    let input = include_bytes!("fixtures/sources/github_copilot_cli/notification.json");
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
    assert_eq!(request["source_id"], "github_copilot_cli");
    assert_eq!(request["title"], "GitHub Copilot CLI");
    assert_eq!(
        request["body"],
        "GitHub Copilot CLI emitted a notification."
    );
    assert_eq!(request["event"]["kind"], "custom");
    assert_eq!(request["event"]["raw_name"], "Notification");
    assert_eq!(request["workspace"]["project_name"], "agents-router");
    assert_eq!(request["conversation"]["session_id"], "copilot-session-1");
    assert_eq!(request["metadata"]["notification_type"], "agent_idle");
    assert_eq!(request["metadata"]["timestamp_ms"], "1775907268684");
}

#[tokio::test]
async fn ingest_submits_structured_opencode_cli_session_event_to_local_service_socket() {
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
        .env("HOME", home.path())
        .args([
            "ingest",
            "--source",
            "opencode_cli",
            "--format",
            "opencode_cli_session",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("command should spawn");

    let input = br#"{
        "event": {
            "type": "session.status",
            "properties": {
                "sessionID": "session-1",
                "status": { "type": "idle" }
            }
        },
        "cwd": "/Users/tester/projects/agents-router",
        "worktree": "main",
        "timestamp": "2026-05-12T10:15:30Z",
        "model": "opencode-model"
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
    assert_eq!(request["source_id"], "opencode_cli");
    assert_eq!(request["title"], "OpenCode CLI");
    assert_eq!(request["body"], "OpenCode CLI finished a task.");
    assert_eq!(request["event"]["kind"], "turn_completed");
    assert_eq!(request["event"]["raw_name"], "session.status");
    assert_eq!(request["workspace"]["project_name"], "agents-router");
    assert_eq!(request["workspace"]["worktree"], "main");
    assert_eq!(request["conversation"]["session_id"], "session-1");
    assert_eq!(request["conversation"]["model"], "opencode-model");
    assert_eq!(request["lifecycle"]["status"], "completed");
    assert_eq!(request["metadata"]["status_type"], "idle");
}

#[tokio::test]
async fn ingest_submits_structured_generic_agent_hook_event_to_local_service_socket() {
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_agents-router"))
        .env("HOME", home.path())
        .args([
            "ingest",
            "--source",
            "cursor_cli",
            "--format",
            "agent_hook_event",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("command should spawn");

    let input = br#"{
        "display": {
            "title": "Cursor CLI",
            "summary": "Cursor CLI finished a task."
        },
        "event": {
            "kind": "turn_completed",
            "raw_name": "wrapper.completed"
        },
        "workspace": {
            "cwd": "/Users/tester/projects/agents-router",
            "project_name": "agents-router",
            "project_path": "/Users/tester/projects/agents-router"
        },
        "conversation": {
            "answer": "Ready for review.",
            "model": "cursor-model"
        },
        "lifecycle": {
            "status": "completed",
            "duration_ms": 1200
        }
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
    assert_eq!(request["source_id"], "cursor_cli");
    assert_eq!(request["title"], "Cursor CLI");
    assert_eq!(request["body"], "Cursor CLI finished a task.");
    assert_eq!(request["event"]["kind"], "turn_completed");
    assert_eq!(request["event"]["raw_name"], "wrapper.completed");
    assert_eq!(request["workspace"]["project_name"], "agents-router");
    assert_eq!(request["conversation"]["answer"], "Ready for review.");
    assert_eq!(request["conversation"]["model"], "cursor-model");
    assert_eq!(request["lifecycle"]["duration_ms"], 1200);
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

    let output = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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
            .contains("agents-router service rejected local ingress event")
    );

    server.await.expect("server task should finish");
}

#[tokio::test]
async fn emit_fails_when_local_service_is_not_running() {
    let home = short_home();

    let output = Command::new(env!("CARGO_BIN_EXE_agents-router"))
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
    assert!(String::from_utf8_lossy(&output.stderr).contains("run `agents-router start` first"));
}

#[tokio::test]
async fn safety_reset_clears_persisted_pause_when_service_is_not_running() {
    let home = short_home();
    let state_path = delivery_safety_state_path_for_home(home.path());
    create_parent(&state_path);
    fs::write(
        &state_path,
        r#"{
  "schema_version": 1,
  "global_pause": {
    "paused_at": "2026-05-14T00:00:00Z",
    "reason": "multiple_duplicate_storms",
    "message_fingerprint_count": 3,
    "window_seconds": 120
  }
}"#,
    )
    .expect("safety state should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_agents-router"))
        .env("HOME", home.path())
        .args(["safety", "reset"])
        .output()
        .await
        .expect("command should run");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let state: Value = serde_json::from_slice(
        &fs::read(&state_path).expect("safety state should still exist after reset"),
    )
    .expect("safety state should be JSON");
    assert_eq!(state["schema_version"], 1);
    assert_eq!(state["global_pause"], Value::Null);
}

fn socket_path_for_home(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return home
        .join("Library")
        .join("Application Support")
        .join("agents-router")
        .join("agents-router.sock");

    #[cfg(all(unix, not(target_os = "macos")))]
    home.join(".local")
        .join("state")
        .join("agents-router")
        .join("agents-router.sock")
}

fn config_path_for_home(home: &Path) -> PathBuf {
    home.join(".config")
        .join("agents-router")
        .join("config.toml")
}

fn codex_sessions_path_for_home(home: &Path) -> PathBuf {
    home.join(".codex").join("sessions")
}

fn service_metadata_path_for_home(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return home
        .join("Library")
        .join("Application Support")
        .join("agents-router")
        .join("service.json");

    #[cfg(all(unix, not(target_os = "macos")))]
    home.join(".local")
        .join("state")
        .join("agents-router")
        .join("service.json")
}

fn delivery_safety_state_path_for_home(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return home
        .join("Library")
        .join("Application Support")
        .join("agents-router")
        .join("delivery-safety-state.json");

    #[cfg(all(unix, not(target_os = "macos")))]
    home.join(".local")
        .join("state")
        .join("agents-router")
        .join("delivery-safety-state.json")
}

fn create_parent(path: &Path) {
    fs::create_dir_all(path.parent().expect("path should have parent"))
        .expect("parent directory should be created");
}

fn write_codex_desktop_and_cli_config(home: &Path) {
    let path = config_path_for_home(home);
    write_codex_desktop_and_cli_config_at(&path);
}

fn write_codex_desktop_and_cli_config_at(path: &Path) {
    create_parent(path);
    fs::write(
        path,
        r#"
schema_version = 1

[notification]
answer_detail = "preview"

[[sources]]
id = "codex_desktop"
type = "codex_desktop"

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "debug_webhook"
type = "webhook"
url = "http://127.0.0.1:9/agents-router-test"

[[routes]]
sources = ["codex_desktop", "codex_cli"]
providers = ["debug_webhook"]
"#,
    )
    .expect("config should be written");
}

fn write_service_metadata(home: &Path, config_path: &Path) {
    let path = service_metadata_path_for_home(home);
    create_parent(&path);
    fs::write(
        path,
        format!(
            r#"{{
  "binary_path": "/tmp/agents-router",
  "config_path": "{}",
  "log_file": "/tmp/agents-router.log",
  "installed_at": "2026-05-14T00:00:00Z"
}}"#,
            config_path.display()
        ),
    )
    .expect("service metadata should be written");
}

fn write_codex_rollout_session_meta(home: &Path, session_id: &str, originator: &str) {
    let path = codex_sessions_path_for_home(home)
        .join("2026")
        .join("05")
        .join("14")
        .join(format!("rollout-2026-05-14T00-00-00-{session_id}.jsonl"));
    create_parent(&path);
    fs::write(
        path,
        format!(
            r#"{{"timestamp":"2026-05-14T00:00:00.000Z","type":"session_meta","payload":{{"id":"{session_id}","originator":"{originator}","cwd":"/Users/tester/projects/agents-router","model":"gpt-5.2-codex"}}}}
"#
        ),
    )
    .expect("rollout should be written");
}

fn codex_stop_payload(session_id: &str) -> String {
    format!(
        r#"{{
  "cwd": "/Users/tester/projects/agents-router",
  "hook_event_name": "Stop",
  "last_assistant_message": "Ready for review.",
  "model": "gpt-5.2-codex",
  "session_id": "{session_id}",
  "turn_id": "turn-1"
}}"#
    )
}

fn short_home() -> TempDir {
    tempdir_in("/tmp").expect("short temp home should be created")
}
