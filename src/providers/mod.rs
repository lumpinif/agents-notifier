pub mod discord;
pub mod feishu_lark;
mod formatting;
mod http;
pub mod microsoft_teams;
pub mod ntfy;
pub mod pushover;
pub mod slack;
pub mod telegram;
pub mod webhook;
pub mod whatsapp;

use anyhow::anyhow;

use crate::config::{Config, ProviderType};
use crate::router::Provider;

pub use discord::DiscordProvider;
pub use feishu_lark::FeishuLarkProvider;
pub use microsoft_teams::MicrosoftTeamsProvider;
pub use ntfy::NtfyProvider;
pub use pushover::PushoverProvider;
pub use slack::SlackProvider;
pub use telegram::TelegramProvider;
pub use webhook::WebhookProvider;
pub use whatsapp::WhatsappProvider;

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
            ProviderType::Pushover => {
                Ok(Box::new(PushoverProvider::from_config(provider)?) as Box<dyn Provider>)
            }
            ProviderType::Slack => {
                Ok(Box::new(SlackProvider::from_config(provider)?) as Box<dyn Provider>)
            }
            ProviderType::Discord => {
                Ok(Box::new(DiscordProvider::from_config(provider)?) as Box<dyn Provider>)
            }
            ProviderType::Telegram => {
                Ok(Box::new(TelegramProvider::from_config(provider)?) as Box<dyn Provider>)
            }
            ProviderType::Whatsapp => {
                Ok(Box::new(WhatsappProvider::from_config(provider)?) as Box<dyn Provider>)
            }
            ProviderType::MicrosoftTeams => {
                Ok(Box::new(MicrosoftTeamsProvider::from_config(provider)?) as Box<dyn Provider>)
            }
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()
        .map_err(|error| anyhow!("{error}"))
}
