use tempfile::{TempDir, tempdir_in};
use tokio::process::Command;

#[tokio::test]
async fn start_fails_with_actionable_message_when_default_config_is_missing() {
    let home = short_home();

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .arg("start")
        .output()
        .await
        .expect("command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("No agents-notifier config found"));
    assert!(stderr.contains("Run `agents-notifier setup` in an interactive terminal"));
    assert!(stderr.contains("--config <PATH>"));
}

#[tokio::test]
async fn watch_background_is_not_supported() {
    let home = short_home();

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .args(["watch", "--background"])
        .output()
        .await
        .expect("command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("unexpected argument '--background'"));
}

#[tokio::test]
async fn setup_requires_interactive_terminal() {
    let home = short_home();

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .arg("setup")
        .output()
        .await
        .expect("command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("must be run in an interactive terminal"));
    assert!(stderr.contains("choose an agent and a provider"));
}

#[tokio::test]
async fn configure_is_not_supported() {
    let home = short_home();

    let output = Command::new(env!("CARGO_BIN_EXE_agents-notifier"))
        .env("HOME", home.path())
        .arg("configure")
        .output()
        .await
        .expect("command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("configure"));
}

fn short_home() -> TempDir {
    tempdir_in("/tmp").expect("short temp home should be created")
}
