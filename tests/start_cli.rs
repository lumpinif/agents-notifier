use std::path::Path;

use tempfile::{TempDir, tempdir_in};
use tokio::process::Command;

#[tokio::test]
async fn start_fails_with_actionable_message_when_default_config_is_missing() {
    let home = short_home();

    let output = command_with_home(home.path())
        .arg("start")
        .output()
        .await
        .expect("command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("No agents-router config found"));
    assert!(stderr.contains("Run `agents-router setup` in an interactive terminal"));
    assert!(stderr.contains("--config <PATH>"));
}

#[tokio::test]
async fn watch_background_is_not_supported() {
    let home = short_home();

    let output = command_with_home(home.path())
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

    let output = command_with_home(home.path())
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

    let output = command_with_home(home.path())
        .arg("configure")
        .output()
        .await
        .expect("command should run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("configure"));
}

#[tokio::test]
async fn version_command_prints_package_version() {
    let home = short_home();

    let output = command_with_home(home.path())
        .arg("version")
        .output()
        .await
        .expect("command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert_eq!(
        stdout.trim(),
        concat!("agents-router ", env!("CARGO_PKG_VERSION"))
    );
}

#[tokio::test]
async fn version_flag_prints_package_version() {
    let home = short_home();

    let output = command_with_home(home.path())
        .arg("--version")
        .output()
        .await
        .expect("command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert_eq!(
        stdout.trim(),
        concat!("agents-router ", env!("CARGO_PKG_VERSION"))
    );
}

#[tokio::test]
async fn help_lists_uninstall_command() {
    let home = short_home();

    let output = command_with_home(home.path())
        .arg("--help")
        .output()
        .await
        .expect("command should run");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("uninstall"));
    assert!(stdout.contains("remove Agents Router local files"));
}

fn short_home() -> TempDir {
    tempdir_in(std::env::temp_dir()).expect("short temp home should be created")
}

fn command_with_home(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_agents-router"));
    command.env("HOME", home);
    command.env("USERPROFILE", home);
    command.env("APPDATA", home.join("AppData").join("Roaming"));
    command.env("LOCALAPPDATA", home.join("AppData").join("Local"));
    command
}
