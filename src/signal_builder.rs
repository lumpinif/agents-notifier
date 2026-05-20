use std::collections::BTreeMap;

use anyhow::{Context, anyhow};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::config::{AnswerDetail, PromptDetail, SourceType, ValidatedConfig};
use crate::signal::{
    Signal, SignalAnswer, SignalAnswerKind, SignalConversation, SignalDisplay, SignalEvent,
    SignalEventKind, SignalLifecycle, SignalLink, SignalSource, SignalWorkspace,
};

const PREVIEW_LIMIT_CHARS: usize = 360;

#[derive(Debug, Clone)]
pub(crate) struct SignalDraft {
    pub source_id: String,
    pub expected_source_type: SourceType,
    pub display: SignalDisplay,
    pub event: SignalEvent,
    pub workspace: Option<SignalWorkspace>,
    pub conversation: Option<SignalConversationDraft>,
    pub lifecycle: Option<SignalLifecycle>,
    pub links: Vec<SignalLink>,
    pub timestamp: Option<DateTime<Utc>>,
    pub metadata: BTreeMap<String, String>,
}

impl SignalDraft {
    pub(crate) fn new(
        source_id: impl Into<String>,
        expected_source_type: SourceType,
        title: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            expected_source_type,
            display: SignalDisplay {
                title: title.into(),
                summary: summary.into(),
            },
            event: SignalEvent {
                kind: SignalEventKind::Custom,
                raw_name: None,
            },
            workspace: None,
            conversation: None,
            lifecycle: None,
            links: Vec::new(),
            timestamp: None,
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SignalConversationDraft {
    pub session_id: Option<String>,
    pub session_title: Option<String>,
    pub turn_id: Option<String>,
    pub prompt: Option<String>,
    pub answer: Option<String>,
    pub model: Option<String>,
}

pub(crate) struct SignalBuilder<'a> {
    config: &'a ValidatedConfig,
}

impl<'a> SignalBuilder<'a> {
    pub(crate) fn new(config: &'a ValidatedConfig) -> Self {
        Self { config }
    }

