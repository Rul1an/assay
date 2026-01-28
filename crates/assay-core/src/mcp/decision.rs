//! Tool decision events and always-emit guard (SPEC-Mandate-v1.0.4 §7.9).
//!
//! This module implements the "always emit decision" invariant (I1):
//! Every tool call attempt MUST emit exactly one decision event.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::sync::Arc;

/// Reason codes for tool decisions (SPEC-Mandate-v1.0.4 §7.10).
pub mod reason_codes {
    // Policy denials (P_*)
    pub const P_POLICY_DENY: &str = "P_POLICY_DENY";
    pub const P_TOOL_DENIED: &str = "P_TOOL_DENIED";
    pub const P_TOOL_NOT_ALLOWED: &str = "P_TOOL_NOT_ALLOWED";
    pub const P_ARG_SCHEMA: &str = "P_ARG_SCHEMA";
    pub const P_RATE_LIMIT: &str = "P_RATE_LIMIT";
    pub const P_TOOL_DRIFT: &str = "P_TOOL_DRIFT";
    pub const P_MANDATE_REQUIRED: &str = "P_MANDATE_REQUIRED";
    pub const P_MANDATE_VALID: &str = "P_MANDATE_VALID";

    // Mandate failures (M_*)
    pub const M_EXPIRED: &str = "M_EXPIRED";
    pub const M_NOT_YET_VALID: &str = "M_NOT_YET_VALID";
    pub const M_NONCE_REPLAY: &str = "M_NONCE_REPLAY";
    pub const M_ALREADY_USED: &str = "M_ALREADY_USED";
    pub const M_MAX_USES_EXCEEDED: &str = "M_MAX_USES_EXCEEDED";
    pub const M_TOOL_NOT_IN_SCOPE: &str = "M_TOOL_NOT_IN_SCOPE";
    pub const M_KIND_MISMATCH: &str = "M_KIND_MISMATCH";
    pub const M_AUDIENCE_MISMATCH: &str = "M_AUDIENCE_MISMATCH";
    pub const M_ISSUER_NOT_TRUSTED: &str = "M_ISSUER_NOT_TRUSTED";
    pub const M_TRANSACTION_REF_MISMATCH: &str = "M_TRANSACTION_REF_MISMATCH";
    pub const M_NOT_FOUND: &str = "M_NOT_FOUND";

    // Store/system errors (S_*)
    pub const S_DB_ERROR: &str = "S_DB_ERROR";
    pub const S_INTERNAL_ERROR: &str = "S_INTERNAL_ERROR";

    // Timeout/execution errors (T_*)
    pub const T_TIMEOUT: &str = "T_TIMEOUT";
    pub const T_EXEC_ERROR: &str = "T_EXEC_ERROR";
}

/// Decision outcome for a tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
    Error,
}

/// A tool decision event (CloudEvents compliant).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEvent {
    /// CloudEvents specversion
    pub specversion: &'static str,
    /// Unique event ID
    pub id: String,
    /// Event type: assay.tool.decision
    #[serde(rename = "type")]
    pub event_type: &'static str,
    /// Event source (configured, not dynamic)
    pub source: String,
    /// Event timestamp (ISO 8601)
    pub time: String,
    /// Event data
    pub data: DecisionData,
}

/// Data payload for a decision event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionData {
    /// Tool name
    pub tool: String,
    /// Decision outcome
    pub decision: Decision,
    /// Machine-parseable reason code (MUST)
    pub reason_code: String,
    /// Human-readable reason (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Unique tool call identifier (MUST for idempotency)
    pub tool_call_id: String,
    /// Request ID from JSON-RPC
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Value>,
    /// Mandate ID if present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mandate_id: Option<String>,
    /// Use ID from consumption (if consumed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_id: Option<String>,
    /// Use count at time of decision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_count: Option<u32>,
    /// Whether tool matched mandate scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mandate_scope_match: Option<bool>,
    /// Whether mandate kind allows operation class
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mandate_kind_match: Option<bool>,
    /// Whether transaction_ref matched
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_ref_match: Option<bool>,
    /// Authorization latency in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authz_latency_ms: Option<u64>,
    /// Store latency in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store_latency_ms: Option<u64>,
}

