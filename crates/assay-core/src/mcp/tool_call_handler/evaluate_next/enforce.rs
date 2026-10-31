use super::super::super::decision::{reason_codes, ObligationOutcomeStatus};
use super::super::emit;
use super::{
    approval::validate_approval_required, redaction::validate_redact_args,
    scope::validate_restrict_scope, OUTCOME_NORMALIZATION_VERSION, OUTCOME_STAGE_HANDLER,
};
use chrono::{DateTime, Utc};
use serde_json::Value;

const ENFORCED_OBLIGATIONS: [&str; 3] = ["approval_required", "restrict_scope", "redact_args"];
const OUTCOME_REASON_NOT_REACHED: &str = "not_reached";

pub(in crate::mcp::tool_call_handler) struct ObligationDenial {
    pub(in crate::mcp::tool_call_handler) reason_code: &'static str,
    pub(in crate::mcp::tool_call_handler) reason: String,
}

/// The one reading of `approval_required`, `restrict_scope` and `redact_args`, shared by the
/// handler and by `assay mcp wrap`. `args` holds the arguments to forward when this returns `None`.
pub(in crate::mcp::tool_call_handler) fn enforce_obligations(
    tool_name: &str,
    args: &mut Value,
    tool_match: &mut emit::ToolMatchMetadata,
    now: DateTime<Utc>,
) -> Option<ObligationDenial> {
    let denial = if let Some(failure) = validate_approval_required(tool_name, args, tool_match, now)
    {
        Some(ObligationDenial {
            reason_code: reason_codes::P_APPROVAL_REQUIRED,
            reason: failure.to_string(),
        })
    } else if let Some(failure) = validate_restrict_scope(tool_match) {
        Some(ObligationDenial {
            reason_code: reason_codes::P_RESTRICT_SCOPE,
            reason: failure.to_string(),
        })
    } else {
        validate_redact_args(args, tool_match).map(|failure| ObligationDenial {
            reason_code: reason_codes::P_REDACT_ARGS,
            reason: failure.to_string(),
        })
    };
    if denial.is_some() {
        mark_not_reached(tool_match);
    }
    denial
}

/// What an earlier denial kept from running states that, so no record reads as if it ran.
fn mark_not_reached(tool_match: &mut emit::ToolMatchMetadata) {
    for outcome in tool_match.obligation_outcomes.iter_mut().filter(|outcome| {
        outcome.status == ObligationOutcomeStatus::Skipped
            && ENFORCED_OBLIGATIONS.contains(&outcome.obligation_type.as_str())
    }) {
        outcome.reason = Some("an earlier obligation denied the call".to_string());
        outcome.reason_code = Some(OUTCOME_REASON_NOT_REACHED.to_string());
        outcome.enforcement_stage = Some(OUTCOME_STAGE_HANDLER.to_string());
        outcome.normalization_version = Some(OUTCOME_NORMALIZATION_VERSION.to_string());
    }
    if tool_match.redaction_applied_state.as_deref() == Some("applied") {
        tool_match.redaction_applied_state = Some("not_applied".to_string());
        tool_match.redact_args_result = Some("not_applied".to_string());
    }
}
