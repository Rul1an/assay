//! Tool decision events and always-emit guard (SPEC-Mandate-v1.0.4 §7.9).
//!
//! This module implements the "always emit decision" invariant (I1):
//! Every tool call attempt MUST emit exactly one decision event.

use self::outcome_convergence::classify_decision_outcome;
use self::replay_compat::project_replay_compat;
use super::policy::{
    ApprovalArtifact, ApprovalFreshness, FailClosedContext, PolicyObligation, RedactArgsContract,
    RestrictScopeContract, TypedPolicyDecision,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::sync::Arc;

mod outcome_convergence;
mod replay_compat;
mod replay_diff;
pub use outcome_convergence::{DecisionOrigin, DecisionOutcomeKind, OutcomeCompatState};
pub use replay_compat::{ReplayClassificationSource, DECISION_BASIS_VERSION_V1};
pub use replay_diff::{
    basis_from_decision_data, classify_replay_diff, ReplayDiffBasis, ReplayDiffBucket,
};

/// Reason codes for tool decisions (SPEC-Mandate-v1.0.4 §7.10).
pub mod reason_codes {
    // Policy decisions (P_*)
    pub const P_POLICY_ALLOW: &str = "P_POLICY_ALLOW";
    pub const P_POLICY_DENY: &str = "P_POLICY_DENY";
    pub const P_TOOL_DENIED: &str = "P_TOOL_DENIED";
    pub const P_TOOL_NOT_ALLOWED: &str = "P_TOOL_NOT_ALLOWED";
    pub const P_ARG_SCHEMA: &str = "P_ARG_SCHEMA";
    pub const P_RATE_LIMIT: &str = "P_RATE_LIMIT";
    pub const P_TOOL_DRIFT: &str = "P_TOOL_DRIFT";
    pub const P_APPROVAL_REQUIRED: &str = "P_APPROVAL_REQUIRED";
    pub const P_RESTRICT_SCOPE: &str = "P_RESTRICT_SCOPE";
    pub const P_REDACT_ARGS: &str = "P_REDACT_ARGS";
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
    pub const M_REVOKED: &str = "M_REVOKED";

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

/// Fulfillment status for a runtime obligation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationOutcomeStatus {
    Applied,
    Skipped,
    Error,
}

/// Runtime fulfillment result for an individual obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationOutcome {
    #[serde(rename = "type")]
    pub obligation_type: String,
    pub status: ObligationOutcomeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforcement_stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalization_version: Option<String>,
}

/// Normalized decision path classification for fulfillment evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FulfillmentDecisionPath {
    PolicyAllow,
    PolicyDeny,
    FailClosedDeny,
    DecisionError,
}

const OUTCOME_STAGE_HANDLER: &str = "handler";
const OUTCOME_REASON_CODE_APPLIED: &str = "obligation_applied";
const OUTCOME_REASON_CODE_SKIPPED: &str = "obligation_skipped";
const OUTCOME_REASON_CODE_ERROR: &str = "obligation_error";
const OUTCOME_NORMALIZATION_VERSION_V1: &str = "v1";

