//! Mandate CloudEvents (SPEC-Mandate-v1 §3)
//!
//! CloudEvents envelopes for mandate grant and lifecycle events.
//!
//! # Event Types
//!
//! | Type | Purpose |
//! |------|---------|
//! | `assay.mandate.v1` | Mandate grant |
//! | `assay.mandate.used.v1` | Consumption receipt |
//! | `assay.mandate.revoked.v1` | Revocation |

use crate::mandate::types::{Mandate, Signature};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::Digest;

/// CloudEvents type for mandate grant.
pub const EVENT_TYPE_MANDATE: &str = "assay.mandate.v1";

/// CloudEvents type for mandate consumption.
pub const EVENT_TYPE_MANDATE_USED: &str = "assay.mandate.used.v1";

/// CloudEvents type for mandate revocation.
pub const EVENT_TYPE_MANDATE_REVOKED: &str = "assay.mandate.revoked.v1";

/// CloudEvents envelope for mandate events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MandateEvent<T> {
    /// CloudEvents spec version (always "1.0")
    pub specversion: String,

    /// Unique event ID
    pub id: String,

    /// Event type (e.g., "assay.mandate.v1")
    #[serde(rename = "type")]
    pub type_: String,

    /// Event source URI
    pub source: String,

    /// Event timestamp
    pub time: DateTime<Utc>,

    /// Content type (always "application/json")
    pub datacontenttype: String,

    /// Event payload
    pub data: T,
}

impl<T> MandateEvent<T> {
    /// Create a new mandate event.
    pub fn new(
        id: impl Into<String>,
        type_: impl Into<String>,
        source: impl Into<String>,
        data: T,
    ) -> Self {
        Self {
            specversion: "1.0".to_string(),
            id: id.into(),
            type_: type_.into(),
            source: source.into(),
            time: Utc::now(),
            datacontenttype: "application/json".to_string(),
            data,
        }
    }

    /// Set explicit timestamp.
    pub fn with_time(mut self, time: DateTime<Utc>) -> Self {
        self.time = time;
        self
    }
}

/// Create a mandate grant event.
pub fn mandate_event(
    id: impl Into<String>,
    source: impl Into<String>,
    mandate: Mandate,
) -> MandateEvent<Mandate> {
    MandateEvent::new(id, EVENT_TYPE_MANDATE, source, mandate)
}

/// Mandate consumption receipt payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MandateUsedPayload {
    /// Reference to consumed mandate
    pub mandate_id: String,

    /// Unique identifier for this use
    pub use_id: String,

    /// Tool call that consumed the mandate
    pub tool_call_id: String,

    /// When consumption occurred
    pub consumed_at: DateTime<Utc>,

    /// Ordinal use number (1-indexed)
    pub use_count: u32,

    /// Optional signature for high-risk deployments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
}

impl MandateUsedPayload {
    /// Create a new usage receipt.
    pub fn new(
        mandate_id: impl Into<String>,
        tool_call_id: impl Into<String>,
        use_count: u32,
    ) -> Self {
        let mandate_id = mandate_id.into();
        let tool_call_id = tool_call_id.into();
        let use_id = format!(
            "sha256:{}",
            hex::encode(sha2::Sha256::digest(
                format!("{}:{}:{}", mandate_id, tool_call_id, use_count).as_bytes()
            ))
        );

        Self {
            mandate_id,
            use_id,
            tool_call_id,
            consumed_at: Utc::now(),
            use_count,
            signature: None,
        }
    }

    /// Set explicit consumed_at timestamp.
    pub fn with_consumed_at(mut self, consumed_at: DateTime<Utc>) -> Self {
        self.consumed_at = consumed_at;
        self
    }
}

/// Create a mandate used event.
pub fn mandate_used_event(
    id: impl Into<String>,
    source: impl Into<String>,
    payload: MandateUsedPayload,
) -> MandateEvent<MandateUsedPayload> {
    MandateEvent::new(id, EVENT_TYPE_MANDATE_USED, source, payload)
}

/// Revocation reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RevocationReason {
    /// User explicitly revoked
    #[default]
    UserRequested,
    /// Administrative action
    AdminOverride,
    /// Automated policy enforcement
    PolicyViolation,
    /// Voluntary early expiration
    ExpiredEarly,
}

/// Mandate revocation payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MandateRevokedPayload {
    /// Mandate being revoked
    pub mandate_id: String,

    /// Effective revocation time
    pub revoked_at: DateTime<Utc>,

    /// Revocation reason
    pub reason: RevocationReason,

    /// Subject who revoked
    pub revoked_by: String,

    /// Optional signature for high-risk deployments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Signature>,
}

impl MandateRevokedPayload {
    /// Create a new revocation payload.
    pub fn new(
        mandate_id: impl Into<String>,
        reason: RevocationReason,
        revoked_by: impl Into<String>,
    ) -> Self {
        Self {
            mandate_id: mandate_id.into(),
            revoked_at: Utc::now(),
            reason,
            revoked_by: revoked_by.into(),
            signature: None,
        }
    }

    /// Set explicit revoked_at timestamp.
    pub fn with_revoked_at(mut self, revoked_at: DateTime<Utc>) -> Self {
        self.revoked_at = revoked_at;
        self
    }
}

