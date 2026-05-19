use tempfile::tempdir;

use crate::config::{
    CliConfig, LogConfig, NotificationConfig, ProviderType, RawConfig, RawProviderConfig,
    RouteConfig, SourceConfig, SourceType, ValidatedConfig,
};
use crate::local_integrations::{LocalSourceIntegrationPaths, codex_cli_stop_hook_command};
use crate::setup;

use super::*;

#[test]
fn reload_replaces_current_runtime_snapshot() {
    let dir = tempdir().expect("tempdir should be created");
    let path = dir.path().join("config.toml");
    setup::write_config(&path, &raw_test_config(None)).expect("config should be written");
    let runtime = RuntimeState::new(test_config(None)).expect("runtime should be created");

    setup::write_config(&path, &raw_test_config(Some(5)))
        .expect("updated config should be written");
    runtime
        .reload_from_path(&path)
        .expect("valid config should reload");

    assert_eq!(
        runtime
            .current()
            .expect("runtime snapshot should be readable")
            .config
            .routes[0]
            .minimum_task_duration_minutes,
        Some(5)
    );
}

#[test]
fn failed_reload_keeps_previous_runtime_snapshot() {
    let dir = tempdir().expect("tempdir should be created");
    let path = dir.path().join("config.toml");
    setup::write_config(&path, &raw_test_config(Some(5))).expect("config should be written");
    let runtime = RuntimeState::new(test_config(Some(5))).expect("runtime should be created");

    std::fs::write(
        &path,
        r#"
schema_version = 1

[[sources]]
id = "codex_cli"
type = "codex_cli"

[[providers]]
id = "phone"
type = "ntfy"
server = "https://ntfy.sh"
topic = "agents-router-test"

[[routes]]
sources = ["codex_cli"]
providers = ["phone"]
minimum_task_duration_minutes = 0
"#,
    )
    .expect("invalid config should be written");

    runtime
        .reload_from_path(&path)
        .expect_err("invalid config should not reload");

    assert_eq!(
        runtime
            .current()
            .expect("runtime snapshot should be readable")
            .config
            .routes[0]
            .minimum_task_duration_minutes,
        Some(5)
    );
}

#[test]
fn reload_with_local_integrations_applies_before_replacing_snapshot() {
    let dir = tempdir().expect("tempdir should be created");
    let path = dir.path().join("config.toml");
    let codex_config_path = dir.path().join("codex").join("config.toml");
    let claude_settings_path = dir.path().join("claude").join("settings.json");
    let integration_paths = LocalSourceIntegrationPaths {
        codex_cli_config_path: codex_config_path.clone(),
        claude_code_settings_path: claude_settings_path,
    };
    setup::write_config(&path, &raw_test_config(None)).expect("config should be written");
    let runtime = RuntimeState::new(test_config(Some(5))).expect("runtime should be created");

    let report = runtime
        .reload_from_path_with_local_integrations(&path, &integration_paths)
        .expect("valid config and local integrations should reload");

    assert!(report.has_changes());
    assert!(
        std::fs::read_to_string(codex_config_path)
            .expect("Codex CLI config should be written")
            .contains(codex_cli_stop_hook_command().as_str())
    );
    assert_eq!(
        runtime
            .current()
            .expect("runtime snapshot should be readable")
            .config
            .routes[0]
            .minimum_task_duration_minutes,
        None
    );
}

#[test]
fn failed_local_integration_keeps_previous_runtime_snapshot() {
    let dir = tempdir().expect("tempdir should be created");
    let path = dir.path().join("config.toml");
    let codex_config_path = dir.path().join("codex").join("config.toml");
    std::fs::create_dir_all(codex_config_path.parent().expect("path should have parent"))
        .expect("codex config dir should be created");
    std::fs::write(&codex_config_path, "notify = [")
        .expect("invalid Codex config should be written");
    let claude_settings_path = dir.path().join("claude").join("settings.json");
    let integration_paths = LocalSourceIntegrationPaths {
        codex_cli_config_path: codex_config_path,
        claude_code_settings_path: claude_settings_path,
    };
    setup::write_config(&path, &raw_test_config(None)).expect("config should be written");
    let runtime = RuntimeState::new(test_config(Some(5))).expect("runtime should be created");

    runtime
        .reload_from_path_with_local_integrations(&path, &integration_paths)
        .expect_err("invalid local integration config should fail reload");

    assert_eq!(
        runtime
            .current()
            .expect("runtime snapshot should be readable")
            .config
            .routes[0]
            .minimum_task_duration_minutes,
        Some(5)
    );
}

fn test_config(minimum_task_duration_minutes: Option<u64>) -> ValidatedConfig {
    raw_test_config(minimum_task_duration_minutes)
        .validate()
        .expect("test config should validate")
}

fn raw_test_config(minimum_task_duration_minutes: Option<u64>) -> RawConfig {
    let mut route = RouteConfig::new(vec!["codex_cli".to_string()], vec!["phone".to_string()]);
    route.minimum_task_duration_minutes = minimum_task_duration_minutes;

    let mut provider = RawProviderConfig::new("phone", ProviderType::Ntfy);
    provider.server = Some("https://ntfy.sh".to_string());
    provider.topic = Some("agents-router-test".to_string());

    RawConfig {
        schema_version: 1,
        cli: CliConfig::default(),
        log: LogConfig::default(),
        notification: NotificationConfig::default(),
        sources: vec![SourceConfig {
            id: "codex_cli".to_string(),
            source_type: SourceType::CodexCli,
        }],
        providers: vec![provider],
        routes: vec![route],
    }
}
