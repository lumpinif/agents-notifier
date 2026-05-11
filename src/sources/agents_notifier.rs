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
    let source = config
        .source(source_id)
        .with_context(|| format!("source `{source_id}` is not configured"))?;

    if source.source_type != SourceType::AgentsNotifier {
        return Err(anyhow!(
            "source `{}` has type `{}`; expected `agents_notifier`",
            source.id,
            source.source_type.as_str()
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
        CliConfig, Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType,
        RouteConfig, SourceConfig, SourceType,
    };

    #[test]
    fn creates_signal_from_service_test_event() {
        let config = test_config(SourceType::AgentsNotifier);

        let signal = create_signal(
            &config,
            "agents_notifier",
            "Agents Notifier",
            "Test notification from your computer.",
        )
        .expect("agents_notifier source should create signal");

        assert_eq!(signal.source_id, "agents_notifier");
        assert_eq!(signal.source_type, "agents_notifier");
        assert_eq!(signal.title, "Agents Notifier");
        assert_eq!(signal.body, "Test notification from your computer.");
    }

    #[test]
    fn rejects_non_service_source() {
        let config = test_config(SourceType::CodexCli);

        let err = create_signal(&config, "agents_notifier", "Agents Notifier", "Body")
            .expect_err("wrong source type should fail");

        assert!(err.to_string().contains("expected `agents_notifier`"));
    }

    fn test_config(source_type: SourceType) -> Config {
        Config {
            schema_version: 1,
            cli: CliConfig::default(),
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: "agents_notifier".to_string(),
                source_type,
            }],
            providers: vec![ProviderConfig {
                id: "debug".to_string(),
                provider_type: ProviderType::Webhook,
                base_url: None,
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
                token: None,
                token_env: None,
                recipient_user_id: None,
                context_token: None,
                context_token_env: None,
                route_tag: None,
            }],
            routes: vec![RouteConfig {
                sources: vec!["agents_notifier".to_string()],
                providers: vec!["debug".to_string()],
            }],
        }
    }
}
