use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SIGNAL_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Signal {
    pub schema_version: u32,
    pub id: String,
    pub source: SignalSource,
    pub event: SignalEvent,
    pub display: SignalDisplay,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<SignalWorkspace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<SignalConversation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<SignalLifecycle>,
    pub links: Vec<SignalLink>,
    pub timestamp: DateTime<Utc>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalSource {
    pub id: String,
    pub source_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalEvent {
    pub kind: SignalEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignalEventKind {
    TurnCompleted,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalDisplay {
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalWorkspace {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalConversation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<SignalAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalAnswer {
    pub kind: SignalAnswerKind,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignalAnswerKind {
    Preview,
    Full,
}

impl SignalAnswerKind {
    pub fn display_label(self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::Full => "Answer",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalLifecycle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SignalLifecycleStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignalLifecycleStatus {
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignalLink {
    pub label: String,
    pub url: String,
}

impl Signal {
    pub fn new(
        source_id: impl Into<String>,
        source_type: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
        metadata: BTreeMap<String, String>,
    ) -> Self {
        Self::new_with_timestamp(
            Uuid::new_v4().to_string(),
            source_id,
            source_type,
            title,
            body,
            Utc::now(),
            metadata,
        )
    }

    pub fn new_with_timestamp(
        id: impl Into<String>,
        source_id: impl Into<String>,
        source_type: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
        timestamp: DateTime<Utc>,
        metadata: BTreeMap<String, String>,
    ) -> Self {
        let source_id = source_id.into();
        let source_type = source_type.into();
        let title = title.into();
        let body = body.into();

        Self {
            schema_version: SIGNAL_SCHEMA_VERSION,
            id: id.into(),
            source: SignalSource {
                id: source_id,
                source_type,
            },
            event: SignalEvent {
                kind: SignalEventKind::Custom,
                raw_name: None,
            },
            display: SignalDisplay {
                title,
                summary: body,
            },
            workspace: None,
            conversation: None,
            lifecycle: None,
            links: Vec::new(),
            timestamp,
            metadata,
        }
    }

    pub fn new_structured_with_timestamp(
        id: impl Into<String>,
        source: SignalSource,
        event: SignalEvent,
        display: SignalDisplay,
        timestamp: DateTime<Utc>,
        metadata: BTreeMap<String, String>,
    ) -> Self {
        Self {
            schema_version: SIGNAL_SCHEMA_VERSION,
            id: id.into(),
            source,
            event,
            display,
            workspace: None,
            conversation: None,
            lifecycle: None,
            links: Vec::new(),
            timestamp,
            metadata,
        }
    }

    pub fn source_id(&self) -> &str {
        &self.source.id
    }

    pub fn source_type(&self) -> &str {
        &self.source.source_type
    }

    pub fn title(&self) -> &str {
        &self.display.title
    }

    pub fn summary(&self) -> &str {
        &self.display.summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_signal_with_schema_version_and_metadata() {
        let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut metadata = BTreeMap::new();
        metadata.insert("turn_id".to_string(), "turn-1".to_string());

        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_desktop",
            "codex_desktop",
            "Codex",
            "Ready for review.",
            timestamp,
            metadata,
        );

        assert_eq!(signal.schema_version, 2);
        assert_eq!(signal.id, "signal-1");
        assert_eq!(signal.source.id, "codex_desktop");
        assert_eq!(signal.source.source_type, "codex_desktop");
        assert_eq!(signal.event.kind, SignalEventKind::Custom);
        assert_eq!(signal.display.title, "Codex");
        assert_eq!(signal.display.summary, "Ready for review.");
        assert_eq!(signal.metadata.get("turn_id"), Some(&"turn-1".to_string()));
    }

    #[test]
    fn serializes_stable_signal_v2_payload_shape() {
        let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            "Ready for review.",
            timestamp,
            BTreeMap::new(),
        );

        let json = serde_json::to_value(&signal).expect("signal should serialize");

        assert_eq!(
            json,
            serde_json::json!({
                "schema_version": 2,
                "id": "signal-1",
                "source": {
                    "id": "codex_cli",
                    "source_type": "codex_cli"
                },
                "event": {
                    "kind": "custom"
                },
                "display": {
                    "title": "Codex",
                    "summary": "Ready for review."
                },
                "links": [],
                "timestamp": "2026-05-08T12:00:00Z",
                "metadata": {}
            })
        );
        assert!(json.get("title").is_none());
        assert!(json.get("body").is_none());
        assert!(json.get("source_id").is_none());
        assert!(json.get("source_type").is_none());
    }
}
