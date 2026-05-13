use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSelection {
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

impl AgentSelection {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::CodexDesktop => "Codex Desktop",
            Self::CodexCli => "Codex CLI",
            Self::ClaudeCode => "Claude Code",
            Self::CursorCli => "Cursor CLI",
            Self::OpenCodeCli => "OpenCode CLI",
            Self::OpenClaw => "OpenClaw",
            Self::HermesAgentCli => "Hermes Agent CLI",
            Self::GithubCopilotCli => "GitHub Copilot CLI",
            Self::GeminiCli => "Gemini CLI",
            Self::Aider => "Aider",
        }
    }

    pub fn source_id(self) -> &'static str {
        match self {
            Self::CodexDesktop => "codex_desktop",
            Self::CodexCli => "codex_cli",
            Self::ClaudeCode => "claude_code",
            Self::CursorCli => "cursor_cli",
            Self::OpenCodeCli => "opencode_cli",
            Self::OpenClaw => "openclaw",
            Self::HermesAgentCli => "hermes_agent_cli",
            Self::GithubCopilotCli => "github_copilot_cli",
            Self::GeminiCli => "gemini_cli",
            Self::Aider => "aider",
        }
    }

    pub fn supports_duration_filter(self) -> bool {
        matches!(self, Self::CodexDesktop | Self::ClaudeCode)
    }

    pub fn from_hook_source_id(source_id: &str) -> Option<Self> {
        match source_id {
            "cursor_cli" => Some(Self::CursorCli),
            "opencode_cli" => Some(Self::OpenCodeCli),
            "openclaw" => Some(Self::OpenClaw),
            "hermes_agent_cli" => Some(Self::HermesAgentCli),
            "github_copilot_cli" => Some(Self::GithubCopilotCli),
            "gemini_cli" => Some(Self::GeminiCli),
            "aider" => Some(Self::Aider),
            _ => None,
        }
    }

    pub(super) fn source_config(self) -> SourceConfig {
        match self {
            Self::CodexDesktop => SourceConfig {
                id: self.source_id().to_string(),
                source_type: SourceType::CodexDesktop,
            },
            Self::CodexCli => SourceConfig {
                id: self.source_id().to_string(),
                source_type: SourceType::CodexCli,
            },
            Self::ClaudeCode => SourceConfig {
                id: self.source_id().to_string(),
                source_type: SourceType::ClaudeCode,
            },
            Self::CursorCli
            | Self::OpenCodeCli
            | Self::OpenClaw
            | Self::HermesAgentCli
            | Self::GithubCopilotCli
            | Self::GeminiCli
            | Self::Aider => SourceConfig {
                id: self.source_id().to_string(),
                source_type: SourceType::AgentHook,
            },
        }
    }
}
