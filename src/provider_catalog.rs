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
    pub enforcement: MessageConstraintEnforcement,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageConstraintEnforcement {
    PolicyOnly,
    LocalPreflight,
}

const NO_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[];

const NTFY_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::MessageBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(4096),
    source: MessageConstraintSource::ProviderDocumentedDefault,
    enforcement: MessageConstraintEnforcement::PolicyOnly,
}];

const PUSHOVER_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[
    ProviderMessageConstraint {
        surface: MessageSurface::Title,
        unit: MessageLimitUnit::Characters,
        limit: Some(250),
        source: MessageConstraintSource::ProviderDocumented,
        enforcement: MessageConstraintEnforcement::LocalPreflight,
    },
    ProviderMessageConstraint {
        surface: MessageSurface::MessageBody,
        unit: MessageLimitUnit::Characters,
        limit: Some(1024),
        source: MessageConstraintSource::ProviderDocumented,
        enforcement: MessageConstraintEnforcement::LocalPreflight,
    },
];

const SLACK_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::TextBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(4000),
    source: MessageConstraintSource::LocalDeliveryGuard,
    enforcement: MessageConstraintEnforcement::LocalPreflight,
}];

const DISCORD_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::WebhookContent,
    unit: MessageLimitUnit::Characters,
    limit: Some(2000),
    source: MessageConstraintSource::ProviderDocumented,
    enforcement: MessageConstraintEnforcement::LocalPreflight,
}];

const TELEGRAM_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::TextBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(4096),
    source: MessageConstraintSource::ProviderDocumented,
    enforcement: MessageConstraintEnforcement::LocalPreflight,
}];

const WHATSAPP_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::TextBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(4096),
    source: MessageConstraintSource::LocalDeliveryGuard,
    enforcement: MessageConstraintEnforcement::LocalPreflight,
}];

const WECHAT_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] = &[ProviderMessageConstraint {
    surface: MessageSurface::TextBody,
    unit: MessageLimitUnit::Characters,
    limit: Some(3800),
    source: MessageConstraintSource::LocalDeliveryGuard,
    enforcement: MessageConstraintEnforcement::LocalPreflight,
}];

const MICROSOFT_TEAMS_MESSAGE_CONSTRAINTS: &[ProviderMessageConstraint] =
    &[ProviderMessageConstraint {
        surface: MessageSurface::Payload,
        unit: MessageLimitUnit::Bytes,
        limit: Some(28 * 1024),
        source: MessageConstraintSource::ProviderDocumented,
        enforcement: MessageConstraintEnforcement::LocalPreflight,
    }];

