use std::fmt;
use std::str::FromStr;

use crate::config::{SourceConfig, SourceType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceIntegrationId {
    CodexDesktop,
    CodexCli,
    ClaudeCode,
    CursorCli,
    OpenCodeCli,
    OpenClaw,
    HermesAgentCli,
    GithubCopilotCli,
    GeminiCli,
    Aider,
}

impl SourceIntegrationId {
    pub fn descriptor(self) -> &'static SourceIntegrationDescriptor {
        source_integration_descriptor(self)
    }

    pub fn display_name(self) -> &'static str {
        self.descriptor().display_name
    }

    pub fn source_id(self) -> &'static str {
        self.descriptor().source_id
    }

    pub fn supports_duration_filter(self) -> bool {
        self.descriptor().duration_support.is_supported()
    }

    pub fn source_config(self) -> SourceConfig {
        self.descriptor().source_config()
    }

    pub fn from_hook_source_id(source_id: &str) -> Option<Self> {
        source_integration_by_source_id(source_id)
            .filter(|descriptor| descriptor.source_type == SourceType::AgentHook)
            .map(|descriptor| descriptor.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceIntegrationDescriptor {
    pub id: SourceIntegrationId,
    pub display_name: &'static str,
    pub source_id: &'static str,
    pub source_type: SourceType,
    pub setup_order: u16,
    pub platform_support: PlatformSupport,
    pub duration_support: DurationSupport,
    pub ingest_format: Option<SourceIngestFormat>,
    pub hook_command: Option<HookCommandTemplate>,
    pub local_integration: LocalIntegrationKind,
}

impl SourceIntegrationDescriptor {
    pub fn source_config(self) -> SourceConfig {
        SourceConfig {
            id: self.source_id.to_string(),
            source_type: self.source_type,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePlatform {
    Macos,
    Linux,
    Windows,
}

impl RuntimePlatform {
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        return Self::Macos;

        #[cfg(target_os = "linux")]
        return Self::Linux;

        #[cfg(windows)]
        return Self::Windows;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformSupport {
    All,
    MacosAndWindows,
}

impl PlatformSupport {
    pub fn supports(self, platform: RuntimePlatform) -> bool {
        match self {
            Self::All => true,
            Self::MacosAndWindows => {
                matches!(platform, RuntimePlatform::Macos | RuntimePlatform::Windows)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationSupport {
    Supported,
    Unsupported,
}

impl DurationSupport {
    pub fn is_supported(self) -> bool {
        self == Self::Supported
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalIntegrationKind {
    None,
    CodexCliStopHook,
    ClaudeCodeHooks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceIngestFormat {
    AgentHookEvent,
    ClaudeCodeHook,
    CodexCliStop,
    GeminiCliHook,
    GithubCopilotCliNotification,
    OpencodeCliSession,
}

impl SourceIngestFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentHookEvent => "agent_hook_event",
            Self::ClaudeCodeHook => "claude_code_hook",
            Self::CodexCliStop => "codex_cli_stop",
            Self::GeminiCliHook => "gemini_cli_hook",
            Self::GithubCopilotCliNotification => "github_copilot_cli_notification",
            Self::OpencodeCliSession => "opencode_cli_session",
        }
    }
}

impl fmt::Display for SourceIngestFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SourceIngestFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "agent_hook_event" => Ok(Self::AgentHookEvent),
            "claude_code_hook" => Ok(Self::ClaudeCodeHook),
            "codex_cli_stop" => Ok(Self::CodexCliStop),
            "gemini_cli_hook" => Ok(Self::GeminiCliHook),
            "github_copilot_cli_notification" => Ok(Self::GithubCopilotCliNotification),
            "opencode_cli_session" => Ok(Self::OpencodeCliSession),
            _ => Err(format!("unsupported ingest format `{value}`")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookCommandTemplate {
    Emit {
        source_id: &'static str,
    },
    Ingest {
        source_id: &'static str,
        format: SourceIngestFormat,
    },
}

impl HookCommandTemplate {
    pub fn command(self) -> String {
        match self {
            Self::Emit { source_id } => {
                format!("agents-router emit --source {source_id}")
            }
            Self::Ingest { source_id, format } => {
                format!(
                    "agents-router ingest --source {source_id} --format {}",
                    format.as_str()
                )
            }
        }
    }

    pub fn requires_emit_fields(self) -> bool {
        matches!(self, Self::Emit { .. })
    }
}

const SOURCE_INTEGRATION_DESCRIPTORS: &[SourceIntegrationDescriptor] = &[
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::CodexDesktop,
        display_name: "Codex Desktop",
        source_id: "codex_desktop",
        source_type: SourceType::CodexDesktop,
        setup_order: 10,
        platform_support: PlatformSupport::MacosAndWindows,
        duration_support: DurationSupport::Supported,
        ingest_format: None,
        hook_command: None,
        local_integration: LocalIntegrationKind::None,
    },
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::CodexCli,
        display_name: "Codex CLI",
        source_id: "codex_cli",
        source_type: SourceType::CodexCli,
        setup_order: 20,
        platform_support: PlatformSupport::All,
        duration_support: DurationSupport::Unsupported,
        ingest_format: Some(SourceIngestFormat::CodexCliStop),
        hook_command: Some(HookCommandTemplate::Ingest {
            source_id: "codex_cli",
            format: SourceIngestFormat::CodexCliStop,
        }),
        local_integration: LocalIntegrationKind::CodexCliStopHook,
    },
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::ClaudeCode,
        display_name: "Claude Code",
        source_id: "claude_code",
        source_type: SourceType::ClaudeCode,
        setup_order: 30,
        platform_support: PlatformSupport::All,
        duration_support: DurationSupport::Supported,
        ingest_format: Some(SourceIngestFormat::ClaudeCodeHook),
        hook_command: Some(HookCommandTemplate::Ingest {
            source_id: "claude_code",
            format: SourceIngestFormat::ClaudeCodeHook,
        }),
        local_integration: LocalIntegrationKind::ClaudeCodeHooks,
    },
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::CursorCli,
        display_name: "Cursor CLI",
        source_id: "cursor_cli",
        source_type: SourceType::AgentHook,
        setup_order: 40,
        platform_support: PlatformSupport::All,
        duration_support: DurationSupport::Unsupported,
        ingest_format: None,
        hook_command: Some(HookCommandTemplate::Emit {
            source_id: "cursor_cli",
        }),
        local_integration: LocalIntegrationKind::None,
    },
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::OpenCodeCli,
        display_name: "OpenCode CLI",
        source_id: "opencode_cli",
        source_type: SourceType::AgentHook,
        setup_order: 50,
        platform_support: PlatformSupport::All,
        duration_support: DurationSupport::Unsupported,
        ingest_format: Some(SourceIngestFormat::OpencodeCliSession),
        hook_command: Some(HookCommandTemplate::Ingest {
            source_id: "opencode_cli",
            format: SourceIngestFormat::OpencodeCliSession,
        }),
        local_integration: LocalIntegrationKind::None,
    },
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::OpenClaw,
        display_name: "OpenClaw",
        source_id: "openclaw",
        source_type: SourceType::AgentHook,
        setup_order: 60,
        platform_support: PlatformSupport::All,
        duration_support: DurationSupport::Unsupported,
        ingest_format: None,
        hook_command: Some(HookCommandTemplate::Emit {
            source_id: "openclaw",
        }),
        local_integration: LocalIntegrationKind::None,
    },
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::HermesAgentCli,
        display_name: "Hermes Agent CLI",
        source_id: "hermes_agent_cli",
        source_type: SourceType::AgentHook,
        setup_order: 70,
        platform_support: PlatformSupport::All,
        duration_support: DurationSupport::Unsupported,
        ingest_format: None,
        hook_command: Some(HookCommandTemplate::Emit {
            source_id: "hermes_agent_cli",
        }),
        local_integration: LocalIntegrationKind::None,
    },
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::GithubCopilotCli,
        display_name: "GitHub Copilot CLI",
        source_id: "github_copilot_cli",
        source_type: SourceType::AgentHook,
        setup_order: 80,
        platform_support: PlatformSupport::All,
        duration_support: DurationSupport::Unsupported,
        ingest_format: Some(SourceIngestFormat::GithubCopilotCliNotification),
        hook_command: Some(HookCommandTemplate::Ingest {
            source_id: "github_copilot_cli",
            format: SourceIngestFormat::GithubCopilotCliNotification,
        }),
        local_integration: LocalIntegrationKind::None,
    },
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::GeminiCli,
        display_name: "Gemini CLI",
        source_id: "gemini_cli",
        source_type: SourceType::AgentHook,
        setup_order: 90,
        platform_support: PlatformSupport::All,
        duration_support: DurationSupport::Unsupported,
        ingest_format: Some(SourceIngestFormat::GeminiCliHook),
        hook_command: Some(HookCommandTemplate::Ingest {
            source_id: "gemini_cli",
            format: SourceIngestFormat::GeminiCliHook,
        }),
        local_integration: LocalIntegrationKind::None,
    },
    SourceIntegrationDescriptor {
        id: SourceIntegrationId::Aider,
        display_name: "Aider",
        source_id: "aider",
        source_type: SourceType::AgentHook,
        setup_order: 100,
        platform_support: PlatformSupport::All,
        duration_support: DurationSupport::Unsupported,
        ingest_format: None,
        hook_command: Some(HookCommandTemplate::Emit { source_id: "aider" }),
        local_integration: LocalIntegrationKind::None,
    },
];

pub fn all_source_integration_descriptors() -> &'static [SourceIntegrationDescriptor] {
    SOURCE_INTEGRATION_DESCRIPTORS
}

pub fn source_integration_descriptor(
    id: SourceIntegrationId,
) -> &'static SourceIntegrationDescriptor {
    SOURCE_INTEGRATION_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.id == id)
        .expect("every SourceIntegrationId must have a SourceIntegrationDescriptor")
}