impl DecisionEvent {
    /// Create a new decision event.
    pub fn new(source: String, tool_call_id: String, tool: String) -> Self {
        Self {
            specversion: "1.0",
            id: format!("evt_decision_{}", uuid::Uuid::new_v4()),
            event_type: "assay.tool.decision",
            source,
            time: chrono::Utc::now().to_rfc3339(),
            data: DecisionData {
                tool,
                decision: Decision::Error, // Default to error, will be set
                reason_code: reason_codes::S_INTERNAL_ERROR.to_string(),
                reason: Some("Decision not finalized (guard dropped without emit)".to_string()),
                tool_call_id,
                request_id: None,
                mandate_id: None,
                use_id: None,
                use_count: None,
                mandate_scope_match: None,
                mandate_kind_match: None,
                transaction_ref_match: None,
                authz_latency_ms: None,
                store_latency_ms: None,
            },
        }
    }

    /// Set allow decision.
    pub fn allow(mut self, reason_code: &str) -> Self {
        self.data.decision = Decision::Allow;
        self.data.reason_code = reason_code.to_string();
        self.data.reason = None;
        self
    }

    /// Set deny decision.
    pub fn deny(mut self, reason_code: &str, reason: Option<String>) -> Self {
        self.data.decision = Decision::Deny;
        self.data.reason_code = reason_code.to_string();
        self.data.reason = reason;
        self
    }

    /// Set error decision.
    pub fn error(mut self, reason_code: &str, reason: Option<String>) -> Self {
        self.data.decision = Decision::Error;
        self.data.reason_code = reason_code.to_string();
        self.data.reason = reason;
        self
    }

    /// Set request ID.
    pub fn with_request_id(mut self, id: Option<Value>) -> Self {
        self.data.request_id = id;
        self
    }

    /// Set mandate info.
    pub fn with_mandate(
        mut self,
        mandate_id: Option<String>,
        use_id: Option<String>,
        use_count: Option<u32>,
    ) -> Self {
        self.data.mandate_id = mandate_id;
        self.data.use_id = use_id;
        self.data.use_count = use_count;
        self
    }

    /// Set mandate match flags.
    pub fn with_mandate_matches(
        mut self,
        scope_match: Option<bool>,
        kind_match: Option<bool>,
        tx_ref_match: Option<bool>,
    ) -> Self {
        self.data.mandate_scope_match = scope_match;
        self.data.mandate_kind_match = kind_match;
        self.data.transaction_ref_match = tx_ref_match;
        self
    }

    /// Set latencies.
    pub fn with_latencies(mut self, authz_ms: Option<u64>, store_ms: Option<u64>) -> Self {
        self.data.authz_latency_ms = authz_ms;
        self.data.store_latency_ms = store_ms;
        self
    }
}

/// Trait for emitting decision events.
pub trait DecisionEmitter: Send + Sync {
    /// Emit a decision event.
    fn emit(&self, event: &DecisionEvent);
}

/// File-based decision emitter (NDJSON).
pub struct FileDecisionEmitter {
    file: std::sync::Mutex<std::fs::File>,
}

