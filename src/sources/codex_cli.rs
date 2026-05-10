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

    if source.source_type != SourceType::CodexCli {
        return Err(anyhow!(
            "source `{}` has type `{}`; expected `codex_cli`",
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
        Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType, RouteConfig,
        SourceConfig, SourceType,
    };

    #[test]
    fn creates_signal_from_cli_args() {
        let config = test_config(SourceType::CodexCli);

        let signal = create_signal(&config, "codex_cli", "Codex", "Ready for review.")
            .expect("codex_cli source should create signal");

        assert_eq!(signal.source_id, "codex_cli");
        assert_eq!(signal.source_type, "codex_cli");
        assert_eq!(signal.title, "Codex");
        assert_eq!(signal.body, "Ready for review.");
    }

    #[test]
    fn rejects_missing_source() {
        let config = test_config(SourceType::CodexCli);

        let err = create_signal(&config, "missing", "Codex", "Body")
            .expect_err("missing source should fail");

        assert!(
            err.to_string()
                .contains("source `missing` is not configured")
        );
    }

    #[test]
    fn rejects_non_codex_cli_source() {
        let config = test_config(SourceType::CodexDesktop);

        let err = create_signal(&config, "codex_cli", "Codex", "Body")
            .expect_err("wrong source type should fail");

        assert!(err.to_string().contains("expected `codex_cli`"));
    }

    fn test_config(source_type: SourceType) -> Config {
        Config {
            schema_version: 1,
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: "codex_cli".to_string(),
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
                sources: vec!["codex_cli".to_string()],
                providers: vec!["debug".to_string()],
            }],
        }
    }
}
