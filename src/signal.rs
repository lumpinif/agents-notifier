use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const SIGNAL_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Signal {
    pub schema_version: u32,
    pub id: String,
    pub source_id: String,
    pub source_type: String,
    pub title: String,
    pub body: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: BTreeMap<String, String>,
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
        Self {
            schema_version: SIGNAL_SCHEMA_VERSION,
            id: id.into(),
            source_id: source_id.into(),
            source_type: source_type.into(),
            title: title.into(),
            body: body.into(),
            timestamp,
            metadata,
        }
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
            "Codex finished a job.",
            timestamp,
            metadata,
        );

        assert_eq!(signal.schema_version, 1);
        assert_eq!(signal.id, "signal-1");
        assert_eq!(signal.source_id, "codex_desktop");
        assert_eq!(signal.source_type, "codex_desktop");
        assert_eq!(signal.title, "Codex");
        assert_eq!(signal.body, "Codex finished a job.");
        assert_eq!(signal.metadata.get("turn_id"), Some(&"turn-1".to_string()));
    }

    #[test]
    fn serializes_stable_webhook_payload_shape() {
        let timestamp = DateTime::parse_from_rfc3339("2026-05-08T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let signal = Signal::new_with_timestamp(
            "signal-1",
            "codex_cli",
            "codex_cli",
            "Codex",
            "Codex finished a job.",
            timestamp,
            BTreeMap::new(),
        );

        let json = serde_json::to_value(&signal).expect("signal should serialize");

        assert_eq!(
            json,
            serde_json::json!({
                "schema_version": 1,
                "id": "signal-1",
                "source_id": "codex_cli",
                "source_type": "codex_cli",
                "title": "Codex",
                "body": "Codex finished a job.",
                "timestamp": "2026-05-08T12:00:00Z",
                "metadata": {}
            })
        );
    }
}