impl FileDecisionEmitter {
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

impl DecisionEmitter for FileDecisionEmitter {
    fn emit(&self, event: &DecisionEvent) {
        if let Ok(json) = serde_json::to_string(event) {
            if let Ok(mut f) = self.file.lock() {
                let _ = writeln!(f, "{}", json);
            }
        }
    }
}

/// Null emitter for testing.
pub struct NullDecisionEmitter;

impl DecisionEmitter for NullDecisionEmitter {
    fn emit(&self, _event: &DecisionEvent) {}
}

/// RAII guard that ensures a decision event is ALWAYS emitted.
///
/// This implements invariant I1: Every tool call attempt MUST emit exactly one
/// decision event, even on panics or early returns.
///
/// Usage:
/// ```ignore
/// let guard = DecisionEmitterGuard::new(emitter, source, tool_call_id, tool);
/// // ... do authorization work ...
/// guard.emit_allow("P_MANDATE_VALID"); // Consumes guard
/// // OR
/// guard.emit_deny("M_EXPIRED", Some("Mandate expired")); // Consumes guard
/// // If guard is dropped without explicit emit, emits error decision
/// ```
pub struct DecisionEmitterGuard {
    emitter: Arc<dyn DecisionEmitter>,
    event: Option<DecisionEvent>,
}

impl DecisionEmitterGuard {
    /// Create a new guard. The event will be emitted on drop if not explicitly emitted.
    pub fn new(
        emitter: Arc<dyn DecisionEmitter>,
        source: String,
        tool_call_id: String,
        tool: String,
    ) -> Self {
        Self {
            emitter,
            event: Some(DecisionEvent::new(source, tool_call_id, tool)),
        }
    }

    /// Set request ID for the event.
    pub fn set_request_id(&mut self, id: Option<Value>) {
        if let Some(ref mut event) = self.event {
            event.data.request_id = id;
        }
    }

    /// Set mandate info for the event.
    pub fn set_mandate_info(
        &mut self,
        mandate_id: Option<String>,
        use_id: Option<String>,
        use_count: Option<u32>,
    ) {
        if let Some(ref mut event) = self.event {
            event.data.mandate_id = mandate_id;
            event.data.use_id = use_id;
            event.data.use_count = use_count;
        }
    }

    /// Set mandate match flags.
    pub fn set_mandate_matches(
        &mut self,
        scope_match: Option<bool>,
        kind_match: Option<bool>,
        tx_ref_match: Option<bool>,
    ) {
        if let Some(ref mut event) = self.event {
            event.data.mandate_scope_match = scope_match;
            event.data.mandate_kind_match = kind_match;
            event.data.transaction_ref_match = tx_ref_match;
        }
    }

    /// Set latencies.
    pub fn set_latencies(&mut self, authz_ms: Option<u64>, store_ms: Option<u64>) {
        if let Some(ref mut event) = self.event {
            event.data.authz_latency_ms = authz_ms;
            event.data.store_latency_ms = store_ms;
        }
    }

    /// Emit an allow decision and consume the guard.
    pub fn emit_allow(mut self, reason_code: &str) {
        if let Some(event) = self.event.take() {
            self.emitter.emit(&event.allow(reason_code));
        }
    }

    /// Emit a deny decision and consume the guard.
    pub fn emit_deny(mut self, reason_code: &str, reason: Option<String>) {
        if let Some(event) = self.event.take() {
            self.emitter.emit(&event.deny(reason_code, reason));
        }
    }

    /// Emit an error decision and consume the guard.
    pub fn emit_error(mut self, reason_code: &str, reason: Option<String>) {
        if let Some(event) = self.event.take() {
            self.emitter.emit(&event.error(reason_code, reason));
        }
    }

