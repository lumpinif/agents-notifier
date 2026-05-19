use crate::config::ProviderType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub provider_type: ProviderType,
    pub display_name: &'static str,
    pub setup_order: u16,
    pub capabilities: ProviderCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub message_constraints: &'static [ProviderMessageConstraint],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderMessageConstraint {
    pub surface: MessageSurface,
    pub unit: MessageLimitUnit,
    pub limit: Option<usize>,
    pub source: MessageConstraintSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageSurface {
    Title,
    MessageBody,
    TextBody,
    WebhookContent,
    Payload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageLimitUnit {
    Characters,
    Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageConstraintSource {
    ProviderDocumented,
    ProviderDocumentedDefault,
    LocalDeliveryGuard,
}

const NO_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[];

const NTFY_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::MessageBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(4096),
    source: MessageConstraintSource::ProviderDocumentedDefault,
}];

const PUSHOVER_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[
    ProviderMessageConstraint {
        surface: MessageSurface::Title,
        unit: MessageLimitUnit::Characters,
        limit: Some(250),
        source: MessageConstraintSource::ProviderDocumented,
    },
    ProviderMessageConstraint {
        surface: MessageSurface::MessageBody,
        unit: MessageLimitUnit::Characters,
        limit: Some(1024),
        source: MessageConstraintSource::ProviderDocumented,
    },
];

const SLACK_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::TextBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(4000),
    source: MessageConstraintSource::LocalDeliveryGuard,
}];

const DISCORD_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::WebhookContent,
    unit: MessageLimitUnit::Characters,
    limit: Some(2000),
    source: MessageConstraintSource::ProviderDocumented,
}];

const TELEGRAM_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::TextBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(4096),
    source: MessageConstraintSource::ProviderDocumented,
}];

const WHATSAPP_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::TextBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(4096),
    source: MessageConstraintSource::LocalDeliveryGuard,
}];

const WECHAT_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::TextBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(3800),
    source: MessageConstraintSource::LocalDeliveryGuard,
}];

const MICROSOFT_TEAMS_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] =
    &[ProviderMessageConstraint {
        surface: MessageSurface::Payload,
        unit: MessageLimitUnit::Bytes,
        limit: Some(28 * 1024),
        source: MessageConstraintSource::ProviderDocumented,
    }];