/// Additional runtime policy context for Decision Event v2.
#[derive(Debug, Clone, Default)]
pub struct PolicyDecisionEventContext {
    pub typed_decision: Option<TypedPolicyDecision>,
    pub policy_version: Option<String>,
    pub policy_digest: Option<String>,
    pub obligations: Vec<PolicyObligation>,
    pub obligation_outcomes: Vec<ObligationOutcome>,
    pub approval_state: Option<String>,
    pub approval_artifact: Option<ApprovalArtifact>,
    pub approval_freshness: Option<ApprovalFreshness>,
    pub approval_failure_reason: Option<String>,
    pub scope_contract: Option<RestrictScopeContract>,
    pub scope_evaluation_state: Option<String>,
    pub scope_failure_reason: Option<String>,
    pub restrict_scope_present: Option<bool>,
    pub restrict_scope_target: Option<String>,
    pub restrict_scope_match: Option<bool>,
    pub restrict_scope_reason: Option<String>,
    pub redaction_contract: Option<RedactArgsContract>,
    pub redaction_applied_state: Option<String>,
    pub redaction_reason: Option<String>,
    pub redaction_failure_reason: Option<String>,
    pub redact_args_present: Option<bool>,
    pub redact_args_target: Option<String>,
    pub redact_args_mode: Option<String>,
    pub redact_args_result: Option<String>,
    pub redact_args_reason: Option<String>,
    pub fail_closed: Option<FailClosedContext>,
    pub lane: Option<String>,
    pub principal: Option<String>,
    pub auth_context_summary: Option<String>,
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
    /// Tool classes observed at decision time (sorted)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_classes: Vec<String>,
    /// Tool classes that matched the policy decision (sorted)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_tool_classes: Vec<String>,
    /// Match basis for policy evaluation: name, class, or name+class
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_basis: Option<String>,
    /// Rule or policy field that matched
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_rule: Option<String>,
    /// Typed policy decision shape (Wave24 Decision Event v2)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typed_decision: Option<TypedPolicyDecision>,
    /// Policy bundle version used for evaluation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_version: Option<String>,
    /// Policy bundle digest used for evaluation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_digest: Option<String>,
    /// Obligations attached to an allow/deny decision
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obligations: Vec<PolicyObligation>,
    /// Runtime fulfillment outcomes for attached obligations
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obligation_outcomes: Vec<ObligationOutcome>,
    /// Approval state summary for runtime decisioning
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_state: Option<String>,
    /// Approval artifact identifier for runtime decisioning
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    /// Approval artifact approver principal summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approver: Option<String>,
    /// Approval artifact issuance time (ISO 8601 expected)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<String>,
    /// Approval artifact expiry time (ISO 8601 expected)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Approval artifact scope summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Approval artifact bound tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_bound_tool: Option<String>,
    /// Approval artifact bound resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_bound_resource: Option<String>,
    /// Freshness status derived from approval validity window
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_freshness: Option<ApprovalFreshness>,
    /// Approval failure reason for approval_required deny paths
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_failure_reason: Option<String>,
    /// Restrict-scope obligation shape field: type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_type: Option<String>,
    /// Restrict-scope obligation shape field: value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_value: Option<String>,
    /// Restrict-scope obligation shape field: match mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_match_mode: Option<String>,
    /// Restrict-scope evaluation state (contract/evidence only in Wave29)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_evaluation_state: Option<String>,
    /// Restrict-scope failure reason (contract/evidence only in Wave29)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_failure_reason: Option<String>,
    /// Restrict-scope evidence marker: obligation present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restrict_scope_present: Option<bool>,
    /// Restrict-scope evidence marker: evaluated target
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restrict_scope_target: Option<String>,
    /// Restrict-scope evidence marker: passive match result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restrict_scope_match: Option<bool>,
    /// Restrict-scope evidence marker: passive reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restrict_scope_reason: Option<String>,
    /// Redact-args obligation shape field: target
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_target: Option<String>,
    /// Redact-args obligation shape field: mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_mode: Option<String>,
    /// Redact-args obligation shape field: scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_scope: Option<String>,
    /// Redact-args evidence field: applied state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_applied_state: Option<String>,
    /// Redact-args evidence field: reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_reason: Option<String>,
    /// Redact-args deny-path failure reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_failure_reason: Option<String>,
    /// Redact-args additive marker: obligation present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_args_present: Option<bool>,
    /// Redact-args additive marker: target summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_args_target: Option<String>,
    /// Redact-args additive marker: mode summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_args_mode: Option<String>,
    /// Redact-args additive marker: evaluation result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_args_result: Option<String>,
    /// Redact-args additive marker: evaluation reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_args_reason: Option<String>,
    /// Additive fail-closed matrix context for this decision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_closed: Option<FailClosedContext>,
    /// Canonical decision/evidence convergence outcome classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_outcome_kind: Option<DecisionOutcomeKind>,
    /// Origin for the canonical convergence classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_origin: Option<DecisionOrigin>,
    /// Compatibility state for downstream consumers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome_compat_state: Option<OutcomeCompatState>,
    /// Normalized decision path for fulfillment evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfillment_decision_path: Option<FulfillmentDecisionPath>,
    /// Version tag for replay basis compatibility normalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_basis_version: Option<String>,
    /// Whether compatibility fallback was applied during classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compat_fallback_applied: Option<bool>,
    /// Deterministic source used for compatibility classification precedence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification_source: Option<ReplayClassificationSource>,
    /// Deterministic replay classification reason token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_diff_reason: Option<String>,
    /// Whether legacy event shape was detected by compatibility normalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_shape_detected: Option<bool>,
    /// Whether any obligation outcome is normalized as applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obligation_applied_present: Option<bool>,
    /// Whether any obligation outcome is normalized as skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obligation_skipped_present: Option<bool>,
    /// Whether any obligation outcome is normalized as error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obligation_error_present: Option<bool>,
    /// Lane identifier summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    /// Principal identifier summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub principal: Option<String>,
    /// Authentication context summary (non-sensitive, compact)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_context_summary: Option<String>,
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

fn normalize_obligation_outcome(mut outcome: ObligationOutcome) -> ObligationOutcome {
    if outcome.reason_code.is_none() {
        outcome.reason_code = Some(
            match outcome.status {
                ObligationOutcomeStatus::Applied => OUTCOME_REASON_CODE_APPLIED,
                ObligationOutcomeStatus::Skipped => OUTCOME_REASON_CODE_SKIPPED,
                ObligationOutcomeStatus::Error => OUTCOME_REASON_CODE_ERROR,
            }
            .to_string(),
        );
    }
    if outcome.enforcement_stage.is_none() {
        outcome.enforcement_stage = Some(OUTCOME_STAGE_HANDLER.to_string());
    }
    if outcome.normalization_version.is_none() {
        outcome.normalization_version = Some(OUTCOME_NORMALIZATION_VERSION_V1.to_string());
    }
    outcome
}

fn normalize_obligation_outcomes(outcomes: Vec<ObligationOutcome>) -> Vec<ObligationOutcome> {
    outcomes
        .into_iter()
        .map(normalize_obligation_outcome)
        .collect()
}

fn classify_fulfillment_decision_path(data: &DecisionData) -> FulfillmentDecisionPath {
    match data.decision {
        Decision::Allow => FulfillmentDecisionPath::PolicyAllow,
        Decision::Deny => {
            if data
                .fail_closed
                .as_ref()
                .map(|ctx| ctx.fail_closed_applied)
                .unwrap_or(false)
            {
                FulfillmentDecisionPath::FailClosedDeny
            } else {
                FulfillmentDecisionPath::PolicyDeny
            }
        }
        Decision::Error => FulfillmentDecisionPath::DecisionError,
    }
}

fn refresh_fulfillment_normalization(data: &mut DecisionData) {
    let outcomes = std::mem::take(&mut data.obligation_outcomes);
    data.obligation_outcomes = normalize_obligation_outcomes(outcomes);
    data.obligation_applied_present = Some(
        data.obligation_outcomes
            .iter()
            .any(|outcome| outcome.status == ObligationOutcomeStatus::Applied),
    );
    data.obligation_skipped_present = Some(
        data.obligation_outcomes
            .iter()
            .any(|outcome| outcome.status == ObligationOutcomeStatus::Skipped),
    );
    data.obligation_error_present = Some(
        data.obligation_outcomes
            .iter()
            .any(|outcome| outcome.status == ObligationOutcomeStatus::Error),
    );
    let outcome = classify_decision_outcome(
        data.decision,
        data.reason_code.as_str(),
        data.fail_closed
            .as_ref()
            .map(|ctx| ctx.fail_closed_applied)
            .unwrap_or(false),
        data.obligation_applied_present.unwrap_or(false),
        data.obligation_skipped_present.unwrap_or(false),
        data.obligation_error_present.unwrap_or(false),
    );
    data.decision_outcome_kind = Some(outcome.kind);
    data.decision_origin = Some(outcome.origin);
    data.outcome_compat_state = Some(outcome.compat_state);
    data.fulfillment_decision_path = Some(classify_fulfillment_decision_path(data));
    let replay_projection = project_replay_compat(
        data.decision_outcome_kind,
        data.decision_origin,
        data.outcome_compat_state,
        data.fulfillment_decision_path,
        data.decision,
    );
    data.decision_basis_version = Some(DECISION_BASIS_VERSION_V1.to_string());
    data.compat_fallback_applied = Some(replay_projection.compat_fallback_applied);
    data.classification_source = Some(replay_projection.classification_source);
    data.replay_diff_reason = Some(replay_projection.replay_diff_reason.to_string());
    data.legacy_shape_detected = Some(replay_projection.legacy_shape_detected);
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
                tool_classes: Vec::new(),
                matched_tool_classes: Vec::new(),
                match_basis: None,
                matched_rule: None,
                typed_decision: None,
                policy_version: None,
                policy_digest: None,
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
                obligation_applied_present: None,
                obligation_skipped_present: None,
                obligation_error_present: None,
                lane: None,
                principal: None,
                auth_context_summary: None,
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
        } = context;

