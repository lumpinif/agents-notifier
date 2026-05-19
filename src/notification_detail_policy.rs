use crate::config::ProviderType;
use crate::provider_catalog::{ProviderMessageConstraint, provider_message_constraints};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationDetailSupport {
    Configurable,
    FixedPreview {
        constraints: &'static [ProviderMessageConstraint],
    },
    FixedOff {
        constraints: &'static [ProviderMessageConstraint],
    },
}

pub fn answer_detail_support(provider_type: ProviderType) -> NotificationDetailSupport {
    let constraints = provider_message_constraints(provider_type);
    if constraints.is_empty() {
        NotificationDetailSupport::Configurable
    } else {
        NotificationDetailSupport::FixedPreview { constraints }
    }
}

pub fn prompt_detail_support(provider_type: ProviderType) -> NotificationDetailSupport {
    let constraints = provider_message_constraints(provider_type);
    if constraints.is_empty() {
        NotificationDetailSupport::Configurable
    } else {
        NotificationDetailSupport::FixedOff { constraints }
    }
}

pub fn rejects_full_answer(provider_type: ProviderType) -> bool {
    matches!(
        answer_detail_support(provider_type),
        NotificationDetailSupport::FixedPreview { .. }
    )
}

pub fn rejects_prompt_detail(provider_type: ProviderType) -> bool {
    matches!(
        prompt_detail_support(provider_type),
        NotificationDetailSupport::FixedOff { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_catalog::provider_descriptor;

    #[test]
    fn constrained_providers_force_preview_and_prompt_off() {
        for provider_type in [
            ProviderType::Ntfy,
            ProviderType::Pushover,
            ProviderType::Slack,
            ProviderType::Discord,
            ProviderType::Telegram,
            ProviderType::Whatsapp,
            ProviderType::Wechat,
            ProviderType::MicrosoftTeams,
        ] {
            assert!(rejects_full_answer(provider_type));
            assert!(rejects_prompt_detail(provider_type));
            assert!(matches!(
                answer_detail_support(provider_type),
                NotificationDetailSupport::FixedPreview { constraints } if !constraints.is_empty()
            ));
            assert!(matches!(
                prompt_detail_support(provider_type),
                NotificationDetailSupport::FixedOff { constraints } if !constraints.is_empty()
            ));
        }
    }

    #[test]
    fn unconstrained_providers_keep_detail_configurable() {
        for provider_type in [
            ProviderType::FeishuLark,
            ProviderType::Webhook,
            ProviderType::EmailSmtp,
        ] {
            assert!(!rejects_full_answer(provider_type));
            assert!(!rejects_prompt_detail(provider_type));
            assert_eq!(
                answer_detail_support(provider_type),
                NotificationDetailSupport::Configurable
            );
            assert_eq!(
                prompt_detail_support(provider_type),
                NotificationDetailSupport::Configurable
            );
        }
    }

    #[test]
    fn setup_docs_restricted_provider_list_matches_policy() {
        let docs = include_str!("../docs/setup.md");

        for provider_type in [
            ProviderType::Ntfy,
            ProviderType::Pushover,
            ProviderType::Slack,
            ProviderType::Discord,
            ProviderType::Telegram,
            ProviderType::Whatsapp,
            ProviderType::Wechat,
            ProviderType::MicrosoftTeams,
        ] {
            let display_name = provider_descriptor(provider_type).display_name;
            assert!(
                docs.contains(display_name),
                "setup docs should mention restricted provider `{display_name}`"
            );
        }

        for provider_type in [
            ProviderType::FeishuLark,
            ProviderType::Webhook,
            ProviderType::EmailSmtp,
        ] {
            let display_name = provider_descriptor(provider_type).display_name;
            assert!(
                !docs.contains(&format!("- {display_name}, because")),
                "setup docs should not list configurable provider `{display_name}` as restricted"
            );
        }
    }
}
