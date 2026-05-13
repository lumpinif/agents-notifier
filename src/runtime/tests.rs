use tempfile::tempdir;

use crate::config::{
    CliConfig, Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType, RouteConfig,
    SourceConfig, SourceType,
};
use crate::setup;

use super::*;

#[test]
fn reload_replaces_current_runtime_snapshot() {
    let dir = tempdir().expect("tempdir should be created");
    let path = dir.path().join("config.toml");
    setup::write_config(&path, &test_config(None)).expect("config should be written");
    let runtime = RuntimeState::new(test_config(None)).expect("runtime should be created");

    setup::write_config(&path, &test_config(Some(5))).expect("updated config should be written");
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
    setup::write_config(&path, &test_config(Some(5))).expect("config should be written");
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
topic = "agents-notifier-test"

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

fn test_config(minimum_task_duration_minutes: Option<u64>) -> Config {
    let mut route = RouteConfig::new(vec!["codex_cli".to_string()], vec!["phone".to_string()]);
    route.minimum_task_duration_minutes = minimum_task_duration_minutes;

    Config {
        schema_version: 1,
        cli: CliConfig::default(),
        log: LogConfig::default(),
        notification: NotificationConfig::default(),
        sources: vec![SourceConfig {
            id: "codex_cli".to_string(),
            source_type: SourceType::CodexCli,
        }],
        providers: vec![ProviderConfig {
            id: "phone".to_string(),
            provider_type: ProviderType::Ntfy,
            base_url: None,
            server: Some("https://ntfy.sh".to_string()),
            topic: Some("agents-notifier-test".to_string()),
            url: None,
            url_env: None,
            secret: None,
            secret_env: None,
            app_token: None,
            app_token_env: None,
            user_key: None,
            user_key_env: None,
            device: None,
            sound: None,
            bot_token: None,
            bot_token_env: None,
            chat_id: None,
            access_token: None,
            access_token_env: None,
            phone_number_id: None,
            recipient_phone_number: None,
            host: None,
            port: None,
            security: None,
            username: None,
            username_env: None,
            password: None,
            password_env: None,
            from: None,
            to: None,
            reply_to: None,
            token: None,
            token_env: None,
            recipient_user_id: None,
            context_token: None,
            context_token_env: None,
            route_tag: None,
        }],
        routes: vec![route],
    }
}