        self.data.typed_decision = typed_decision;
        self.data.policy_version = policy_version;
        self.data.policy_digest = policy_digest;
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
        refresh_fulfillment_normalization(&mut self.data);
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
            } = context;

            event.data.typed_decision = typed_decision;
            event.data.policy_version = policy_version;
            event.data.policy_digest = policy_digest;
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
    fn test_with_policy_context_sets_approval_artifact_fields() {
        let context = PolicyDecisionEventContext {
            approval_state: Some("approved".to_string()),
            approval_artifact: Some(ApprovalArtifact {
                approval_id: "apr_001".to_string(),
                approver: "alice@example.com".to_string(),
                issued_at: "2026-03-11T11:00:00Z".to_string(),
                expires_at: "2026-03-11T12:00:00Z".to_string(),
                scope: "tool:deploy".to_string(),
                bound_tool: "deploy_service".to_string(),
                bound_resource: "service/prod".to_string(),
            }),
            approval_freshness: Some(ApprovalFreshness::Fresh),
            ..PolicyDecisionEventContext::default()
        };

        let event = DecisionEvent::new(
            "assay://test".to_string(),
            "tc_006".to_string(),
            "deploy_service".to_string(),
        )
        .allow(reason_codes::P_POLICY_ALLOW)
        .with_policy_context(context);

        assert_eq!(event.data.approval_state.as_deref(), Some("approved"));
        assert_eq!(event.data.approval_id.as_deref(), Some("apr_001"));
        assert_eq!(event.data.approver.as_deref(), Some("alice@example.com"));
        assert_eq!(
            event.data.issued_at.as_deref(),
            Some("2026-03-11T11:00:00Z")
        );
        assert_eq!(
            event.data.expires_at.as_deref(),
            Some("2026-03-11T12:00:00Z")
        );
        assert_eq!(event.data.scope.as_deref(), Some("tool:deploy"));
        assert_eq!(
            event.data.approval_bound_tool.as_deref(),
            Some("deploy_service")
        );
        assert_eq!(
            event.data.approval_bound_resource.as_deref(),
            Some("service/prod")
        );
        assert_eq!(
            event.data.approval_freshness,
            Some(ApprovalFreshness::Fresh)
        );
    }

    #[test]
    fn test_reason_codes_are_string_constants() {
        // Ensure reason codes are stable strings
        assert_eq!(reason_codes::P_POLICY_ALLOW, "P_POLICY_ALLOW");
        assert_eq!(reason_codes::P_POLICY_DENY, "P_POLICY_DENY");
        assert_eq!(reason_codes::P_RESTRICT_SCOPE, "P_RESTRICT_SCOPE");
        assert_eq!(reason_codes::P_REDACT_ARGS, "P_REDACT_ARGS");
        assert_eq!(reason_codes::M_EXPIRED, "M_EXPIRED");
        assert_eq!(reason_codes::S_DB_ERROR, "S_DB_ERROR");
        assert_eq!(reason_codes::T_TIMEOUT, "T_TIMEOUT");
    }
}
