//! Mandate lifecycle event builders (CloudEvents).
//!
//! Builds `assay.mandate.used.v1` and `assay.mandate.revoked.v1` events
//! per SPEC-Mandate-v1.0.4.
//!
//! Key design:
//! - CloudEvents.id = use_id (deterministic) for idempotent retries
//! - source = configured event_source (validated at startup)
//! - time = consumed_at from receipt

use crate::runtime::AuthzReceipt;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;

/// CloudEvents type for mandate consumption.
pub const EVENT_TYPE_USED: &str = "assay.mandate.used.v1";
/// CloudEvents type for mandate revocation.
pub const EVENT_TYPE_REVOKED: &str = "assay.mandate.revoked.v1";

/// A mandate lifecycle event (CloudEvents compliant).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub specversion: &'static str,
    /// CloudEvents.id = use_id for idempotent deduplication
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub source: String,
    pub time: String,
    pub datacontenttype: &'static str,
    pub data: Value,
}

/// Build a `assay.mandate.used.v1` CloudEvent from an AuthzReceipt.
///
/// Key properties:
/// - `id` = `receipt.use_id` (deterministic, idempotent)
/// - `time` = `receipt.consumed_at`
/// - `data.use_id` = `receipt.use_id`
pub fn mandate_used_event(source: &str, receipt: &AuthzReceipt) -> LifecycleEvent {
    LifecycleEvent {
        specversion: "1.0",
        id: receipt.use_id.clone(), // Idempotent: same use_id = same event
        event_type: EVENT_TYPE_USED.to_string(),
        source: source.to_string(),
        time: receipt.consumed_at.to_rfc3339(),
        datacontenttype: "application/json",
        data: serde_json::json!({
            "mandate_id": receipt.mandate_id,
            "use_id": receipt.use_id,
            "tool_call_id": receipt.tool_call_id,
            "consumed_at": receipt.consumed_at.to_rfc3339(),
            "use_count": receipt.use_count,
        }),
    }
}

/// Build a `assay.mandate.revoked.v1` CloudEvent.
pub fn mandate_revoked_event(
    source: &str,
    mandate_id: &str,
    revoked_at: DateTime<Utc>,
    reason: Option<&str>,
    revoked_by: Option<&str>,
    event_id: Option<&str>,
) -> LifecycleEvent {
    // Use provided event_id or generate deterministic one
    let id = event_id
        .map(String::from)
        .unwrap_or_else(|| format!("revoke:{}", mandate_id));

    LifecycleEvent {
        specversion: "1.0",
        id,
        event_type: EVENT_TYPE_REVOKED.to_string(),
        source: source.to_string(),
        time: revoked_at.to_rfc3339(),
        datacontenttype: "application/json",
        data: serde_json::json!({
            "mandate_id": mandate_id,
            "revoked_at": revoked_at.to_rfc3339(),
            "reason": reason,
            "revoked_by": revoked_by,
        }),
    }
}

/// Trait for emitting lifecycle events.
pub trait LifecycleEmitter: Send + Sync {
    /// Emit a lifecycle event.
    fn emit(&self, event: &LifecycleEvent);
}

/// File-based lifecycle emitter (NDJSON to audit log).
pub struct FileLifecycleEmitter {
    file: std::sync::Mutex<std::fs::File>,
}

impl FileLifecycleEmitter {
    /// Create a new file emitter.
    pub fn new(path: &std::path::Path) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            file: std::sync::Mutex::new(file),
        })
    }
}

impl LifecycleEmitter for FileLifecycleEmitter {
    fn emit(&self, event: &LifecycleEvent) {
        if let Ok(json) = serde_json::to_string(event) {
            if let Ok(mut f) = self.file.lock() {
                let _ = writeln!(f, "{}", json);
            }
        }
    }
}

/// Null emitter for testing or when audit logging is disabled.
pub struct NullLifecycleEmitter;

impl LifecycleEmitter for NullLifecycleEmitter {
    fn emit(&self, _event: &LifecycleEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_used_event_id_is_use_id() {
        let receipt = AuthzReceipt {
            mandate_id: "sha256:mandate123".to_string(),
            use_id: "sha256:deterministic_use_id".to_string(),
            use_count: 1,
            consumed_at: Utc::now(),
            tool_call_id: "tc_001".to_string(),
            was_new: true,
        };

        let event = mandate_used_event("assay://myorg/myapp", &receipt);

        // Critical: CloudEvents.id == use_id for idempotency
        assert_eq!(event.id, receipt.use_id);
        assert_eq!(event.event_type, EVENT_TYPE_USED);
        assert_eq!(event.source, "assay://myorg/myapp");
    }

    #[test]
    fn test_used_event_contains_required_fields() {
        let receipt = AuthzReceipt {
            mandate_id: "sha256:m".to_string(),
            use_id: "sha256:u".to_string(),
            use_count: 3,
            consumed_at: Utc::now(),
            tool_call_id: "tc".to_string(),
            was_new: true,
        };

        let event = mandate_used_event("assay://test", &receipt);

        // Check data fields
        assert_eq!(event.data["mandate_id"], "sha256:m");
        assert_eq!(event.data["use_id"], "sha256:u");
        assert_eq!(event.data["tool_call_id"], "tc");
        assert_eq!(event.data["use_count"], 3);
    }

    #[test]
    fn test_revoked_event_structure() {
        let event = mandate_revoked_event(
            "assay://myorg/myapp",
            "sha256:mandate456",
            Utc::now(),
            Some("User requested"),
            Some("admin@example.com"),
            Some("evt_revoke_001"),
        );

        assert_eq!(event.id, "evt_revoke_001");
        assert_eq!(event.event_type, EVENT_TYPE_REVOKED);
        assert_eq!(event.data["mandate_id"], "sha256:mandate456");
        assert_eq!(event.data["reason"], "User requested");
    }

    #[test]
    fn test_used_event_serialization() {
        let receipt = AuthzReceipt {
            mandate_id: "sha256:m".to_string(),
            use_id: "sha256:u".to_string(),
            use_count: 1,
            consumed_at: Utc::now(),
            tool_call_id: "tc".to_string(),
            was_new: true,
        };

        let event = mandate_used_event("assay://test", &receipt);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("assay.mandate.used.v1"));
        assert!(json.contains("sha256:u"));
        assert!(json.contains("specversion"));
    }
}
