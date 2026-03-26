use super::super::super::decision::{ObligationOutcome, ObligationOutcomeStatus};
use super::super::super::policy::{ApprovalArtifact, ApprovalFreshness};
use super::super::emit;
use super::classification::requested_resource;
use super::{
    OUTCOME_NORMALIZATION_VERSION, OUTCOME_REASON_VALIDATED_IN_HANDLER, OUTCOME_STAGE_HANDLER,
};
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::mcp::tool_call_handler) enum ApprovalFailure {
    MissingApproval,
    ExpiredApproval,
    BoundToolMismatch,
    BoundResourceMismatch,
}

impl ApprovalFailure {
    fn code(self) -> &'static str {
        match self {
            Self::MissingApproval => "approval_missing",
            Self::ExpiredApproval => "approval_expired",
            Self::BoundToolMismatch => "approval_bound_tool_mismatch",
            Self::BoundResourceMismatch => "approval_bound_resource_mismatch",
        }
    }

    fn as_reason(self) -> &'static str {
        match self {
            Self::MissingApproval => "missing approval",
            Self::ExpiredApproval => "expired approval",
            Self::BoundToolMismatch => "bound tool mismatch",
            Self::BoundResourceMismatch => "bound resource mismatch",
        }
    }
}

impl std::fmt::Display for ApprovalFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_reason())
    }
}

pub(in crate::mcp::tool_call_handler) fn validate_approval_required(
    tool_name: &str,
    args: &Value,
    tool_match: &mut emit::ToolMatchMetadata,
) -> Option<ApprovalFailure> {
    let requires_approval = tool_match
        .obligations
        .iter()
        .any(|obligation| obligation.obligation_type == "approval_required");
    if !requires_approval {
        return None;
    }

    let artifact = parse_approval_artifact(args);
    let Some(artifact) = artifact else {
        return Some(mark_approval_failure(
            tool_match,
            ApprovalFailure::MissingApproval,
        ));
    };
    tool_match.approval_artifact = Some(artifact.clone());

    let freshness = classify_approval_freshness(&artifact);
    tool_match.approval_freshness = Some(freshness);
    if !matches!(freshness, ApprovalFreshness::Fresh) {
        return Some(mark_approval_failure(
            tool_match,
            ApprovalFailure::ExpiredApproval,
        ));
    }

    if artifact.bound_tool != tool_name {
        return Some(mark_approval_failure(
            tool_match,
            ApprovalFailure::BoundToolMismatch,
        ));
    }

    let requested_resource = requested_resource(args);
    if requested_resource != Some(artifact.bound_resource.as_str()) {
        return Some(mark_approval_failure(
            tool_match,
            ApprovalFailure::BoundResourceMismatch,
        ));
    }

    tool_match.approval_state = Some("approved".to_string());
    tool_match.approval_failure_reason = None;
    mark_approval_outcome(
        tool_match,
        ObligationOutcomeStatus::Applied,
        None,
        Some(OUTCOME_REASON_VALIDATED_IN_HANDLER),
    );
    None
}

fn mark_approval_failure(
    tool_match: &mut emit::ToolMatchMetadata,
    failure: ApprovalFailure,
) -> ApprovalFailure {
    tool_match.approval_state = Some("denied".to_string());
    tool_match.approval_failure_reason = Some(failure.as_reason().to_string());
    mark_approval_outcome(
        tool_match,
        ObligationOutcomeStatus::Error,
        Some(failure.as_reason()),
        Some(failure.code()),
    );
    failure
}

fn mark_approval_outcome(
    tool_match: &mut emit::ToolMatchMetadata,
    status: ObligationOutcomeStatus,
    reason: Option<&str>,
    reason_code: Option<&str>,
) {
    if let Some(outcome) = tool_match
        .obligation_outcomes
        .iter_mut()
        .find(|outcome| outcome.obligation_type == "approval_required")
    {
        outcome.status = status;
        outcome.reason = reason.map(ToString::to_string);
        outcome.reason_code = reason_code.map(ToString::to_string);
        outcome.enforcement_stage = Some(OUTCOME_STAGE_HANDLER.to_string());
        outcome.normalization_version = Some(OUTCOME_NORMALIZATION_VERSION.to_string());
        return;
    }

    tool_match.obligation_outcomes.push(ObligationOutcome {
        obligation_type: "approval_required".to_string(),
        status,
        reason: reason.map(ToString::to_string),
        reason_code: reason_code.map(ToString::to_string),
        enforcement_stage: Some(OUTCOME_STAGE_HANDLER.to_string()),
        normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
    });
}

fn parse_approval_artifact(args: &Value) -> Option<ApprovalArtifact> {
    let approval = args.get("_meta")?.get("approval")?;
    Some(ApprovalArtifact {
        approval_id: approval.get("approval_id")?.as_str()?.to_string(),
        approver: approval.get("approver")?.as_str()?.to_string(),
        issued_at: approval.get("issued_at")?.as_str()?.to_string(),
        expires_at: approval.get("expires_at")?.as_str()?.to_string(),
        scope: approval.get("scope")?.as_str()?.to_string(),
        bound_tool: approval.get("bound_tool")?.as_str()?.to_string(),
        bound_resource: approval.get("bound_resource")?.as_str()?.to_string(),
    })
}

fn classify_approval_freshness(artifact: &ApprovalArtifact) -> ApprovalFreshness {
    let issued = DateTime::parse_from_rfc3339(&artifact.issued_at).ok();
    let expires = DateTime::parse_from_rfc3339(&artifact.expires_at).ok();
    let (Some(issued_at), Some(expires_at)) = (issued, expires) else {
        return ApprovalFreshness::Expired;
    };

    let now = Utc::now();
    let issued_at = issued_at.with_timezone(&Utc);
    let expires_at = expires_at.with_timezone(&Utc);

    if now > expires_at {
        ApprovalFreshness::Expired
    } else if now < issued_at {
        ApprovalFreshness::Stale
    } else {
        ApprovalFreshness::Fresh
    }
}