/// Create a mandate revoked event.
pub fn mandate_revoked_event(
    id: impl Into<String>,
    source: impl Into<String>,
    payload: MandateRevokedPayload,
) -> MandateEvent<MandateRevokedPayload> {
    MandateEvent::new(id, EVENT_TYPE_MANDATE_REVOKED, source, payload)
}

/// Extended tool decision payload with mandate linkage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDecisionWithMandate {
    /// Tool name
    pub tool: String,

    /// Decision (allow/deny)
    pub decision: String,

    /// Reason code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,

    /// Args schema hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args_schema_hash: Option<String>,

    /// Tool call ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,

    /// Mandate authorizing this decision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mandate_id: Option<String>,

    /// Whether tool matched mandate scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mandate_scope_match: Option<bool>,

    /// Whether mandate kind allows operation class
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mandate_kind_match: Option<bool>,
}

impl ToolDecisionWithMandate {
    /// Create a new tool decision.
    pub fn new(tool: impl Into<String>, decision: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            decision: decision.into(),
            reason_code: None,
            args_schema_hash: None,
            tool_call_id: None,
            mandate_id: None,
            mandate_scope_match: None,
            mandate_kind_match: None,
        }
    }

    /// Link to a mandate.
    pub fn with_mandate(
        mut self,
        mandate_id: impl Into<String>,
        scope_match: bool,
        kind_match: bool,
    ) -> Self {
        self.mandate_id = Some(mandate_id.into());
        self.mandate_scope_match = Some(scope_match);
        self.mandate_kind_match = Some(kind_match);
        self
    }

    /// Set reason code.
    pub fn with_reason_code(mut self, reason_code: impl Into<String>) -> Self {
        self.reason_code = Some(reason_code.into());
        self
    }

    /// Set tool call ID.
    pub fn with_tool_call_id(mut self, tool_call_id: impl Into<String>) -> Self {
        self.tool_call_id = Some(tool_call_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mandate::types::*;
    use chrono::TimeZone;

    fn create_test_mandate() -> Mandate {
        Mandate {
            mandate_id: "sha256:test123".to_string(),
            mandate_kind: MandateKind::Intent,
            principal: Principal::new("user-123", AuthMethod::Oidc),
            scope: Scope::new(vec!["search_*".to_string()]),
            validity: Validity::at(Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap()),
            constraints: Constraints::default(),
            context: Context::new("myorg/app", "auth.myorg.com"),
            signature: None,
        }
    }

    #[test]
    fn test_mandate_event() {
        let mandate = create_test_mandate();
        let event = mandate_event("evt_001", "assay://myorg/app", mandate);

        assert_eq!(event.specversion, "1.0");
        assert_eq!(event.type_, EVENT_TYPE_MANDATE);
        assert_eq!(event.datacontenttype, "application/json");
        assert_eq!(event.data.mandate_id, "sha256:test123");
    }

    #[test]
    fn test_mandate_used_event() {
        let payload = MandateUsedPayload::new("sha256:mandate123", "tc_001", 1);
        let event = mandate_used_event("evt_use001", "assay://myorg/app", payload);

        assert_eq!(event.type_, EVENT_TYPE_MANDATE_USED);
        assert_eq!(event.data.mandate_id, "sha256:mandate123");
        assert_eq!(event.data.use_count, 1);
        assert!(event.data.use_id.starts_with("sha256:"));
    }

    #[test]
    fn test_mandate_revoked_event() {
        let payload = MandateRevokedPayload::new(
            "sha256:mandate123",
            RevocationReason::UserRequested,
            "user-123",
        );
        let event = mandate_revoked_event("evt_rev001", "assay://myorg/app", payload);

        assert_eq!(event.type_, EVENT_TYPE_MANDATE_REVOKED);
        assert_eq!(event.data.mandate_id, "sha256:mandate123");
        assert_eq!(event.data.reason, RevocationReason::UserRequested);
    }

    #[test]
    fn test_tool_decision_with_mandate() {
        let decision = ToolDecisionWithMandate::new("purchase_item", "allow")
            .with_mandate("sha256:mandate123", true, true)
            .with_reason_code("P_MANDATE_VALID")
            .with_tool_call_id("tc_001");

        assert_eq!(decision.tool, "purchase_item");
        assert_eq!(decision.decision, "allow");
        assert_eq!(decision.mandate_id, Some("sha256:mandate123".to_string()));
        assert_eq!(decision.mandate_scope_match, Some(true));
        assert_eq!(decision.mandate_kind_match, Some(true));
    }

    #[test]
    fn test_revocation_reason_serialization() {
        assert_eq!(
            serde_json::to_string(&RevocationReason::UserRequested).unwrap(),
            "\"user_requested\""
        );
        assert_eq!(
            serde_json::to_string(&RevocationReason::AdminOverride).unwrap(),
            "\"admin_override\""
        );
        assert_eq!(
            serde_json::to_string(&RevocationReason::PolicyViolation).unwrap(),
            "\"policy_violation\""
        );
        assert_eq!(
            serde_json::to_string(&RevocationReason::ExpiredEarly).unwrap(),
            "\"expired_early\""
        );
    }

    #[test]
    fn test_event_serialization() {
        let mandate = create_test_mandate();
        let event = mandate_event("evt_001", "assay://myorg/app", mandate);

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"specversion\":\"1.0\""));
        assert!(json.contains("\"type\":\"assay.mandate.v1\""));
        assert!(json.contains("\"datacontenttype\":\"application/json\""));
    }
}
