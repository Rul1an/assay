//! `assay mcp wrap` enforces obligations through the handler's function and records the result on
//! its own metadata type.

use super::super::decision::{ObligationOutcome, ObligationOutcomeStatus};
use super::super::obligations;
use super::super::policy::{PolicyDecision, PolicyMatchMetadata, TypedPolicyDecision};
use super::emit::ToolMatchMetadata;
use super::evaluate_next::{
    enforce::enforce_obligations, OUTCOME_NORMALIZATION_VERSION, OUTCOME_STAGE_HANDLER,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

const OUTCOME_REASON_DRY_RUN: &str = "dry_run";

pub(in crate::mcp) struct WrapEnforcement {
    /// Replaces the policy decision: the call must not reach the server.
    pub(in crate::mcp) denial: Option<PolicyDecision>,
    /// Arguments to forward in place of the client's, when redaction changed them.
    pub(in crate::mcp) effective_arguments: Option<Value>,
    /// The client's line holds what redaction covers, so it must not be retained.
    pub(in crate::mcp) withhold_request_line: bool,
    /// The record may name this call only by what the arguments do not hold.
    pub(in crate::mcp) record_omits_arguments: bool,
    pub(in crate::mcp) outcomes: Vec<ObligationOutcome>,
}

pub(in crate::mcp) fn enforce_for_wrap(
    tool_name: &str,
    arguments: &Value,
    redaction_covered: bool,
    decision: &PolicyDecision,
    metadata: &mut PolicyMatchMetadata,
    now: DateTime<Utc>,
    dry_run: bool,
) -> WrapEnforcement {
    let outcomes = obligations::execute_log_only(&metadata.obligations, tool_name);
    if let PolicyDecision::Deny {
        tool, code, reason, ..
    } = decision
    {
        if redaction_covered {
            drop_argument_values_from_record(metadata);
        }
        return WrapEnforcement {
            // A schema violation quotes the value it rejects, and this reply and the audit log
            // keep what the contract holds.
            denial: redaction_covered.then(|| PolicyDecision::Deny {
                tool: tool.clone(),
                code: code.clone(),
                reason: reason.clone(),
                contract: json!({ "status": "deny", "error_code": code }),
            }),
            effective_arguments: None,
            withhold_request_line: redaction_covered,
            record_omits_arguments: redaction_covered,
            outcomes,
        };
    }

    let mut tool_match = ToolMatchMetadata::from_policy_metadata(metadata);
    tool_match.obligation_outcomes = outcomes;
    let mut effective = arguments.clone();
    let denial = enforce_obligations(tool_name, &mut effective, &mut tool_match, now);
    if dry_run {
        mark_redaction_not_applied_for_dry_run(&mut tool_match);
    }

    let redacted = tool_match.redaction_applied_state.as_deref() == Some("applied");
    let withhold_request_line = redaction_covered && !redacted;
    let effective_arguments =
        (denial.is_none() && !dry_run && effective != *arguments).then_some(effective);
    write_back(&tool_match, metadata);
    let refused = denial.is_some() && !dry_run;
    if refused {
        metadata.typed_decision = Some(TypedPolicyDecision::Deny);
    }
    let record_omits_arguments =
        redaction_covers_metadata(metadata) || (redaction_covered && refused);
    if record_omits_arguments {
        drop_argument_values_from_record(metadata);
    }
    WrapEnforcement {
        denial: denial.map(|denial| PolicyDecision::Deny {
            tool: tool_name.to_string(),
            code: denial.reason_code.to_string(),
            contract: json!({
                "status": "deny",
                "error_code": denial.reason_code,
                "reason": denial.reason,
            }),
            reason: denial.reason,
        }),
        effective_arguments,
        withhold_request_line,
        record_omits_arguments,
        outcomes: tool_match.obligation_outcomes,
    }
}

/// The `args` and `metadata` targets include `_meta`, which the policy and the proxy read from.
fn redaction_covers_metadata(metadata: &PolicyMatchMetadata) -> bool {
    metadata.redaction_target.as_deref().is_some_and(|target| {
        matches!(
            target.trim().to_ascii_lowercase().as_str(),
            "args" | "metadata"
        )
    })
}

/// Every metadata field holding a value taken from the arguments. The tool call id is the one other
/// such value in a record, and `record_omits_arguments` covers it at its source.
fn drop_argument_values_from_record(metadata: &mut PolicyMatchMetadata) {
    if let Some(target) = metadata.restrict_scope_target.as_mut() {
        if target.starts_with("resource:") {
            *target = "resource:<redacted>".to_string();
        }
    }
    metadata.approval_artifact = None;
    metadata.delegated_from = None;
    metadata.delegation_depth = None;
}

/// A dry run changes no traffic, so the record cannot say a redaction happened.
fn mark_redaction_not_applied_for_dry_run(tool_match: &mut ToolMatchMetadata) {
    if tool_match.redaction_applied_state.as_deref() != Some("applied") {
        return;
    }
    tool_match.redaction_applied_state = Some("not_applied".to_string());
    tool_match.redact_args_result = Some("not_applied".to_string());
    if let Some(outcome) = tool_match
        .obligation_outcomes
        .iter_mut()
        .find(|outcome| outcome.obligation_type == "redact_args")
    {
        outcome.status = ObligationOutcomeStatus::Skipped;
        outcome.reason = Some("dry run forwards the call unchanged".to_string());
        outcome.reason_code = Some(OUTCOME_REASON_DRY_RUN.to_string());
        outcome.enforcement_stage = Some(OUTCOME_STAGE_HANDLER.to_string());
        outcome.normalization_version = Some(OUTCOME_NORMALIZATION_VERSION.to_string());
    }
}

fn write_back(tool_match: &ToolMatchMetadata, metadata: &mut PolicyMatchMetadata) {
    metadata.approval_state = tool_match.approval_state.clone();
    metadata.approval_artifact = tool_match.approval_artifact.clone();
    metadata.approval_freshness = tool_match.approval_freshness;
    metadata.approval_failure_reason = tool_match.approval_failure_reason.clone();
    metadata.scope_evaluation_state = tool_match.scope_evaluation_state.clone();
    metadata.scope_failure_reason = tool_match.scope_failure_reason.clone();
    metadata.restrict_scope_match = tool_match.restrict_scope_match;
    metadata.restrict_scope_reason = tool_match.restrict_scope_reason.clone();
    metadata.redaction_applied_state = tool_match.redaction_applied_state.clone();
    metadata.redaction_reason = tool_match.redaction_reason.clone();
    metadata.redaction_failure_reason = tool_match.redaction_failure_reason.clone();
    metadata.redact_args_result = tool_match.redact_args_result.clone();
    metadata.redact_args_reason = tool_match.redact_args_reason.clone();
}
