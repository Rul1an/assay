use super::emitters::DecisionEmitter;
use super::event_types::{reason_codes, DecisionEvent, PolicyDecisionEventContext};
use super::normalization::refresh_fulfillment_normalization;
use serde_json::Value;
use std::sync::Arc;

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

    /// Set tool match metadata for the event.
    pub fn set_tool_match(
        &mut self,
        tool_classes: Vec<String>,
        matched_tool_classes: Vec<String>,
        match_basis: Option<String>,
        matched_rule: Option<String>,
    ) {
        if let Some(ref mut event) = self.event {
            event.data.tool_classes = tool_classes;
            event.data.matched_tool_classes = matched_tool_classes;
            event.data.match_basis = match_basis;
            event.data.matched_rule = matched_rule;
        }
    }

    /// Set policy context metadata for Decision Event v2.
    pub fn set_policy_context(&mut self, context: PolicyDecisionEventContext) {
        if let Some(ref mut event) = self.event {
            let PolicyDecisionEventContext {
                typed_decision,
                policy_version,
                policy_digest,
                policy_snapshot_digest,
                policy_snapshot_digest_alg,
                policy_snapshot_canonicalization,
                policy_snapshot_schema,
                obligations,
                obligation_outcomes,
                approval_state,
                approval_artifact,
                approval_freshness,
                approval_failure_reason,
                scope_contract,
                scope_evaluation_state,
                scope_failure_reason,
                restrict_scope_present,
                restrict_scope_target,
                restrict_scope_match,
                restrict_scope_reason,
                redaction_contract,
                redaction_applied_state,
                redaction_reason,
                redaction_failure_reason,
                redact_args_present,
                redact_args_target,
                redact_args_mode,
                redact_args_result,
                redact_args_reason,
                fail_closed,
                lane,
                principal,
                auth_context_summary,
                auth_scheme,
                auth_issuer,
                delegated_from,
                delegation_depth,
            } = context;

            event.data.typed_decision = typed_decision;
            event.data.policy_version = policy_version;
            event.data.policy_digest = policy_digest;
            event.data.apply_policy_snapshot_projection(
                policy_snapshot_digest,
                policy_snapshot_digest_alg,
                policy_snapshot_canonicalization,
                policy_snapshot_schema,
            );
            event.data.obligations = obligations;
            event.data.obligation_outcomes = obligation_outcomes;
            event.data.approval_state = approval_state;
            if let Some(artifact) = approval_artifact {
                event.data.approval_id = Some(artifact.approval_id);
                event.data.approver = Some(artifact.approver);
                event.data.issued_at = Some(artifact.issued_at);
                event.data.expires_at = Some(artifact.expires_at);
                event.data.scope = Some(artifact.scope);
                event.data.approval_bound_tool = Some(artifact.bound_tool);
                event.data.approval_bound_resource = Some(artifact.bound_resource);
            }
            event.data.approval_freshness = approval_freshness;
            event.data.approval_failure_reason = approval_failure_reason;
            if let Some(contract) = scope_contract {
                event.data.scope_type = Some(contract.scope_type);
                event.data.scope_value = Some(contract.scope_value);
                event.data.scope_match_mode = Some(contract.scope_match_mode);
            }
            event.data.scope_evaluation_state = scope_evaluation_state;
            event.data.scope_failure_reason = scope_failure_reason;
            event.data.restrict_scope_present = restrict_scope_present;
            event.data.restrict_scope_target = restrict_scope_target;
            event.data.restrict_scope_match = restrict_scope_match;
            event.data.restrict_scope_reason = restrict_scope_reason;
            if let Some(contract) = redaction_contract {
                event.data.redaction_target = Some(contract.redaction_target);
                event.data.redaction_mode = Some(contract.redaction_mode);
                event.data.redaction_scope = Some(contract.redaction_scope);
            }
            event.data.redaction_applied_state = redaction_applied_state;
            event.data.redaction_reason = redaction_reason;
            event.data.redaction_failure_reason = redaction_failure_reason;
            event.data.redact_args_present = redact_args_present;
            event.data.redact_args_target = redact_args_target;
            event.data.redact_args_mode = redact_args_mode;
            event.data.redact_args_result = redact_args_result;
            event.data.redact_args_reason = redact_args_reason;
            event.data.fail_closed = fail_closed;
            event.data.lane = lane;
            event.data.principal = principal;
            event.data.auth_context_summary = auth_context_summary;
            event.data.auth_scheme = auth_scheme;
            event.data.auth_issuer = auth_issuer;
            event.data.delegated_from = delegated_from;
            event.data.delegation_depth = delegation_depth;
            refresh_fulfillment_normalization(&mut event.data);
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
        self.event = None;
        self.emitter.emit(&event);
    }
}

impl Drop for DecisionEmitterGuard {
    fn drop(&mut self) {
        if let Some(event) = self.event.take() {
            self.emitter.emit(&event.error(
                reason_codes::S_INTERNAL_ERROR,
                Some(
                    "Decision guard dropped without explicit emit (possible panic or early return)"
                        .to_string(),
                ),
            ));
        }
    }
}
