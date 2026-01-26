//! Evidence Contract v1 Types
//!
//! CloudEvents-compatible envelope for Assay Evidence.
//! Designed for auditability, determinism, and interoperability.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Spec version for this implementation of the Evidence Contract.
pub const SPEC_VERSION: &str = "1.0";

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

    /// Event Type URN (e.g., "assay.env.filtered")
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

    /// Cryptographic Content Hash of canonical data.
    ///
    /// Required for v1 Evidence Contract verification.
    /// If None during export, BundleWriter will compute it.
    /// Verifier will FAIL if this is None.
    #[serde(skip_serializing_if = "Option::is_none", rename = "assaycontenthash")]
    pub content_hash: Option<String>,

    // -- Data --
    /// The event payload (CloudEvents "data" field)
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
            specversion: SPEC_VERSION.into(),
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
    /// Extensible: unknown types pass through
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadExecObserved {
    pub argv0: String,
    pub args_hash: String,
    pub env_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PayloadSandboxDegraded {
    pub reason_code: String,
    pub message: String,
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
}
