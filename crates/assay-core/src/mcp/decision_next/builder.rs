use super::event_types::{
    reason_codes, Decision, DecisionData, DecisionEvent, PolicyDecisionEventContext,
};
use super::normalization::refresh_fulfillment_normalization;
use serde_json::Value;

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
                tool_classes: Vec::new(),
                matched_tool_classes: Vec::new(),
                match_basis: None,
                matched_rule: None,
                typed_decision: None,
                policy_version: None,
                policy_digest: None,
                policy_snapshot_digest: None,
                policy_snapshot_digest_alg: None,
                policy_snapshot_canonicalization: None,
                policy_snapshot_schema: None,
                obligations: Vec::new(),
                obligation_outcomes: Vec::new(),
                approval_state: None,
                approval_id: None,
                approver: None,
                issued_at: None,
                expires_at: None,
                scope: None,
                approval_bound_tool: None,
                approval_bound_resource: None,
                approval_freshness: None,
                approval_failure_reason: None,
                scope_type: None,
                scope_value: None,
                scope_match_mode: None,
                scope_evaluation_state: None,
                scope_failure_reason: None,
                restrict_scope_present: None,
                restrict_scope_target: None,
                restrict_scope_match: None,
                restrict_scope_reason: None,
                redaction_target: None,
                redaction_mode: None,
                redaction_scope: None,
                redaction_applied_state: None,
                redaction_reason: None,
                redaction_failure_reason: None,
                redact_args_present: None,
                redact_args_target: None,
                redact_args_mode: None,
                redact_args_result: None,
                redact_args_reason: None,
                fail_closed: None,
                decision_outcome_kind: None,
                decision_origin: None,
                outcome_compat_state: None,
                fulfillment_decision_path: None,
                decision_basis_version: None,
                compat_fallback_applied: None,
                classification_source: None,
                replay_diff_reason: None,
                legacy_shape_detected: None,
                decision_consumer_contract_version: None,
                consumer_read_path: None,
                consumer_fallback_applied: None,
                consumer_payload_state: None,
                required_consumer_fields: Vec::new(),
                decision_context_contract_version: None,
                context_payload_state: None,
                required_context_fields: Vec::new(),
                missing_context_fields: Vec::new(),
                policy_deny: None,
                fail_closed_deny: None,
                enforcement_deny: None,
                deny_precedence_version: None,
                deny_classification_source: None,
                deny_legacy_fallback_applied: None,
                deny_convergence_reason: None,
                obligation_applied_present: None,
                obligation_skipped_present: None,
                obligation_error_present: None,
                lane: None,
                principal: None,
                auth_context_summary: None,
                auth_scheme: None,
                auth_issuer: None,
                delegated_from: None,
                delegation_depth: None,
                decision: Decision::Error,
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
        refresh_fulfillment_normalization(&mut self.data);
        self
    }

    /// Set deny decision.
    pub fn deny(mut self, reason_code: &str, reason: Option<String>) -> Self {
        self.data.decision = Decision::Deny;
        self.data.reason_code = reason_code.to_string();
        self.data.reason = reason;
        refresh_fulfillment_normalization(&mut self.data);
        self
    }

    /// Set error decision.
    pub fn error(mut self, reason_code: &str, reason: Option<String>) -> Self {
        self.data.decision = Decision::Error;
        self.data.reason_code = reason_code.to_string();
        self.data.reason = reason;
        refresh_fulfillment_normalization(&mut self.data);
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

    /// Set tool match metadata.
    pub fn with_tool_match(
        mut self,
        tool_classes: Vec<String>,
        matched_tool_classes: Vec<String>,
        match_basis: Option<String>,
        matched_rule: Option<String>,
    ) -> Self {
        self.data.tool_classes = tool_classes;
        self.data.matched_tool_classes = matched_tool_classes;
        self.data.match_basis = match_basis;
        self.data.matched_rule = matched_rule;
        self
    }

    /// Set policy context fields for Decision Event v2.
    pub fn with_policy_context(mut self, context: PolicyDecisionEventContext) -> Self {
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

        self.data.typed_decision = typed_decision;
        self.data.policy_version = policy_version;
        self.data.policy_digest = policy_digest;
        self.data.apply_policy_snapshot_projection(
            policy_snapshot_digest,
            policy_snapshot_digest_alg,
            policy_snapshot_canonicalization,
            policy_snapshot_schema,
        );
        self.data.obligations = obligations;
        self.data.obligation_outcomes = obligation_outcomes;
        self.data.approval_state = approval_state;
        if let Some(artifact) = approval_artifact {
            self.data.approval_id = Some(artifact.approval_id);
            self.data.approver = Some(artifact.approver);
            self.data.issued_at = Some(artifact.issued_at);
            self.data.expires_at = Some(artifact.expires_at);
            self.data.scope = Some(artifact.scope);
            self.data.approval_bound_tool = Some(artifact.bound_tool);
            self.data.approval_bound_resource = Some(artifact.bound_resource);
        }
        self.data.approval_freshness = approval_freshness;
        self.data.approval_failure_reason = approval_failure_reason;
        if let Some(contract) = scope_contract {
            self.data.scope_type = Some(contract.scope_type);
            self.data.scope_value = Some(contract.scope_value);
            self.data.scope_match_mode = Some(contract.scope_match_mode);
        }
        self.data.scope_evaluation_state = scope_evaluation_state;
        self.data.scope_failure_reason = scope_failure_reason;
        self.data.restrict_scope_present = restrict_scope_present;
        self.data.restrict_scope_target = restrict_scope_target;
        self.data.restrict_scope_match = restrict_scope_match;
        self.data.restrict_scope_reason = restrict_scope_reason;
        if let Some(contract) = redaction_contract {
            self.data.redaction_target = Some(contract.redaction_target);
            self.data.redaction_mode = Some(contract.redaction_mode);
            self.data.redaction_scope = Some(contract.redaction_scope);
        }
        self.data.redaction_applied_state = redaction_applied_state;
        self.data.redaction_reason = redaction_reason;
        self.data.redaction_failure_reason = redaction_failure_reason;
        self.data.redact_args_present = redact_args_present;
        self.data.redact_args_target = redact_args_target;
        self.data.redact_args_mode = redact_args_mode;
        self.data.redact_args_result = redact_args_result;
        self.data.redact_args_reason = redact_args_reason;
        self.data.fail_closed = fail_closed;
        self.data.lane = lane;
        self.data.principal = principal;
        self.data.auth_context_summary = auth_context_summary;
        self.data.auth_scheme = auth_scheme;
        self.data.auth_issuer = auth_issuer;
        self.data.delegated_from = delegated_from;
        self.data.delegation_depth = delegation_depth;
        refresh_fulfillment_normalization(&mut self.data);
        self
    }
}