const PROVIDER_DESCRIPTORS: &[ProviderDescriptor] = &[
    ProviderDescriptor {
        provider_type: ProviderType::Ntfy,
        display_name: "ntfy",
        setup_order: 10,
        capabilities: ProviderCapabilities {
            message_constraints: NTFY_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Slack,
        display_name: "Slack",
        setup_order: 20,
        capabilities: ProviderCapabilities {
            message_constraints: SLACK_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Discord,
        display_name: "Discord",
        setup_order: 30,
        capabilities: ProviderCapabilities {
            message_constraints: DISCORD_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Pushover,
        display_name: "Pushover",
        setup_order: 40,
        capabilities: ProviderCapabilities {
            message_constraints: PUSHOVER_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::FeishuLark,
        display_name: "Feishu/Lark custom bot",
        setup_order: 50,
        capabilities: ProviderCapabilities {
            message_constraints: NO_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Webhook,
        display_name: "Webhook",
        setup_order: 60,
        capabilities: ProviderCapabilities {
            message_constraints: NO_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Telegram,
        display_name: "Telegram",
        setup_order: 70,
        capabilities: ProviderCapabilities {
            message_constraints: TELEGRAM_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Whatsapp,
        display_name: "WhatsApp",
        setup_order: 80,
        capabilities: ProviderCapabilities {
            message_constraints: WHATSAPP_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Wechat,
        display_name: "WeChat",
        setup_order: 90,
        capabilities: ProviderCapabilities {
            message_constraints: WECHAT_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::MicrosoftTeams,
        display_name: "Microsoft Teams",
        setup_order: 100,
        capabilities: ProviderCapabilities {
            message_constraints: MICROSOFT_TEAMS_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::EmailSmtp,
        display_name: "Email SMTP",
        setup_order: 110,
        capabilities: ProviderCapabilities {
            message_constraints: NO_MESSAGE_CONSTRAINTS,
        },
    },
];

pub fn all_provider_descriptors() -> &'static [ProviderDescriptor] {
    PROVIDER_DESCRIPTORS
}

pub fn provider_descriptor(provider_type: ProviderType) -> &'static ProviderDescriptor {
    PROVIDER_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.provider_type == provider_type)
        .expect("every ProviderType must have a ProviderDescriptor")
}

pub fn setup_provider_descriptors() -> impl Iterator<Item = &'static ProviderDescriptor> {
    let mut descriptors = PROVIDER_DESCRIPTORS.iter().collect::<Vec<_>>();
    descriptors.sort_by_key(|descriptor| descriptor.setup_order);
    descriptors.into_iter()
}

pub fn provider_message_constraints(
    provider_type: ProviderType,
) -> &'static [ProviderMessageConstraint] {
    provider_descriptor(provider_type)
        .capabilities
        .message_constraints
}

pub fn provider_message_constraint(
    provider_type: ProviderType,
    surface: MessageSurface,
) -> Option<&'static ProviderMessageConstraint> {
    provider_message_constraints(provider_type)
        .iter()
        .find(|constraint| constraint.surface == surface)
}

pub fn provider_message_limit(provider_type: ProviderType, surface: MessageSurface) -> usize {
    provider_message_constraint(provider_type, surface)
        .and_then(|constraint| constraint.limit)
        .expect("provider message limit must be cataloged for this surface")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_type_has_one_descriptor() {
        let provider_types = [
            ProviderType::Ntfy,
            ProviderType::Webhook,
            ProviderType::FeishuLark,
            ProviderType::Pushover,
            ProviderType::Slack,
            ProviderType::Discord,
            ProviderType::Telegram,
            ProviderType::Whatsapp,
            ProviderType::Wechat,
            ProviderType::MicrosoftTeams,
            ProviderType::EmailSmtp,
        ];

        for provider_type in provider_types {
            let matches = all_provider_descriptors()
                .iter()
                .filter(|descriptor| descriptor.provider_type == provider_type)
                .count();
            assert_eq!(
                matches,
                1,
                "{} should have one descriptor",
                provider_type.as_str()
            );
        }
        assert_eq!(all_provider_descriptors().len(), provider_types.len());
    }

    #[test]
    fn setup_provider_order_matches_existing_ui_order() {
        let provider_ids = setup_provider_descriptors()
            .map(|descriptor| descriptor.provider_type.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            provider_ids,
            vec![
                "ntfy",
                "slack",
                "discord",
                "pushover",
                "feishu_lark",
                "webhook",
                "telegram",
                "whatsapp",
                "wechat",
                "microsoft_teams",
                "email_smtp",
            ]
        );
    }

    #[test]
    fn setup_provider_order_is_driven_by_unique_setup_order() {
        let mut setup_orders = all_provider_descriptors()
            .iter()
            .map(|descriptor| descriptor.setup_order)
            .collect::<Vec<_>>();
        setup_orders.sort_unstable();
        setup_orders.dedup();
        assert_eq!(setup_orders.len(), all_provider_descriptors().len());

        let sorted_setup_orders = setup_provider_descriptors()
            .map(|descriptor| descriptor.setup_order)
            .collect::<Vec<_>>();
        assert!(sorted_setup_orders.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn message_constraints_are_canonical() {
        assert_eq!(
            provider_message_constraints(ProviderType::Ntfy),
            &[ProviderMessageConstraint {
                surface: MessageSurface::MessageBody,
                unit: MessageLimitUnit::Characters,
                limit: Some(4096),
                source: MessageConstraintSource::ProviderDocumentedDefault,
            }]
        );
        assert_eq!(
            provider_message_constraints(ProviderType::Pushover),
            &[
                ProviderMessageConstraint {
                    surface: MessageSurface::Title,
                    unit: MessageLimitUnit::Characters,
                    limit: Some(250),
                    source: MessageConstraintSource::ProviderDocumented,
                },
                ProviderMessageConstraint {
                    surface: MessageSurface::MessageBody,
                    unit: MessageLimitUnit::Characters,
                    limit: Some(1024),
                    source: MessageConstraintSource::ProviderDocumented,
                },
            ]
        );
        assert_eq!(
            provider_message_limit(ProviderType::MicrosoftTeams, MessageSurface::Payload),
            28 * 1024
        );
        assert!(provider_message_constraints(ProviderType::Webhook).is_empty());
        assert!(provider_message_constraints(ProviderType::FeishuLark).is_empty());
        assert!(provider_message_constraints(ProviderType::EmailSmtp).is_empty());
    }
}
