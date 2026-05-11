use std::collections::BTreeMap;

use anyhow::{Context, anyhow};

use crate::config::{Config, SourceType};
use crate::signal::Signal;

pub fn create_signal(
    config: &Config,
    source_id: &str,
    title: impl Into<String>,
    body: impl Into<String>,
) -> anyhow::Result<Signal> {
    create_signal_for_type(config, source_id, SourceType::AgentHook, title, body)
}

pub fn create_signal_for_type(
    config: &Config,
    source_id: &str,
    expected_source_type: SourceType,
    title: impl Into<String>,
    body: impl Into<String>,
) -> anyhow::Result<Signal> {
    let source = config
        .source(source_id)
        .with_context(|| format!("source `{source_id}` is not configured"))?;

    if source.source_type != expected_source_type {
        return Err(anyhow!(
            "source `{}` has type `{}`; expected `{}`",
            source.id,
            source.source_type.as_str(),
            expected_source_type.as_str()
        ));
    }

    Ok(Signal::new(
        source.id.clone(),
        source.source_type.as_str(),
        title,
        body,
        BTreeMap::new(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType, RouteConfig,
        SourceConfig, SourceType,
    };

    #[test]
    fn creates_signal_for_expected_hook_source_type() {
        let config = test_config("opencode_cli", SourceType::AgentHook);

        let signal = create_signal(&config, "opencode_cli", "OpenCode CLI", "Ready for review.")
            .expect("expected hook source should create signal");

        assert_eq!(signal.source_id, "opencode_cli");
        assert_eq!(signal.source_type, "agent_hook");
        assert_eq!(signal.title, "OpenCode CLI");
        assert_eq!(signal.body, "Ready for review.");
    }

    #[test]
    fn rejects_missing_hook_source() {
        let config = test_config("codex_cli", SourceType::CodexCli);

        let err = create_signal_for_type(&config, "missing", SourceType::CodexCli, "Codex", "Body")
            .expect_err("missing source should fail");

        assert!(
            err.to_string()
                .contains("source `missing` is not configured")
        );
    }

    #[test]
    fn rejects_unexpected_hook_source_type() {
        let config = test_config("claude_code", SourceType::ClaudeCode);

        let err = create_signal_for_type(
            &config,
            "claude_code",
            SourceType::CodexCli,
            "Codex",
            "Body",
        )
        .expect_err("wrong source type should fail");

        assert!(
            err.to_string()
                .contains("source `claude_code` has type `claude_code`; expected `codex_cli`")
        );
    }

    fn test_config(source_id: &str, source_type: SourceType) -> Config {
        Config {
            schema_version: 1,
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: source_id.to_string(),
                source_type,
            }],
            providers: vec![ProviderConfig {
                id: "debug".to_string(),
                provider_type: ProviderType::Webhook,
                server: None,
                topic: None,
                url: Some("https://example.com/hook".to_string()),
                url_env: None,
                secret: None,
                secret_env: None,
                app_token: None,
                app_token_env: None,
                user_key: None,
                user_key_env: None,
                device: None,
                sound: None,
            }],
            routes: vec![RouteConfig {
                sources: vec![source_id.to_string()],
                providers: vec!["debug".to_string()],
            }],
        }
    }
}