const PROVIDER_DESCRIPTORS: &[ProviderDescriptor] = &[
    ProviderDescriptor {
        provider_type: ProviderType::Slack,
        display_name: "Slack",
        setup_order: 10,
        capabilities: ProviderCapabilities {
            message_constraints: SLACK_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Discord,
        display_name: "Discord",
        setup_order: 20,
        capabilities: ProviderCapabilities {
            message_constraints: DISCORD_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Telegram,
        display_name: "Telegram",
        setup_order: 30,
        capabilities: ProviderCapabilities {
            message_constraints: TELEGRAM_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::MicrosoftTeams,
        display_name: "Microsoft Teams",
        setup_order: 40,
        capabilities: ProviderCapabilities {
            message_constraints: MICROSOFT_TEAMS_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::EmailSmtp,
        display_name: "Email SMTP",
        setup_order: 50,
        capabilities: ProviderCapabilities {
            message_constraints: NO_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Ntfy,
        display_name: "ntfy",
        setup_order: 60,
        capabilities: ProviderCapabilities {
            message_constraints: NTFY_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Pushover,
        display_name: "Pushover",
        setup_order: 70,
        capabilities: ProviderCapabilities {
            message_constraints: PUSHOVER_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::FeishuLark,
        display_name: "Feishu/Lark custom bot",
        setup_order: 80,
        capabilities: ProviderCapabilities {
            message_constraints: NO_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Webhook,
        display_name: "Webhook",
        setup_order: 90,
        capabilities: ProviderCapabilities {
            message_constraints: NO_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Whatsapp,
        display_name: "WhatsApp",
        setup_order: 100,
        capabilities: ProviderCapabilities {
            message_constraints: WHATSAPP_MESSAGE_CONSTRAINTS,
        },
    },
    ProviderDescriptor {
        provider_type: ProviderType::Wechat,
        display_name: "WeChat",
        setup_order: 110,
        capabilities: ProviderCapabilities {
            message_constraints: WECHAT_MESSAGE_CONSTRAINTS,
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

pub fn default_setup_provider_type() -> ProviderType {
    ProviderType::Slack
}

pub fn default_setup_provider_descriptor() -> &'static ProviderDescriptor {
    provider_descriptor(default_setup_provider_type())
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

pub fn provider_local_preflight_message_constraint(
    provider_type: ProviderType,
    surface: MessageSurface,
) -> Option<&'static ProviderMessageConstraint> {
    provider_message_constraint(provider_type, surface)
        .filter(|constraint| constraint.enforcement == MessageConstraintEnforcement::LocalPreflight)
}

pub fn provider_local_preflight_message_limit(
    provider_type: ProviderType,
    surface: MessageSurface,
) -> usize {
    provider_local_preflight_message_constraint(provider_type, surface)
        .and_then(|constraint| constraint.limit)
        .expect("provider local preflight message limit must be cataloged for this surface")
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
                "slack",
                "discord",
                "telegram",
                "microsoft_teams",
                "email_smtp",
                "ntfy",
                "pushover",
                "feishu_lark",
                "webhook",
                "whatsapp",
                "wechat",
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
    fn default_setup_provider_is_cataloged() {
        assert_eq!(default_setup_provider_type(), ProviderType::Slack);
        assert_eq!(
            default_setup_provider_descriptor().provider_type,
            default_setup_provider_type()
        );
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
                enforcement: MessageConstraintEnforcement::PolicyOnly,
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
                    enforcement: MessageConstraintEnforcement::LocalPreflight,
                },
                ProviderMessageConstraint {
                    surface: MessageSurface::MessageBody,
                    unit: MessageLimitUnit::Characters,
                    limit: Some(1024),
                    source: MessageConstraintSource::ProviderDocumented,
                    enforcement: MessageConstraintEnforcement::LocalPreflight,
                },
            ]
        );
        let teams_payload_constraint =
            provider_message_constraint(ProviderType::MicrosoftTeams, MessageSurface::Payload)
                .expect("Teams payload constraint should be cataloged");
        assert_eq!(teams_payload_constraint.limit, Some(28 * 1024));
        assert_eq!(
            provider_local_preflight_message_limit(
                ProviderType::MicrosoftTeams,
                MessageSurface::Payload
            ),
            28 * 1024
        );
        assert!(
            provider_local_preflight_message_constraint(
                ProviderType::Ntfy,
                MessageSurface::MessageBody
            )
            .is_none()
        );
        assert!(provider_message_constraints(ProviderType::Webhook).is_empty());
        assert!(provider_message_constraints(ProviderType::FeishuLark).is_empty());
        assert!(provider_message_constraints(ProviderType::EmailSmtp).is_empty());
    }

    #[test]
    fn user_docs_include_cataloged_message_constraint_limits() {
        for descriptor in all_provider_descriptors() {
            let provider_docs = provider_user_docs(descriptor.provider_type);
            for constraint in descriptor.capabilities.message_constraints {
                let Some(limit) = constraint.limit else {
                    continue;
                };
                let expected = docs_limit_text(*constraint, limit);
                assert!(
                    provider_docs.contains(&expected),
                    "provider docs should include cataloged limit `{expected}` for `{}`",
                    descriptor.provider_type.as_str()
                );
            }
        }
    }

    #[test]
    fn setup_docs_include_policy_limit_for_each_restricted_provider() {
        let setup_docs = include_str!("../docs/setup.md");

        for (provider_type, surface) in [
            (ProviderType::Ntfy, MessageSurface::MessageBody),
            (ProviderType::Pushover, MessageSurface::MessageBody),
            (ProviderType::Slack, MessageSurface::TextBody),
            (ProviderType::Discord, MessageSurface::WebhookContent),
            (ProviderType::Telegram, MessageSurface::TextBody),
            (ProviderType::Whatsapp, MessageSurface::TextBody),
            (ProviderType::Wechat, MessageSurface::TextBody),
            (ProviderType::MicrosoftTeams, MessageSurface::Payload),
        ] {
            let descriptor = provider_descriptor(provider_type);
            let constraint = provider_message_constraint(provider_type, surface)
                .expect("restricted provider policy limit must be cataloged");
            let limit = constraint
                .limit
                .expect("restricted provider policy limit must have a numeric value");
            let expected = docs_limit_text(*constraint, limit);
            let provider_line = setup_docs
                .lines()
                .find(|line| line.starts_with(&format!("- {},", descriptor.display_name)))
                .unwrap_or_else(|| {
                    panic!(
                        "setup docs should include a restricted provider line for `{}`",
                        descriptor.provider_type.as_str()
                    )
                });

            assert!(
                provider_line.contains(&expected),
                "setup docs line `{provider_line}` should include cataloged limit `{expected}`"
            );
        }
    }

    fn provider_user_docs(provider_type: ProviderType) -> String {
        match provider_type {
            ProviderType::Ntfy => [
                include_str!("../docs/providers/ntfy.md"),
                include_str!("../docs/providers/ntfy.zh-CN.md"),
            ]
            .join("\n"),
            ProviderType::Pushover => [
                include_str!("../docs/providers/pushover.md"),
                include_str!("../docs/providers/pushover.zh-CN.md"),
            ]
            .join("\n"),
            ProviderType::Slack => [
                include_str!("../docs/providers/slack.md"),
                include_str!("../docs/providers/slack.zh-CN.md"),
            ]
            .join("\n"),
            ProviderType::Discord => [
                include_str!("../docs/providers/discord.md"),
                include_str!("../docs/providers/discord.zh-CN.md"),
            ]
            .join("\n"),
            ProviderType::Telegram => [
                include_str!("../docs/providers/telegram.md"),
                include_str!("../docs/providers/telegram.zh-CN.md"),
            ]
            .join("\n"),
            ProviderType::Whatsapp => [
                include_str!("../docs/providers/whatsapp.md"),
                include_str!("../docs/providers/whatsapp.zh-CN.md"),
            ]
            .join("\n"),
            ProviderType::Wechat => [
                include_str!("../docs/providers/wechat.md"),
                include_str!("../docs/providers/wechat.zh-CN.md"),
            ]
            .join("\n"),
            ProviderType::MicrosoftTeams => [
                include_str!("../docs/providers/microsoft-teams.md"),
                include_str!("../docs/providers/microsoft-teams.zh-CN.md"),
            ]
            .join("\n"),
            ProviderType::Webhook | ProviderType::FeishuLark | ProviderType::EmailSmtp => {
                String::new()
            }
        }
    }

    fn docs_limit_text(constraint: ProviderMessageConstraint, limit: usize) -> String {
        if constraint.unit == MessageLimitUnit::Bytes && limit % 1024 == 0 {
            return format!("{} KB", limit / 1024);
        }

        limit.to_string()
    }
}
