//! Evidence Contract v1 Types
//!
//! CloudEvents-compatible envelope for Assay Evidence.
//! Designed for auditability, determinism, and interoperability.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// CloudEvents specversion used by Evidence Contract v1 envelopes.
pub const CE_SPECVERSION: &str = "1.0";

/// Assay Evidence Spec version implemented by this crate.
pub const ASSAY_EVIDENCE_SPEC_VERSION: &str = "1.0";

/// Backward-compatible alias for the CloudEvents specversion.
///
/// New code should prefer `CE_SPECVERSION` when filling the CloudEvents
/// envelope and `ASSAY_EVIDENCE_SPEC_VERSION` when referring to Assay's
/// own evidence contract version.
pub const SPEC_VERSION: &str = CE_SPECVERSION;

/// Alias for clearer semantics
pub type Envelope = EvidenceEvent;

/// Producer metadata for manifest and provenance tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProducerMeta {
    /// Producer name (e.g., "assay-cli")
    pub name: String,
    /// Semantic version (e.g., "2.6.0")
    pub version: String,
    /// Git commit SHA (short or full)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
}

impl ProducerMeta {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            git: None,
        }
    }

    pub fn with_git(mut self, git: impl Into<String>) -> Self {
        self.git = Some(git.into());
        self
    }

    /// Format as single string: "name/version (git)"
    pub fn to_string_compact(&self) -> String {
        match &self.git {
            Some(g) => format!("{}/{} ({})", self.name, self.version, g),
            None => format!("{}/{}", self.name, self.version),
        }
    }
}

impl Default for ProducerMeta {
    fn default() -> Self {
        Self {
            name: "assay".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            git: option_env!("ASSAY_GIT_SHA").map(String::from),
        }
    }
}

/// CloudEvents envelope for Assay Evidence (v1.0 compliant).
///
/// Designed for maximum interoperability:
/// - Flat extensions (OTel alignment)
/// - Deterministic serialization (JCS)
/// - Content-addressed hashing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceEvent {
    /// CloudEvents spec version (fixed "1.0")
    pub specversion: String,

    /// Event type (dot-separated identifier, e.g., "assay.env.filtered")
    #[serde(rename = "type")]
    pub type_: String,

    /// Source URI (Producer ID, e.g., "urn:assay:cli")
    pub source: String,

    /// Stream Identity: `{run_id}:{seq}` (Unique per Source)
    pub id: String,

    /// Timestamp: RFC3339 UTC
    pub time: DateTime<Utc>,

    /// Content Type (fixed "application/json")
    #[serde(rename = "datacontenttype")]
    pub data_content_type: String,

    /// Subject (Optional) - e.g. tool name or resource path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,

    // -- OTel Extensions --
    /// W3C Trace Parent
    #[serde(skip_serializing_if = "Option::is_none", rename = "traceparent")]
    pub trace_parent: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "tracestate")]
    pub trace_state: Option<String>,

    // -- Assay Context Extensions (Flattened) --
    /// Run identifier (deterministic or UUIDv7)
    #[serde(rename = "assayrunid")]
    pub run_id: String,

    /// Sequence number within run (0-indexed, contiguous)
    #[serde(rename = "assayseq")]
    pub seq: u64,

    /// Producer name (e.g., "assay-cli")
    #[serde(rename = "assayproducer")]
    pub producer: String,

    /// Producer version (e.g., "2.6.0")
    #[serde(rename = "assayproducerversion")]
    pub producer_version: String,

    /// Git commit SHA
    #[serde(rename = "assaygit")]
    pub git_sha: String,

    /// Policy ID (hash of policy file)
    #[serde(skip_serializing_if = "Option::is_none", rename = "assaypolicyid")]
    pub policy_id: Option<String>,

    /// Privacy flag: contains PII
    #[serde(rename = "assaypii")]
    pub contains_pii: bool,

    /// Privacy flag: contains secrets
    #[serde(rename = "assaysecrets")]
    pub contains_secrets: bool,

    #[serde(rename = "assaycontenthash")]
    pub content_hash: Option<String>,

    #[serde(rename = "data")]
    pub payload: serde_json::Value,
}

