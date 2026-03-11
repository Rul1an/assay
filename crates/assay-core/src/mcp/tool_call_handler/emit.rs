use super::super::decision::{reason_codes, DecisionEvent, PolicyDecisionEventContext};
use super::super::policy::PolicyMatchMetadata;
use super::types::HandleResult;
use crate::runtime::AuthzReceipt;

#[derive(Clone)]
pub(super) struct ToolMatchMetadata {
    pub(super) tool_classes: Vec<String>,
    pub(super) matched_tool_classes: Vec<String>,
    pub(super) match_basis: Option<String>,
    pub(super) matched_rule: Option<String>,
    pub(super) typed_decision: Option<super::super::policy::TypedPolicyDecision>,
    pub(super) policy_version: Option<String>,
    pub(super) policy_digest: Option<String>,
    pub(super) obligations: Vec<super::super::policy::PolicyObligation>,
    pub(super) obligation_outcomes: Vec<super::super::decision::ObligationOutcome>,
    pub(super) approval_state: Option<String>,
    pub(super) approval_artifact: Option<super::super::policy::ApprovalArtifact>,
    pub(super) approval_freshness: Option<super::super::policy::ApprovalFreshness>,
    pub(super) approval_failure_reason: Option<String>,
    pub(super) lane: Option<String>,
    pub(super) principal: Option<String>,
    pub(super) auth_context_summary: Option<String>,
}

impl ToolMatchMetadata {
    pub(super) fn from_policy_metadata(metadata: &PolicyMatchMetadata) -> Self {
        Self {
            tool_classes: metadata.tool_classes.clone(),
            matched_tool_classes: metadata.matched_tool_classes.clone(),
            match_basis: metadata.match_basis.as_str().map(ToString::to_string),
            matched_rule: metadata.matched_rule.clone(),
            typed_decision: metadata.typed_decision,
            policy_version: metadata.policy_version.clone(),
            policy_digest: metadata.policy_digest.clone(),
            obligations: metadata.obligations.clone(),
            obligation_outcomes: Vec::new(),
            approval_state: metadata.approval_state.clone(),
            approval_artifact: metadata.approval_artifact.clone(),
            approval_freshness: metadata.approval_freshness,
            approval_failure_reason: metadata.approval_failure_reason.clone(),
            lane: metadata.lane.clone(),
            principal: metadata.principal.clone(),
            auth_context_summary: metadata.auth_context_summary.clone(),
        }
    }

    pub(super) fn policy_context(&self) -> PolicyDecisionEventContext {
        PolicyDecisionEventContext {
            typed_decision: self.typed_decision,
            policy_version: self.policy_version.clone(),
            policy_digest: self.policy_digest.clone(),
            obligations: self.obligations.clone(),
            obligation_outcomes: self.obligation_outcomes.clone(),
            approval_state: self.approval_state.clone(),
            approval_artifact: self.approval_artifact.clone(),
            approval_freshness: self.approval_freshness,
            approval_failure_reason: self.approval_failure_reason.clone(),
            lane: self.lane.clone(),
            principal: self.principal.clone(),
            auth_context_summary: self.auth_context_summary.clone(),
        }
    }
}

pub(super) fn error_not_tool_call(event_source: &str, tool_call_id: String) -> HandleResult {
    HandleResult::Error {
        reason_code: reason_codes::S_INTERNAL_ERROR.to_string(),
        reason: "Not a tool call".to_string(),
        decision_event: DecisionEvent::new(
            event_source.to_string(),
            tool_call_id,
            "unknown".to_string(),
        )
        .error(
            reason_codes::S_INTERNAL_ERROR,
            Some("Not a tool call".to_string()),
        ),
    }
}

pub(super) fn deny(
    event_source: &str,
    tool_call_id: String,
    tool_name: String,
    reason_code: &str,
    reason: String,
    tool_match: ToolMatchMetadata,
) -> HandleResult {
    let decision_event = DecisionEvent::new(event_source.to_string(), tool_call_id, tool_name)
        .deny(reason_code, Some(reason.clone()))
        .with_tool_match(
            tool_match.tool_classes.clone(),
            tool_match.matched_tool_classes.clone(),
            tool_match.match_basis.clone(),
            tool_match.matched_rule.clone(),
        )
        .with_policy_context(tool_match.policy_context());

    HandleResult::Deny {
        reason_code: reason_code.to_string(),
        reason,
        decision_event,
    }
}

pub(super) fn allow(
    event_source: &str,
    tool_call_id: String,
    tool_name: String,
    reason_code: &str,
    receipt: Option<AuthzReceipt>,
    tool_match: ToolMatchMetadata,
) -> HandleResult {
    let decision_event = DecisionEvent::new(event_source.to_string(), tool_call_id, tool_name)
        .allow(reason_code)
        .with_tool_match(
            tool_match.tool_classes.clone(),
            tool_match.matched_tool_classes.clone(),
            tool_match.match_basis.clone(),
            tool_match.matched_rule.clone(),
        )
        .with_policy_context(tool_match.policy_context());

    HandleResult::Allow {
        receipt,
        decision_event,
    }
}