fn source_integration_by_source_id(
    source_id: &str,
) -> Option<&'static SourceIntegrationDescriptor> {
    SOURCE_INTEGRATION_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.source_id == source_id)
}

pub fn source_integration_for_source(
    source_id: &str,
    source_type: SourceType,
) -> Option<&'static SourceIntegrationDescriptor> {
    SOURCE_INTEGRATION_DESCRIPTORS.iter().find(|descriptor| {
        descriptor.source_id == source_id && descriptor.source_type == source_type
    })
}

pub fn setup_source_integration_descriptors_for_platform(
    platform: RuntimePlatform,
) -> impl Iterator<Item = &'static SourceIntegrationDescriptor> {
    let mut descriptors = SOURCE_INTEGRATION_DESCRIPTORS
        .iter()
        .filter(move |descriptor| descriptor.platform_support.supports(platform))
        .collect::<Vec<_>>();
    descriptors.sort_by_key(|descriptor| descriptor.setup_order);
    descriptors.into_iter()
}

pub fn default_source_integration_for_platform(
    platform: RuntimePlatform,
) -> &'static SourceIntegrationDescriptor {
    if RuntimePlatform::Macos == platform || RuntimePlatform::Windows == platform {
        source_integration_descriptor(SourceIntegrationId::CodexDesktop)
    } else {
        source_integration_descriptor(SourceIntegrationId::CodexCli)
    }
}

