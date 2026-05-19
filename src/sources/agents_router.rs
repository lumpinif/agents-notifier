use std::collections::BTreeMap;

use anyhow::{Context, anyhow};

use crate::config::{SourceType, ValidatedConfig};
use crate::signal::Signal;

pub fn create_signal(
    config: &ValidatedConfig,
    source_id: &str,
    title: impl Into<String>,
    body: impl Into<String>,
) -> anyhow::Result<Signal> {
    let source = config
        .source(source_id)
        .with_context(|| format!("source `{source_id}` is not configured"))?;

    if source.source_type != SourceType::AgentsRouter {
        return Err(anyhow!(
            "source `{}` has type `{}`; expected `agents_router`",
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
        CliConfig, LogConfig, NotificationConfig, ProviderType, RawConfig, RawProviderConfig,
        RouteConfig, SourceConfig, SourceType, ValidatedConfig,
    };

    #[test]
    fn creates_signal_from_service_test_event() {
        let config = test_config(SourceType::AgentsRouter);

        let signal = create_signal(
            &config,
            "agents_router",
            "Agents Router",
            "Test notification from your computer.",
        )
        .expect("agents_router source should create signal");

        assert_eq!(signal.source_id(), "agents_router");
        assert_eq!(signal.source_type(), "agents_router");
        assert_eq!(signal.title(), "Agents Router");
        assert_eq!(signal.summary(), "Test notification from your computer.");
    }

    #[test]
    fn rejects_non_service_source() {
        let config = test_config(SourceType::AgentHook);

        let err = create_signal(&config, "agents_router", "Agents Router", "Body")
            .expect_err("wrong source type should fail");

        assert!(err.to_string().contains("expected `agents_router`"));
    }

    fn test_config(source_type: SourceType) -> ValidatedConfig {
        let mut provider = RawProviderConfig::new("debug", ProviderType::Webhook);
        provider.url = Some("https://example.com/hook".to_string());

        RawConfig {
            schema_version: 1,
            cli: CliConfig::default(),
            log: LogConfig::default(),
            notification: NotificationConfig::default(),
            sources: vec![SourceConfig {
                id: "agents_router".to_string(),
                source_type,
            }],
            providers: vec![provider],
            routes: vec![RouteConfig::new(
                vec!["agents_router".to_string()],
                vec!["debug".to_string()],
            )],
        }
        .validate()
        .expect("test config should validate")
    }
}
