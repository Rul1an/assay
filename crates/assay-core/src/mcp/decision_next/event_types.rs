use super::super::{
    consumer_contract::{ConsumerPayloadState, ConsumerReadPath},
    context_contract::ContextPayloadState,
    deny_convergence::DenyClassificationSource,
    outcome_convergence::{DecisionOrigin, DecisionOutcomeKind, OutcomeCompatState},
    replay_compat::ReplayClassificationSource,
};
use crate::mcp::policy::{
    ApprovalArtifact, ApprovalFreshness, FailClosedContext, PolicyObligation, RedactArgsContract,
    RestrictScopeContract, TypedPolicyDecision,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Policy snapshot digest algorithm used by supported decision events.
pub const POLICY_SNAPSHOT_DIGEST_ALG_SHA256: &str = "sha256";
/// Canonicalization applied before computing `policy_snapshot_digest`.
pub const POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY: &str = "jcs:mcp_policy";
/// Bounded schema tag for the supported MCP policy snapshot projection.
pub const POLICY_SNAPSHOT_SCHEMA_V1: &str = "assay.mcp.policy.snapshot.v1";

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
    /// G3 v1: allowlisted scheme when policy-projected
    pub auth_scheme: Option<String>,
    /// G3 v1: trimmed issuer string
    pub auth_issuer: Option<String>,
    pub delegated_from: Option<String>,
    pub delegation_depth: Option<u32>,
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
    /// P56a: digest of the canonical policy snapshot used for this evaluation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_snapshot_digest: Option<String>,
    /// P56a: digest algorithm for `policy_snapshot_digest`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_snapshot_digest_alg: Option<String>,
    /// P56a: canonicalization used before digesting the policy snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_snapshot_canonicalization: Option<String>,
    /// P56a: bounded schema tag for the snapshot surface being digested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_snapshot_schema: Option<String>,
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
    /// Version tag for bounded consumer-facing compatibility reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_consumer_contract_version: Option<String>,
    /// Consumer-facing precedence path selected for deterministic reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumer_read_path: Option<ConsumerReadPath>,
    /// Whether consumer-facing reads depended on compatibility fallback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumer_fallback_applied: Option<bool>,
    /// Overall consumer-facing payload robustness state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumer_payload_state: Option<ConsumerPayloadState>,
    /// Contract-level required fields for bounded consumer reads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_consumer_fields: Vec<String>,
    /// Version tag for bounded context-envelope compatibility reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_context_contract_version: Option<String>,
    /// Overall context-envelope completeness state for downstream consumers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_payload_state: Option<ContextPayloadState>,
    /// Contract-level required fields for bounded context-envelope reads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_context_fields: Vec<String>,
    /// Missing context fields for the current envelope projection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_context_fields: Vec<String>,
    /// Explicit deny classification marker: policy deny path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_deny: Option<bool>,
    /// Explicit deny classification marker: fail-closed deny path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_closed_deny: Option<bool>,
    /// Explicit deny classification marker: runtime enforcement deny path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforcement_deny: Option<bool>,
    /// Version tag for deterministic deny-precedence convergence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny_precedence_version: Option<String>,
    /// Source selected by deterministic deny-classification precedence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny_classification_source: Option<DenyClassificationSource>,
    /// Whether deny classification used additive legacy fallback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny_legacy_fallback_applied: Option<bool>,
    /// Deterministic reason token for deny convergence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny_convergence_reason: Option<String>,
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
    /// G3 v1: `oauth2` | `jwt_bearer` (policy-projected)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_scheme: Option<String>,
    /// G3 v1: trimmed issuer (`iss`) string
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_issuer: Option<String>,
    /// Upstream delegated authority identifier, when explicitly carried by the supported flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegated_from: Option<String>,
    /// Explicit delegation depth reported by the supported flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation_depth: Option<u32>,
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

impl DecisionData {
    /// Project the legacy-compatible `policy_digest` into the explicit P56a
    /// review surface. This is a pure projection: never reconstruct a digest,
    /// and never let the snapshot fields represent a different value.
    pub(crate) fn apply_policy_snapshot_projection(&mut self) {
        self.policy_snapshot_digest = self.policy_digest.clone();

        if self.policy_snapshot_digest.is_some() {
            self.policy_snapshot_digest_alg = Some(POLICY_SNAPSHOT_DIGEST_ALG_SHA256.to_string());
            self.policy_snapshot_canonicalization =
                Some(POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY.to_string());
            self.policy_snapshot_schema = Some(POLICY_SNAPSHOT_SCHEMA_V1.to_string());
        } else {
            self.policy_snapshot_digest_alg = None;
            self.policy_snapshot_canonicalization = None;
            self.policy_snapshot_schema = None;
        }
    }
}