impl EvidenceEvent {
    /// Create a new event with required fields.
    ///
    /// Note: `content_hash` will be None; call `compute_content_hash()` or
    /// let `BundleWriter` normalize it before export.
    pub fn new(
        type_: impl Into<String>,
        source: impl Into<String>,
        run_id: impl Into<String>,
        seq: u64,
        payload: serde_json::Value,
    ) -> Self {
        let run_id = run_id.into();
        Self {
            specversion: CE_SPECVERSION.into(),
            type_: type_.into(),
            source: source.into(),
            id: format!("{}:{}", run_id, seq),
            time: Utc::now(),
            data_content_type: "application/json".into(),
            subject: None,
            trace_parent: None,
            trace_state: None,
            run_id,
            seq,
            producer: "assay".into(),
            producer_version: env!("CARGO_PKG_VERSION").into(),
            git_sha: option_env!("ASSAY_GIT_SHA").unwrap_or("unknown").into(),
            policy_id: None,
            contains_pii: false,
            contains_secrets: false,
            content_hash: None,
            payload,
        }
    }

    /// Set subject
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Set producer metadata
    pub fn with_producer(mut self, meta: &ProducerMeta) -> Self {
        self.producer = meta.name.clone();
        self.producer_version = meta.version.clone();
        self.git_sha = meta.git.clone().unwrap_or_else(|| "unknown".into());
        self
    }

    /// Set explicit timestamp (for deterministic export)
    pub fn with_time(mut self, time: DateTime<Utc>) -> Self {
        self.time = time;
        self
    }

    /// Set policy ID
    pub fn with_policy_id(mut self, policy_id: impl Into<String>) -> Self {
        self.policy_id = Some(policy_id.into());
        self
    }

    /// Set privacy flags
    pub fn with_privacy(mut self, contains_pii: bool, contains_secrets: bool) -> Self {
        self.contains_pii = contains_pii;
        self.contains_secrets = contains_secrets;
        self
    }

    /// Set trace context (OTel)
    pub fn with_trace(mut self, parent: impl Into<String>) -> Self {
        self.trace_parent = Some(parent.into());
        self
    }

    /// Extract ProducerMeta from this event
    pub fn producer_meta(&self) -> ProducerMeta {
        ProducerMeta {
            name: self.producer.clone(),
            version: self.producer_version.clone(),
            git: if self.git_sha == "unknown" {
                None
            } else {
                Some(self.git_sha.clone())
            },
        }
    }
}

// -- Strongly Typed Payload Helpers --

