use super::super::super::decision::{ObligationOutcome, ObligationOutcomeStatus};
use super::super::super::policy::RedactArgsContract;
use super::super::emit;
use super::{
    OUTCOME_NORMALIZATION_VERSION, OUTCOME_REASON_VALIDATED_IN_HANDLER, OUTCOME_STAGE_HANDLER,
};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::mcp::tool_call_handler) enum RedactArgsFailure {
    TargetMissing,
    ModeUnsupported,
    ScopeUnsupported,
    ApplyFailed,
}

impl RedactArgsFailure {
    fn code(self) -> &'static str {
        match self {
            Self::TargetMissing => "redaction_target_missing",
            Self::ModeUnsupported => "redaction_mode_unsupported",
            Self::ScopeUnsupported => "redaction_scope_unsupported",
            Self::ApplyFailed => "redaction_apply_failed",
        }
    }

    fn as_reason(self) -> &'static str {
        match self {
            Self::TargetMissing => "redaction target missing",
            Self::ModeUnsupported => "redaction mode unsupported",
            Self::ScopeUnsupported => "redaction scope unsupported",
            Self::ApplyFailed => "redaction apply failed",
        }
    }
}

impl std::fmt::Display for RedactArgsFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_reason())
    }
}

pub(in crate::mcp::tool_call_handler) fn validate_redact_args(
    args: &mut Value,
    tool_match: &mut emit::ToolMatchMetadata,
) -> Option<RedactArgsFailure> {
    let requires_redaction = tool_match
        .obligations
        .iter()
        .any(|obligation| obligation.obligation_type == "redact_args");
    if !requires_redaction {
        return None;
    }

    let Some(contract) = tool_match.redaction_contract.as_ref() else {
        return Some(mark_redact_args_failure(
            tool_match,
            RedactArgsFailure::TargetMissing,
        ));
    };

    match apply_redact_args_runtime(args, contract) {
        Ok(()) => {
            tool_match.redaction_applied_state = Some("applied".to_string());
            tool_match.redact_args_result = Some("applied".to_string());
            tool_match.redaction_failure_reason = None;
            tool_match.redaction_reason = None;
            tool_match.redact_args_reason = None;
            mark_redact_args_outcome(
                tool_match,
                ObligationOutcomeStatus::Applied,
                None,
                Some(OUTCOME_REASON_VALIDATED_IN_HANDLER),
            );
            None
        }
        Err(failure) => {
            let reason_code = failure.code();
            let applied_state = match failure {
                RedactArgsFailure::ModeUnsupported | RedactArgsFailure::ScopeUnsupported => {
                    "not_evaluated"
                }
                RedactArgsFailure::TargetMissing | RedactArgsFailure::ApplyFailed => "not_applied",
            };
            tool_match.redaction_applied_state = Some(applied_state.to_string());
            tool_match.redact_args_result = Some(applied_state.to_string());
            tool_match.redaction_failure_reason = Some(reason_code.to_string());
            tool_match.redaction_reason = Some(reason_code.to_string());
            tool_match.redact_args_reason = Some(reason_code.to_string());
            Some(mark_redact_args_failure(tool_match, failure))
        }
    }
}

fn apply_redact_args_runtime(
    args: &mut Value,
    contract: &RedactArgsContract,
) -> Result<(), RedactArgsFailure> {
    let target = contract.redaction_target.trim().to_ascii_lowercase();
    let mode = contract.redaction_mode.trim().to_ascii_lowercase();
    let scope = contract.redaction_scope.trim().to_ascii_lowercase();

    if !matches!(scope.as_str(), "request" | "args" | "metadata") {
        return Err(RedactArgsFailure::ScopeUnsupported);
    }

    if mode == "drop" {
        return apply_drop_redaction(args, &target);
    }

    let Some(target_value) = redaction_target_value_mut(args, &target) else {
        return Err(RedactArgsFailure::TargetMissing);
    };

    apply_value_redaction(target_value, &mode)
}

