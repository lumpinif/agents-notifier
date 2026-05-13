use std::sync::{Arc, Mutex};

use agents_router::config::{
    CliConfig, Config, LogConfig, NotificationConfig, ProviderConfig, ProviderType, RouteConfig,
    SourceConfig, SourceType,
};
use agents_router::delivery::ProviderSendResult;
use agents_router::router::{Provider, ProviderFuture};
use agents_router::signal::Signal;

pub struct SignalCaptureProvider {
    signals: Arc<Mutex<Vec<Signal>>>,
}

impl SignalCaptureProvider {
    pub fn new() -> Self {
        Self {
            signals: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn signals(&self) -> Vec<Signal> {
        self.signals
            .lock()
            .expect("signals lock should not be poisoned")
            .clone()
    }
}

impl Provider for SignalCaptureProvider {
    fn id(&self) -> &str {
        "capture"
    }

    fn provider_type(&self) -> &str {
        "signal_capture"
    }

    fn send<'a>(&'a self, signal: &'a Signal) -> ProviderFuture<'a> {
        Box::pin(async move {
            self.signals
                .lock()
                .expect("signals lock should not be poisoned")
                .push(signal.clone());
            Ok(ProviderSendResult::sent(
                self.id(),
                self.provider_type(),
                signal,
            ))
        })
    }
}

pub fn source_contract_config(source_id: &str, source_type: SourceType) -> Config {
    let mut provider = ProviderConfig::new("capture", ProviderType::Webhook);
    provider.url = Some("https://example.com/capture".to_string());

    Config {
        schema_version: 1,
        cli: CliConfig::default(),
        log: LogConfig::default(),
        notification: NotificationConfig {
            answer_detail: agents_router::config::AnswerDetail::Full,
            prompt_detail: agents_router::config::PromptDetail::On,
        },
        sources: vec![SourceConfig {
            id: source_id.to_string(),
            source_type,
        }],
        providers: vec![provider],
        routes: vec![RouteConfig::new(
            vec![source_id.to_string()],
            vec!["capture".to_string()],
        )],
    }
}
