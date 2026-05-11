use crate::config::{Config, SourceType};
use crate::signal::Signal;
use crate::sources::agent_hook;

pub fn create_signal(
    config: &Config,
    source_id: &str,
    title: impl Into<String>,
    body: impl Into<String>,
) -> anyhow::Result<Signal> {
    agent_hook::create_signal_for_type(config, source_id, SourceType::ClaudeCode, title, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType, RouteConfig,
        SourceConfig, SourceType,
    };

    #[test]
    fn creates_signal_from_claude_code_hook_event() {
        let config = test_config(SourceType::ClaudeCode);

        let signal = create_signal(
            &config,
            "claude_code",
            "Claude Code",
            "Claude Code finished a task.",
        )
        .expect("claude_code source should create signal");

        assert_eq!(signal.source_id, "claude_code");
        assert_eq!(signal.source_type, "claude_code");
        assert_eq!(signal.title, "Claude Code");
        assert_eq!(signal.body, "Claude Code finished a task.");
    }

    #[test]
    fn rejects_non_claude_code_source() {
        let config = test_config(SourceType::CodexCli);

        let err = create_signal(&config, "claude_code", "Claude Code", "Body")
            .expect_err("wrong source type should fail");

        assert!(err.to_string().contains("expected `claude_code`"));
    }

    fn test_config(source_type: SourceType) -> Config {
        Config {
            schema_version: 1,
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: "claude_code".to_string(),
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
            }],
            routes: vec![RouteConfig {
                sources: vec!["claude_code".to_string()],
                providers: vec!["debug".to_string()],
            }],
        }
    }
}
