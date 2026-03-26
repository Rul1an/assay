use super::super::super::decision::{ObligationOutcome, ObligationOutcomeStatus};
use super::super::emit;
use super::{OUTCOME_NORMALIZATION_VERSION, OUTCOME_STAGE_HANDLER};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::mcp::tool_call_handler) enum RestrictScopeFailure {
    TargetMissing,
    TargetMismatch,
    MatchModeUnsupported,
    TypeUnsupported,
}

impl RestrictScopeFailure {
    fn code(self) -> &'static str {
        match self {
            Self::TargetMissing => "scope_target_missing",
            Self::TargetMismatch => "scope_target_mismatch",
            Self::MatchModeUnsupported => "scope_match_mode_unsupported",
            Self::TypeUnsupported => "scope_type_unsupported",
        }
    }

    fn from_code(code: Option<&str>) -> Self {
        match code {
            Some("scope_target_missing") => Self::TargetMissing,
            Some("scope_match_mode_unsupported") => Self::MatchModeUnsupported,
            Some("scope_type_unsupported") => Self::TypeUnsupported,
            _ => Self::TargetMismatch,
        }
    }

    fn as_reason(self) -> &'static str {
        match self {
            Self::TargetMissing => "scope target missing",
            Self::TargetMismatch => "scope target mismatch",
            Self::MatchModeUnsupported => "scope match mode unsupported",
            Self::TypeUnsupported => "scope type unsupported",
        }
    }
}

impl std::fmt::Display for RestrictScopeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_reason())
    }
}

pub(in crate::mcp::tool_call_handler) fn validate_restrict_scope(
    tool_match: &mut emit::ToolMatchMetadata,
) -> Option<RestrictScopeFailure> {
    let requires_scope = tool_match
        .obligations
        .iter()
        .any(|obligation| obligation.obligation_type == "restrict_scope");
    if !requires_scope {
        return None;
    }

    if matches!(
        tool_match.scope_evaluation_state.as_deref(),
        Some("matched")
    ) {
        tool_match.restrict_scope_match = Some(true);
        tool_match.scope_failure_reason = None;
        tool_match.restrict_scope_reason = None;
        mark_restrict_scope_outcome(tool_match, ObligationOutcomeStatus::Applied, None, None);
        return None;
    }

    let failure = RestrictScopeFailure::from_code(
        tool_match
            .scope_failure_reason
            .as_deref()
            .or(tool_match.restrict_scope_reason.as_deref()),
    );
    Some(mark_restrict_scope_failure(tool_match, failure))
}

fn mark_restrict_scope_failure(
    tool_match: &mut emit::ToolMatchMetadata,
    failure: RestrictScopeFailure,
) -> RestrictScopeFailure {
    let failure_code = failure.code().to_string();
    tool_match.restrict_scope_match = Some(false);
    tool_match.scope_failure_reason = Some(failure_code.clone());
    tool_match.restrict_scope_reason = Some(failure_code.clone());
    if tool_match.scope_evaluation_state.is_none() {
        tool_match.scope_evaluation_state = Some("not_evaluated".to_string());
    }
    mark_restrict_scope_outcome(
        tool_match,
        ObligationOutcomeStatus::Error,
        Some(failure_code.as_str()),
        Some(failure_code.as_str()),
    );
    failure
}

fn mark_restrict_scope_outcome(
    tool_match: &mut emit::ToolMatchMetadata,
    status: ObligationOutcomeStatus,
    reason: Option<&str>,
    reason_code: Option<&str>,
) {
    if let Some(outcome) = tool_match
        .obligation_outcomes
        .iter_mut()
        .find(|outcome| outcome.obligation_type == "restrict_scope")
    {
        outcome.status = status;
        outcome.reason = reason.map(ToString::to_string);
        outcome.reason_code = reason_code.map(ToString::to_string);
        outcome.enforcement_stage = Some(OUTCOME_STAGE_HANDLER.to_string());
        outcome.normalization_version = Some(OUTCOME_NORMALIZATION_VERSION.to_string());
        return;
    }

    tool_match.obligation_outcomes.push(ObligationOutcome {
        obligation_type: "restrict_scope".to_string(),
        status,
        reason: reason.map(ToString::to_string),
        reason_code: reason_code.map(ToString::to_string),
        enforcement_stage: Some(OUTCOME_STAGE_HANDLER.to_string()),
        normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
    });
}