pub fn codex_cli_stop_hook_command() -> String {
    source_integration_descriptor(SourceIntegrationId::CodexCli)
        .hook_command
        .expect("Codex CLI hook command must be cataloged")
        .command()
}

pub fn claude_code_hook_command() -> String {
    source_integration_descriptor(SourceIntegrationId::ClaudeCode)
        .hook_command
        .expect("Claude Code hook command must be cataloged")
        .command()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_source_integration_order_matches_existing_ui_order() {
        let ids = setup_source_integration_descriptors_for_platform(RuntimePlatform::Macos)
            .map(|descriptor| descriptor.source_id)
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "codex_desktop",
                "codex_cli",
                "claude_code",
                "cursor_cli",
                "opencode_cli",
                "openclaw",
                "hermes_agent_cli",
                "github_copilot_cli",
                "gemini_cli",
                "aider",
            ]
        );
    }

    #[test]
    fn setup_source_integration_order_is_driven_by_unique_setup_order() {
        let mut setup_orders = all_source_integration_descriptors()
            .iter()
            .map(|descriptor| descriptor.setup_order)
            .collect::<Vec<_>>();
        setup_orders.sort_unstable();
        setup_orders.dedup();
        assert_eq!(
            setup_orders.len(),
            all_source_integration_descriptors().len()
        );

        let macos_setup_orders =
            setup_source_integration_descriptors_for_platform(RuntimePlatform::Macos)
                .map(|descriptor| descriptor.setup_order)
                .collect::<Vec<_>>();
        assert!(macos_setup_orders.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn source_integration_descriptors_are_canonical() {
        let cases = [
            (
                SourceIntegrationId::CodexDesktop,
                "Codex Desktop",
                "codex_desktop",
                SourceType::CodexDesktop,
                PlatformSupport::MacosAndWindows,
                DurationSupport::Supported,
                None,
                LocalIntegrationKind::None,
            ),
            (
                SourceIntegrationId::CodexCli,
                "Codex CLI",
                "codex_cli",
                SourceType::CodexCli,
                PlatformSupport::All,
                DurationSupport::Unsupported,
                Some(SourceIngestFormat::CodexCliStop),
                LocalIntegrationKind::CodexCliStopHook,
            ),
            (
                SourceIntegrationId::ClaudeCode,
                "Claude Code",
                "claude_code",
                SourceType::ClaudeCode,
                PlatformSupport::All,
                DurationSupport::Supported,
                Some(SourceIngestFormat::ClaudeCodeHook),
                LocalIntegrationKind::ClaudeCodeHooks,
            ),
            (
                SourceIntegrationId::CursorCli,
                "Cursor CLI",
                "cursor_cli",
                SourceType::AgentHook,
                PlatformSupport::All,
                DurationSupport::Unsupported,
                None,
                LocalIntegrationKind::None,
            ),
            (
                SourceIntegrationId::OpenCodeCli,
                "OpenCode CLI",
                "opencode_cli",
                SourceType::AgentHook,
                PlatformSupport::All,
                DurationSupport::Unsupported,
                Some(SourceIngestFormat::OpencodeCliSession),
                LocalIntegrationKind::None,
            ),
            (
                SourceIntegrationId::OpenClaw,
                "OpenClaw",
                "openclaw",
                SourceType::AgentHook,
                PlatformSupport::All,
                DurationSupport::Unsupported,
                None,
                LocalIntegrationKind::None,
            ),
            (
                SourceIntegrationId::HermesAgentCli,
                "Hermes Agent CLI",
                "hermes_agent_cli",
                SourceType::AgentHook,
                PlatformSupport::All,
                DurationSupport::Unsupported,
                None,
                LocalIntegrationKind::None,
            ),
            (
                SourceIntegrationId::GithubCopilotCli,
                "GitHub Copilot CLI",
                "github_copilot_cli",
                SourceType::AgentHook,
                PlatformSupport::All,
                DurationSupport::Unsupported,
                Some(SourceIngestFormat::GithubCopilotCliNotification),
                LocalIntegrationKind::None,
            ),
            (
                SourceIntegrationId::GeminiCli,
                "Gemini CLI",
                "gemini_cli",
                SourceType::AgentHook,
                PlatformSupport::All,
                DurationSupport::Unsupported,
                Some(SourceIngestFormat::GeminiCliHook),
                LocalIntegrationKind::None,
            ),
            (
                SourceIntegrationId::Aider,
                "Aider",
                "aider",
                SourceType::AgentHook,
                PlatformSupport::All,
                DurationSupport::Unsupported,
                None,
                LocalIntegrationKind::None,
            ),
        ];

        for (
            id,
            display_name,
            source_id,
            source_type,
            platform_support,
            duration_support,
            ingest_format,
            local_integration,
        ) in cases
        {
            let descriptor = source_integration_descriptor(id);
            assert_eq!(descriptor.display_name, display_name);
            assert_eq!(descriptor.source_id, source_id);
            assert_eq!(descriptor.source_type, source_type);
            assert_eq!(descriptor.platform_support, platform_support);
            assert_eq!(descriptor.duration_support, duration_support);
            assert_eq!(descriptor.ingest_format, ingest_format);
            assert_eq!(descriptor.local_integration, local_integration);
        }
    }

    #[test]
    fn linux_setup_source_integrations_exclude_codex_desktop() {
        let ids = setup_source_integration_descriptors_for_platform(RuntimePlatform::Linux)
            .map(|descriptor| descriptor.source_id)
            .collect::<Vec<_>>();

        assert_eq!(ids.first(), Some(&"codex_cli"));
        assert!(!ids.contains(&"codex_desktop"));
    }

    #[test]
    fn default_source_integration_depends_on_platform() {
        assert_eq!(
            default_source_integration_for_platform(RuntimePlatform::Macos).id,
            SourceIntegrationId::CodexDesktop
        );
        assert_eq!(
            default_source_integration_for_platform(RuntimePlatform::Windows).id,
            SourceIntegrationId::CodexDesktop
        );
        assert_eq!(
            default_source_integration_for_platform(RuntimePlatform::Linux).id,
            SourceIntegrationId::CodexCli
        );
    }

    #[test]
    fn hook_commands_are_generated_from_descriptors() {
        assert_eq!(
            codex_cli_stop_hook_command(),
            "agents-router ingest --source codex_cli --format codex_cli_stop"
        );
        assert_eq!(
            claude_code_hook_command(),
            "agents-router ingest --source claude_code --format claude_code_hook"
        );
        assert_eq!(
            source_integration_descriptor(SourceIntegrationId::GeminiCli)
                .hook_command
                .expect("Gemini CLI should expose hook command")
                .command(),
            "agents-router ingest --source gemini_cli --format gemini_cli_hook"
        );
    }

    #[test]
    fn emit_hook_commands_are_explicit_prefixes() {
        let command = source_integration_descriptor(SourceIntegrationId::Aider)
            .hook_command
            .expect("Aider should expose hook command");

        assert_eq!(command.command(), "agents-router emit --source aider");
        assert!(command.requires_emit_fields());
    }

    #[test]
    fn source_integration_lookup_requires_matching_source_type() {
        assert_eq!(
            source_integration_for_source("codex_cli", SourceType::CodexCli)
                .map(|descriptor| descriptor.id),
            Some(SourceIntegrationId::CodexCli)
        );
        assert_eq!(
            source_integration_for_source("codex_cli", SourceType::AgentHook),
            None
        );
        assert_eq!(
            source_integration_for_source("claude_code", SourceType::AgentHook),
            None
        );
    }

    #[test]
    fn documented_ingest_hook_commands_match_catalog() {
        let docs = [
            include_str!("../docs/agents/codex-cli.md"),
            include_str!("../docs/agents/codex-cli.zh-CN.md"),
            include_str!("../docs/agents/claude-code.md"),
            include_str!("../docs/agents/claude-code.zh-CN.md"),
            include_str!("../docs/agents/gemini-cli.md"),
            include_str!("../docs/agents/gemini-cli.zh-CN.md"),
            include_str!("../docs/agents/github-copilot-cli.md"),
            include_str!("../docs/agents/github-copilot-cli.zh-CN.md"),
            include_str!("../docs/agents/opencode-cli.md"),
        ]
        .join("\n");

        for id in [
            SourceIntegrationId::CodexCli,
            SourceIntegrationId::ClaudeCode,
            SourceIntegrationId::GeminiCli,
            SourceIntegrationId::GithubCopilotCli,
            SourceIntegrationId::OpenCodeCli,
        ] {
            let descriptor = source_integration_descriptor(id);
            let command = descriptor
                .hook_command
                .expect("structured integrations should expose hook command")
                .command();
            assert!(
                docs.contains(&command),
                "docs should include catalog command `{command}`"
            );
        }
    }

    #[test]
    fn ingest_format_strings_remain_compatible() {
        for (raw, expected) in [
            ("agent_hook_event", SourceIngestFormat::AgentHookEvent),
            ("claude_code_hook", SourceIngestFormat::ClaudeCodeHook),
            ("codex_cli_stop", SourceIngestFormat::CodexCliStop),
            ("gemini_cli_hook", SourceIngestFormat::GeminiCliHook),
            (
                "github_copilot_cli_notification",
                SourceIngestFormat::GithubCopilotCliNotification,
            ),
            (
                "opencode_cli_session",
                SourceIngestFormat::OpencodeCliSession,
            ),
        ] {
            assert_eq!(raw.parse::<SourceIngestFormat>(), Ok(expected));
            assert_eq!(expected.as_str(), raw);
        }
    }
}