    /// Emit with a pre-built event (advanced use).
    pub fn emit_event(mut self, event: DecisionEvent) {
        self.event = None; // Clear so drop doesn't double-emit
        self.emitter.emit(&event);
    }
}

impl Drop for DecisionEmitterGuard {
    fn drop(&mut self) {
        // If event is still present, it means no explicit emit was called.
        // This is the safety net: always emit something.
        if let Some(event) = self.event.take() {
            // Emit error decision with "guard dropped" reason
            self.emitter.emit(&event.error(
                reason_codes::S_INTERNAL_ERROR,
                Some("Decision guard dropped without explicit emit (possible panic or early return)".to_string()),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingEmitter {
        count: AtomicUsize,
        last_decision: std::sync::Mutex<Option<Decision>>,
        last_reason_code: std::sync::Mutex<Option<String>>,
    }

    impl CountingEmitter {
        fn new() -> Self {
            Self {
                count: AtomicUsize::new(0),
                last_decision: std::sync::Mutex::new(None),
                last_reason_code: std::sync::Mutex::new(None),
            }
        }
    }

    impl DecisionEmitter for CountingEmitter {
        fn emit(&self, event: &DecisionEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
            *self.last_decision.lock().unwrap() = Some(event.data.decision);
            *self.last_reason_code.lock().unwrap() = Some(event.data.reason_code.clone());
        }
    }

    #[test]
    fn test_guard_explicit_allow_emits_once() {
        let emitter = Arc::new(CountingEmitter::new());
        let guard = DecisionEmitterGuard::new(
            emitter.clone(),
            "assay://test".to_string(),
            "tc_001".to_string(),
            "test_tool".to_string(),
        );

        guard.emit_allow(reason_codes::P_MANDATE_VALID);

        assert_eq!(emitter.count.load(Ordering::SeqCst), 1);
        assert_eq!(
            *emitter.last_decision.lock().unwrap(),
            Some(Decision::Allow)
        );
    }

    #[test]
    fn test_guard_explicit_deny_emits_once() {
        let emitter = Arc::new(CountingEmitter::new());
        let guard = DecisionEmitterGuard::new(
            emitter.clone(),
            "assay://test".to_string(),
            "tc_002".to_string(),
            "test_tool".to_string(),
        );

        guard.emit_deny(reason_codes::M_EXPIRED, Some("Mandate expired".to_string()));

        assert_eq!(emitter.count.load(Ordering::SeqCst), 1);
        assert_eq!(*emitter.last_decision.lock().unwrap(), Some(Decision::Deny));
        assert_eq!(
            *emitter.last_reason_code.lock().unwrap(),
            Some(reason_codes::M_EXPIRED.to_string())
        );
    }

    #[test]
    fn test_guard_drop_emits_error() {
        let emitter = Arc::new(CountingEmitter::new());
        {
            let _guard = DecisionEmitterGuard::new(
                emitter.clone(),
                "assay://test".to_string(),
                "tc_003".to_string(),
                "test_tool".to_string(),
            );
            // Guard dropped without explicit emit
        }

        assert_eq!(emitter.count.load(Ordering::SeqCst), 1);
        assert_eq!(
            *emitter.last_decision.lock().unwrap(),
            Some(Decision::Error)
        );
        assert_eq!(
            *emitter.last_reason_code.lock().unwrap(),
            Some(reason_codes::S_INTERNAL_ERROR.to_string())
        );
    }

    #[test]
    fn test_guard_no_double_emit() {
        let emitter = Arc::new(CountingEmitter::new());
        {
            let guard = DecisionEmitterGuard::new(
                emitter.clone(),
                "assay://test".to_string(),
                "tc_004".to_string(),
                "test_tool".to_string(),
            );
            guard.emit_allow(reason_codes::P_POLICY_DENY);
            // Guard dropped after explicit emit
        }

        // Should only emit once
        assert_eq!(emitter.count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_event_serialization() {
        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_005".to_string(),
            "test_tool".to_string(),
        )
        .allow(reason_codes::P_MANDATE_VALID)
        .with_mandate(
            Some("sha256:abc".to_string()),
            Some("sha256:use".to_string()),
            Some(1),
        )
        .with_mandate_matches(Some(true), Some(true), Some(true));

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("assay.tool.decision"));
        assert!(json.contains("tc_005"));
        assert!(json.contains("allow"));
    }

    #[test]
    fn test_reason_codes_are_string_constants() {
        // Ensure reason codes are stable strings
        assert_eq!(reason_codes::P_POLICY_DENY, "P_POLICY_DENY");
        assert_eq!(reason_codes::M_EXPIRED, "M_EXPIRED");
        assert_eq!(reason_codes::S_DB_ERROR, "S_DB_ERROR");
        assert_eq!(reason_codes::T_TIMEOUT, "T_TIMEOUT");
    }
}