fn apply_drop_redaction(args: &mut Value, target: &str) -> Result<(), RedactArgsFailure> {
    match target {
        "path" | "query" | "headers" | "body" => args
            .as_object_mut()
            .ok_or(RedactArgsFailure::ApplyFailed)?
            .remove(target)
            .map(|_| ())
            .ok_or(RedactArgsFailure::TargetMissing),
        "metadata" => args
            .as_object_mut()
            .ok_or(RedactArgsFailure::ApplyFailed)?
            .remove("_meta")
            .map(|_| ())
            .ok_or(RedactArgsFailure::TargetMissing),
        "args" => {
            *args = Value::Object(Map::new());
            Ok(())
        }
        _ => Err(RedactArgsFailure::TargetMissing),
    }
}

fn redaction_target_value_mut<'a>(args: &'a mut Value, target: &str) -> Option<&'a mut Value> {
    match target {
        "path" => args.get_mut("path"),
        "query" => args.get_mut("query"),
        "headers" => args.get_mut("headers"),
        "body" => args.get_mut("body"),
        "metadata" => args.get_mut("_meta"),
        "args" => Some(args),
        _ => None,
    }
}

fn apply_value_redaction(target_value: &mut Value, mode: &str) -> Result<(), RedactArgsFailure> {
    match mode {
        "mask" => {
            *target_value = Value::String("[REDACTED]".to_string());
            Ok(())
        }
        "hash" => {
            let input = target_value.to_string();
            *target_value = Value::String(format!("md5:{:x}", md5::compute(input)));
            Ok(())
        }
        "partial" => {
            let Some(input) = target_value.as_str() else {
                return Err(RedactArgsFailure::ApplyFailed);
            };
            *target_value = Value::String(partial_mask(input));
            Ok(())
        }
        _ => Err(RedactArgsFailure::ModeUnsupported),
    }
}

fn partial_mask(input: &str) -> String {
    if input.is_empty() {
        return "***".to_string();
    }
    let keep = input.chars().take(2).collect::<String>();
    format!("{keep}***")
}

fn mark_redact_args_failure(
    tool_match: &mut emit::ToolMatchMetadata,
    failure: RedactArgsFailure,
) -> RedactArgsFailure {
    let failure_code = failure.code().to_string();
    tool_match.redaction_failure_reason = Some(failure_code.clone());
    if tool_match.redaction_applied_state.is_none() {
        tool_match.redaction_applied_state = Some("not_evaluated".to_string());
    }
    if tool_match.redact_args_result.is_none() {
        tool_match.redact_args_result = tool_match.redaction_applied_state.clone();
    }
    tool_match.redaction_reason = Some(failure_code.clone());
    tool_match.redact_args_reason = Some(failure_code.clone());
    mark_redact_args_outcome(
        tool_match,
        ObligationOutcomeStatus::Error,
        Some(failure_code.as_str()),
        Some(failure_code.as_str()),
    );
    failure
}

fn mark_redact_args_outcome(
    tool_match: &mut emit::ToolMatchMetadata,
    status: ObligationOutcomeStatus,
    reason: Option<&str>,
    reason_code: Option<&str>,
) {
    if let Some(outcome) = tool_match
        .obligation_outcomes
        .iter_mut()
        .find(|outcome| outcome.obligation_type == "redact_args")
    {
        outcome.status = status;
        outcome.reason = reason.map(ToString::to_string);
        outcome.reason_code = reason_code.map(ToString::to_string);
        outcome.enforcement_stage = Some(OUTCOME_STAGE_HANDLER.to_string());
        outcome.normalization_version = Some(OUTCOME_NORMALIZATION_VERSION.to_string());
        return;
    }

    tool_match.obligation_outcomes.push(ObligationOutcome {
        obligation_type: "redact_args".to_string(),
        status,
        reason: reason.map(ToString::to_string),
        reason_code: reason_code.map(ToString::to_string),
        enforcement_stage: Some(OUTCOME_STAGE_HANDLER.to_string()),
        normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
    });
}
