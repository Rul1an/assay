//! Frozen validation-only copy of the DecisionEvent serde graph pinned by Incident Package v1.
//!
//! Source baseline: bf2659824e51c63edb433d2386b003f16a2da419. The complete mapping and
//! current-producer parity are reviewed privately. The two borrowed header strings are owned here
//! because storage lifetime is not a JSON admission rule. This module never replaces the retained
//! original `Value`.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationOutcomeStatus {
    Applied,
    Skipped,
    Error,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FulfillmentDecisionPath {
    PolicyAllow,
    PolicyDeny,
    FailClosedDeny,
    DecisionError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEvent {
    /// CloudEvents specversion
    pub specversion: String,
    /// Unique event ID
    pub id: String,
    /// Event type: assay.tool.decision
    #[serde(rename = "type")]
    pub event_type: String,
    /// Event source (configured, not dynamic)
    pub source: String,
    /// When the decision was made (RFC 3339). For decisions from the tool-call handler this is
    /// the instant approval freshness and mandate validity were judged against.
    pub time: String,
    /// Event data
    pub data: DecisionData,
}

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
    /// P56b: digest of the bounded MCP tool definition observed from `tools/list`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_definition_digest: Option<String>,
    /// P56b: digest algorithm for `tool_definition_digest`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_definition_digest_alg: Option<String>,
    /// P56b: canonicalization used before digesting the tool definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_definition_canonicalization: Option<String>,
    /// P56b: bounded schema tag for the tool-definition surface being digested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_definition_schema: Option<String>,
    /// P56b: observed source surface for the tool-definition digest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_definition_source: Option<String>,
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
    /// Digest of the retained structured approval artifact basis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_artifact_digest: Option<String>,
    /// Digest algorithm for `approval_artifact_digest`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_artifact_digest_alg: Option<String>,
    /// Profile/canonicalization used for `approval_artifact_digest`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_artifact_digest_profile: Option<String>,
    /// Retained view class for the approval basis
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_retained_view: Option<String>,
    /// Explicit non-claims for structured approval retention
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_retention_non_claims: Option<Vec<String>>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TypedPolicyDecision {
    Allow,
    #[serde(rename = "allow_with_obligations")]
    AllowWithObligations,
    Deny,
    #[serde(rename = "deny_with_alert")]
    DenyWithAlert,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyObligation {
    #[serde(rename = "type")]
    pub obligation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restrict_scope: Option<RestrictScopeContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redact_args: Option<RedactArgsContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestrictScopeContract {
    pub scope_type: String,
    pub scope_value: String,
    pub scope_match_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactArgsContract {
    pub redaction_target: String,
    pub redaction_mode: String,
    pub redaction_scope: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalFreshness {
    Fresh,
    Stale,
    Expired,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolRiskClass {
    HighRisk,
    LowRiskRead,
    Default,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailClosedMode {
    FailClosed,
    DegradeReadOnly,
    FailSafeAllow,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailClosedTrigger {
    PolicyEngineUnavailable,
    ContextProviderUnavailable,
    RuntimeDependencyError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailClosedContext {
    pub tool_risk_class: ToolRiskClass,
    pub fail_closed_mode: FailClosedMode,
    pub fail_closed_trigger: Option<FailClosedTrigger>,
    pub fail_closed_applied: bool,
    pub fail_closed_error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerReadPath {
    ConvergedDecision,
    CompatibilityMarkers,
    LegacyDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumerPayloadState {
    Converged,
    CompatibilityFallback,
    LegacyBase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub enum ContextPayloadState {
    CompleteEnvelope,
    PartialEnvelope,
    AbsentEnvelope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyClassificationSource {
    OutcomeKind,
    OriginContext,
    FulfillmentPath,
    LegacyDecision,
    NotDeny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOutcomeKind {
    PolicyDeny,
    FailClosedDeny,
    EnforcementDeny,
    ObligationApplied,
    ObligationSkipped,
    ObligationError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionOrigin {
    PolicyEngine,
    FailClosedMatrix,
    RuntimeEnforcement,
    ObligationExecutor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeCompatState {
    LegacyFieldsPreserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayClassificationSource {
    ConvergedOutcome,
    FulfillmentPath,
    LegacyFallback,
}

pub(crate) fn validate(value: &Value) -> serde_json::Result<()> {
    let _: DecisionEvent = DecisionEvent::deserialize(value)?;
    Ok(())
}