/// Typed payload variants (for convenience, not enforced by contract)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum Payload {
    #[serde(rename = "assay.env.filtered")]
    EnvFiltered(PayloadEnvFiltered),
    #[serde(rename = "assay.tool.decision")]
    ToolDecision(PayloadToolDecision),
    #[serde(rename = "assay.exec.observed")]
    ExecObserved(PayloadExecObserved),
    #[serde(rename = "assay.sandbox.degraded")]
    SandboxDegraded(PayloadSandboxDegraded),
    #[serde(rename = "assay.profile.started")]
    ProfileStarted(PayloadProfileStarted),
    #[serde(rename = "assay.profile.finished")]
    ProfileFinished(PayloadProfileFinished),
    #[serde(rename = "assay.policy.suggested")]
    PolicySuggested(PayloadPolicySuggested),
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadEnvFiltered {
    pub mode: String,
    pub passed_keys: Vec<String>,
    pub dropped_keys: Vec<String>,
    pub counters: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadToolDecision {
    pub tool: String,
    pub decision: String,
    pub reason_code: Option<String>,
    pub args_schema_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegated_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_depth: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadExecObserved {
    pub argv0: String,
    pub args_hash: String,
    pub env_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxDegradationReasonCode {
    BackendUnavailable,
    PolicyConflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxDegradationMode {
    AuditFallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxDegradationComponent {
    Landlock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PayloadSandboxDegraded {
    pub reason_code: SandboxDegradationReasonCode,
    pub degradation_mode: SandboxDegradationMode,
    pub component: SandboxDegradationComponent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadProfileStarted {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadProfileFinished {
    pub event_count: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadPolicySuggested {
    pub extends: Vec<String>,
    pub fs_allow_count: u64,
    pub fs_deny_count: u64,
    pub net_allow_count: u64,
    pub env_allow_count: u64,
    pub process_allow_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes_count: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_producer_meta_compact() {
        let meta = ProducerMeta::new("assay-cli", "2.6.0").with_git("abc1234");
        assert_eq!(meta.to_string_compact(), "assay-cli/2.6.0 (abc1234)");

        let meta_no_git = ProducerMeta::new("assay-cli", "2.6.0");
        assert_eq!(meta_no_git.to_string_compact(), "assay-cli/2.6.0");
    }

    #[test]
    fn version_constants_keep_cloudevents_and_assay_axes_separate() {
        assert_eq!(CE_SPECVERSION, "1.0");
        assert_eq!(ASSAY_EVIDENCE_SPEC_VERSION, "1.0");
        assert_eq!(SPEC_VERSION, CE_SPECVERSION);

        let event = EvidenceEvent::new(
            "assay.test.event",
            "urn:assay:test",
            "run_version_constants",
            0,
            serde_json::json!({}),
        );
        assert_eq!(event.specversion, CE_SPECVERSION);
    }

    #[test]
    fn tool_decision_payload_delegation_fields_are_additive() {
        let without = serde_json::json!({
            "tool": "deploy_service",
            "decision": "allow",
            "reason_code": "P_POLICY_ALLOW",
            "args_schema_hash": null
        });
        let without_payload: PayloadToolDecision =
            serde_json::from_value(without).expect("legacy payload should deserialize");
        assert_eq!(without_payload.delegated_from, None);
        assert_eq!(without_payload.delegation_depth, None);

        let with = serde_json::json!({
            "tool": "deploy_service",
            "decision": "allow",
            "reason_code": "P_POLICY_ALLOW",
            "args_schema_hash": null,
            "delegated_from": "agent:planner",
            "delegation_depth": 1
        });
        let with_payload: PayloadToolDecision =
            serde_json::from_value(with).expect("delegation payload should deserialize");
        assert_eq!(
            with_payload.delegated_from.as_deref(),
            Some("agent:planner")
        );
        assert_eq!(with_payload.delegation_depth, Some(1));
    }

    #[test]
    fn test_event_id_format() {
        let event = EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_123",
            42,
            serde_json::json!({}),
        );
        assert_eq!(event.id, "run_123:42");
        assert_eq!(event.run_id, "run_123");
        assert_eq!(event.seq, 42);
    }

    #[test]
    fn sandbox_degraded_payload_serde_shape_is_stable() {
        let payload = PayloadSandboxDegraded {
            reason_code: SandboxDegradationReasonCode::BackendUnavailable,
            degradation_mode: SandboxDegradationMode::AuditFallback,
            component: SandboxDegradationComponent::Landlock,
            detail: None,
        };

        let value = serde_json::to_value(&payload).expect("payload should serialize");
        assert_eq!(value["reason_code"], "backend_unavailable");
        assert_eq!(value["degradation_mode"], "audit_fallback");
        assert_eq!(value["component"], "landlock");
        assert!(value.get("detail").is_none(), "detail should stay optional");

        let roundtrip: PayloadSandboxDegraded =
            serde_json::from_value(value).expect("payload should deserialize");
        assert_eq!(roundtrip, payload);
    }
}