    pub(crate) fn build(&self, draft: SignalDraft) -> anyhow::Result<Signal> {
        let source = self
            .config
            .source(&draft.source_id)
            .with_context(|| format!("source `{}` is not configured", draft.source_id))?;

        if source.source_type != draft.expected_source_type {
            return Err(anyhow!(
                "source `{}` has type `{}`; expected `{}`",
                source.id,
                source.source_type.as_str(),
                draft.expected_source_type.as_str()
            ));
        }

        let mut signal = Signal::new_structured_with_timestamp(
            Uuid::new_v4().to_string(),
            SignalSource {
                id: source.id.clone(),
                source_type: source.source_type.as_str().to_string(),
            },
            draft.event,
            draft.display,
            draft.timestamp.unwrap_or_else(Utc::now),
            draft.metadata,
        );
        signal.workspace = draft.workspace;
        signal.lifecycle = draft.lifecycle;
        signal.links = draft.links;
        signal.conversation = draft.conversation.map(|conversation| SignalConversation {
            session_id: conversation.session_id,
            session_title: conversation.session_title,
            turn_id: conversation.turn_id,
            prompt: prompt_for_config(conversation.prompt, self.config.notification.prompt_detail),
            answer: answer_for_config(conversation.answer, self.config.notification.answer_detail),
            model: conversation.model,
        });

        Ok(signal)
    }
}

fn prompt_for_config(prompt: Option<String>, prompt_detail: PromptDetail) -> Option<String> {
    if prompt_detail != PromptDetail::On {
        return None;
    }

    prompt.and_then(present_owned)
}

fn answer_for_config(answer: Option<String>, answer_detail: AnswerDetail) -> Option<SignalAnswer> {
    let answer = answer.and_then(present_owned)?;
    let (kind, content) = match answer_detail {
        AnswerDetail::Preview => (SignalAnswerKind::Preview, preview_text(&answer)),
        AnswerDetail::Full => (SignalAnswerKind::Full, answer),
    };

    if content.is_empty() {
        None
    } else {
        Some(SignalAnswer { kind, content })
    }
}

fn preview_text(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&compact, PREVIEW_LIMIT_CHARS)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn present_owned(value: String) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        CliConfig, LogConfig, NotificationConfig, ProviderType, RawConfig, RawProviderConfig,
        RouteConfig, SourceConfig,
    };

    #[test]
    fn rejects_missing_source() {
        let config = test_config(
            "codex_cli",
            SourceType::CodexCli,
            NotificationConfig::default(),
        );
        let draft = SignalDraft::new("missing", SourceType::CodexCli, "Codex", "Body");

        let err = SignalBuilder::new(&config)
            .build(draft)
            .expect_err("missing source should fail");

        assert!(
            err.to_string()
                .contains("source `missing` is not configured")
        );
    }

    #[test]
    fn rejects_source_type_mismatch() {
        let config = test_config(
            "claude_code",
            SourceType::ClaudeCode,
            NotificationConfig::default(),
        );
        let draft = SignalDraft::new("claude_code", SourceType::CodexCli, "Codex", "Body");

        let err = SignalBuilder::new(&config)
            .build(draft)
            .expect_err("wrong source type should fail");

        assert!(
            err.to_string()
                .contains("source `claude_code` has type `claude_code`; expected `codex_cli`")
        );
    }

    #[test]
    fn applies_prompt_and_answer_detail_policy() {
        let config = test_config(
            "codex_cli",
            SourceType::CodexCli,
            NotificationConfig {
                answer_detail: AnswerDetail::Preview,
                prompt_detail: PromptDetail::Off,
            },
        );
        let mut draft = SignalDraft::new("codex_cli", SourceType::CodexCli, "Codex", "Done");
        draft.conversation = Some(SignalConversationDraft {
            prompt: Some("Do not include this prompt.".to_string()),
            answer: Some(format!("{} done", "word ".repeat(120))),
            model: Some("gpt-5.2-codex".to_string()),
            ..SignalConversationDraft::default()
        });

        let signal = SignalBuilder::new(&config)
            .build(draft)
            .expect("draft should build");
        let conversation = signal.conversation.expect("conversation should be present");
        let answer = conversation.answer.expect("answer should be present");

        assert_eq!(conversation.prompt, None);
        assert_eq!(conversation.model.as_deref(), Some("gpt-5.2-codex"));
        assert_eq!(answer.kind, SignalAnswerKind::Preview);
        assert!(answer.content.chars().count() <= PREVIEW_LIMIT_CHARS + 3);
        assert!(answer.content.ends_with("..."));
    }

    #[test]
    fn keeps_prompt_and_full_answer_when_configured() {
        let config = test_config(
            "gemini_cli",
            SourceType::AgentHook,
            NotificationConfig {
                answer_detail: AnswerDetail::Full,
                prompt_detail: PromptDetail::On,
            },
        );
        let mut draft = SignalDraft::new("gemini_cli", SourceType::AgentHook, "Gemini CLI", "Done");
        draft.conversation = Some(SignalConversationDraft {
            prompt: Some(" Review this patch. ".to_string()),
            answer: Some(" Fixed the route.\n\nNext, run tests. ".to_string()),
            ..SignalConversationDraft::default()
        });

        let signal = SignalBuilder::new(&config)
            .build(draft)
            .expect("draft should build");
        let conversation = signal.conversation.expect("conversation should be present");
        let answer = conversation.answer.expect("answer should be present");

        assert_eq!(conversation.prompt.as_deref(), Some("Review this patch."));
        assert_eq!(answer.kind, SignalAnswerKind::Full);
        assert_eq!(answer.content, "Fixed the route.\n\nNext, run tests.");
    }

    #[test]
    fn passes_structured_fields_through() {
        let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let config = test_config(
            "aider",
            SourceType::AgentHook,
            NotificationConfig::default(),
        );
        let mut draft = SignalDraft::new("aider", SourceType::AgentHook, "Aider", "Done");
        draft.timestamp = Some(timestamp);
        draft.workspace = Some(SignalWorkspace {
            cwd: Some("/repo".to_string()),
            project_name: Some("repo".to_string()),
            project_path: Some("/repo".to_string()),
            branch: Some("main".to_string()),
            worktree: None,
        });
        draft.links.push(SignalLink {
            label: "Open".to_string(),
            url: "file:///repo".to_string(),
        });
        draft
            .metadata
            .insert("safe_key".to_string(), "value".to_string());

        let signal = SignalBuilder::new(&config)
            .build(draft)
            .expect("draft should build");

        assert_eq!(signal.timestamp, timestamp);
        assert_eq!(
            signal
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.project_name.as_deref()),
            Some("repo")
        );
        assert_eq!(signal.links.len(), 1);
        assert_eq!(
            signal.metadata.get("safe_key").map(String::as_str),
            Some("value")
        );
    }

    fn test_config(
        source_id: &str,
        source_type: SourceType,
        notification: NotificationConfig,
    ) -> ValidatedConfig {
        let mut provider = RawProviderConfig::new("debug", ProviderType::Webhook);
        provider.url = Some("https://example.com/hook".to_string());

        RawConfig {
            schema_version: 1,
            cli: CliConfig::default(),
            log: LogConfig::default(),
            notification,
            sources: vec![SourceConfig {
                id: source_id.to_string(),
                source_type,
            }],
            providers: vec![provider],
            routes: vec![RouteConfig::new(
                vec![source_id.to_string()],
                vec!["debug".to_string()],
            )],
        }
        .validate()
        .expect("test config should validate")
    }
}
