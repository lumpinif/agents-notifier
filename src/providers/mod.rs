pub mod feishu_lark;
mod formatting;
mod http;
pub mod ntfy;
pub mod webhook;

use anyhow::anyhow;

use crate::config::{Config, ProviderType};
use crate::router::Provider;

pub use feishu_lark::FeishuLarkProvider;
pub use ntfy::NtfyProvider;
pub use webhook::WebhookProvider;

pub fn build_providers(config: &Config) -> anyhow::Result<Vec<Box<dyn Provider>>> {
    config
        .providers
        .iter()
        .map(|provider| match provider.provider_type {
            ProviderType::Ntfy => {
                Ok(Box::new(NtfyProvider::from_config(provider)?) as Box<dyn Provider>)
            }
            ProviderType::Webhook => {
                Ok(Box::new(WebhookProvider::from_config(provider)?) as Box<dyn Provider>)
            }
            ProviderType::FeishuLark => {
                Ok(Box::new(FeishuLarkProvider::from_config(provider)?) as Box<dyn Provider>)
            }
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()
        .map_err(|error| anyhow!("{error}"))
}
