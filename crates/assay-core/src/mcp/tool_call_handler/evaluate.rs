use super::super::decision::{reason_codes, DecisionEmitterGuard};
use super::super::identity::ToolIdentity;
use super::super::jsonrpc::JsonRpcRequest;
use super::super::lifecycle::mandate_used_event;
use super::super::obligations;
use super::super::policy::{
    ApprovalArtifact, ApprovalFreshness, FailClosedContext, FailClosedMode, FailClosedTrigger,
    PolicyDecision, PolicyState, ToolRiskClass,
};
use super::emit;
use super::types::{HandleResult, ToolCallHandler};
use crate::runtime::{AuthorizeError, MandateData, OperationClass, ToolCallData};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::time::Instant;

const OUTCOME_NORMALIZATION_VERSION: &str = "v1";
const OUTCOME_STAGE_HANDLER: &str = "handler";
const OUTCOME_REASON_VALIDATED_IN_HANDLER: &str = "validated_in_handler";
const FAIL_CLOSED_RUNTIME_DEPENDENCY_ERROR: &str = "fail_closed_runtime_dependency_error";
const DEGRADE_READ_ONLY_RUNTIME_DEPENDENCY_ERROR: &str =
    "degrade_read_only_runtime_dependency_error";

pub(super) fn handle_tool_call(
    handler: &ToolCallHandler,
    request: &JsonRpcRequest,
    state: &mut PolicyState,
    runtime_identity: Option<&ToolIdentity>,
    mandate: Option<&MandateData>,
    transaction_object: Option<&Value>,
) -> HandleResult {
    let params = match request.tool_params() {
        Some(p) => p,
        None => {
            // Not a tool call - still must emit decision (I1 invariant)
            let tool_call_id = handler.extract_tool_call_id(request);
            let guard = DecisionEmitterGuard::new(
                handler.emitter.clone(),
                handler.config.event_source.clone(),
                tool_call_id.clone(),
                "unknown".to_string(),
            );
            guard.emit_error(
                reason_codes::S_INTERNAL_ERROR,
                Some("Not a tool call".to_string()),
            );

            return emit::error_not_tool_call(&handler.config.event_source, tool_call_id);
        }
    };

    let tool_name = params.name.clone();
    let tool_call_id = handler.extract_tool_call_id(request);

    // Create guard - ensures decision is ALWAYS emitted
    let mut guard = DecisionEmitterGuard::new(
        handler.emitter.clone(),
        handler.config.event_source.clone(),
        tool_call_id.clone(),
        tool_name.clone(),
    );
    guard.set_request_id(request.id.clone());

    let start = Instant::now();
    let original_arguments = params.arguments.clone();
    let mut effective_arguments = original_arguments.clone();

    // Step 1: Policy evaluation
    let policy_eval = handler.policy.evaluate_with_metadata(
        &tool_name,
        &effective_arguments,
        state,
        runtime_identity,
    );
    let mut tool_match = emit::ToolMatchMetadata::from_policy_metadata(&policy_eval.metadata);
    tool_match.obligation_outcomes =
        obligations::execute_log_only(&tool_match.obligations, &tool_name);
    seed_fail_closed_context(
        &mut tool_match,
        handler.operation_class_for_tool(&tool_name),
    );
    guard.set_tool_match(
        tool_match.tool_classes.clone(),
        tool_match.matched_tool_classes.clone(),
        tool_match.match_basis.clone(),
        tool_match.matched_rule.clone(),
    );
    guard.set_policy_context(tool_match.policy_context());

    match policy_eval.decision {
        PolicyDecision::Deny {
            tool: _,
            code,
            reason,
            contract: _,
        } => {
            let reason_code = handler.map_policy_code_to_reason(&code);
            guard.set_policy_context(tool_match.policy_context());
            guard.emit_deny(&reason_code, Some(reason.clone()));

            return emit::deny(
                &handler.config.event_source,
                tool_call_id,
                tool_name,
                &reason_code,
                reason,
                tool_match,
            );
        }
        PolicyDecision::AllowWithWarning { .. } | PolicyDecision::Allow => {
            // Continue to mandate check
        }
    }

    // Step 2: approval_required obligation enforcement (Wave28)
    if let Some(failure) =
        validate_approval_required(&tool_name, &effective_arguments, &mut tool_match)
    {
        let reason = failure.to_string();
        guard.set_policy_context(tool_match.policy_context());
        guard.emit_deny(reason_codes::P_APPROVAL_REQUIRED, Some(reason.clone()));

        return emit::deny(
            &handler.config.event_source,
            tool_call_id,
            tool_name,
            reason_codes::P_APPROVAL_REQUIRED,
            reason,
            tool_match,
        );
    }

    // Step 3: restrict_scope obligation enforcement (Wave30)
    if let Some(failure) = validate_restrict_scope(&mut tool_match) {
        let reason = failure.to_string();
        guard.set_policy_context(tool_match.policy_context());
        guard.emit_deny(reason_codes::P_RESTRICT_SCOPE, Some(reason.clone()));

        return emit::deny(
            &handler.config.event_source,
            tool_call_id,
            tool_name,
            reason_codes::P_RESTRICT_SCOPE,
            reason,
            tool_match,
        );
    }

    // Step 4: redact_args obligation enforcement (Wave32)
    if let Some(failure) = validate_redact_args(&mut effective_arguments, &mut tool_match) {
        let reason = failure.to_string();
        guard.set_policy_context(tool_match.policy_context());
        guard.emit_deny(reason_codes::P_REDACT_ARGS, Some(reason.clone()));

        return emit::deny(
            &handler.config.event_source,
            tool_call_id,
            tool_name,
            reason_codes::P_REDACT_ARGS,
            reason,
            tool_match,
        );
    }

    // Step 5: Check if mandate is required
    let is_commit_tool = handler.is_commit_tool(&tool_name);
    if is_commit_tool && handler.config.require_mandate_for_commit && mandate.is_none() {
        let reason = "Commit tool requires mandate authorization".to_string();
        guard.set_policy_context(tool_match.policy_context());
        guard.emit_deny(reason_codes::P_MANDATE_REQUIRED, Some(reason.clone()));

        return emit::deny(
            &handler.config.event_source,
            tool_call_id,
            tool_name,
            reason_codes::P_MANDATE_REQUIRED,
            reason,
            tool_match,
        );
    }

    // Step 6: Mandate authorization (if mandate present)
    if let (Some(authorizer), Some(mandate_data)) = (&handler.authorizer, mandate) {
        let operation_class = handler.operation_class_for_tool(&tool_name);

        let tool_call_data = ToolCallData {
            tool_name: tool_name.clone(),
            tool_call_id: tool_call_id.clone(),
            operation_class,
            transaction_object: transaction_object.cloned(),
            source_run_id: None,
        };

        let authz_start = Instant::now();
        match authorizer.authorize_and_consume(mandate_data, &tool_call_data) {
            Ok(receipt) => {
                let authz_ms = authz_start.elapsed().as_millis() as u64;
                guard.set_mandate_info(
                    Some(mandate_data.mandate_id.clone()),
                    Some(receipt.use_id.clone()),
                    Some(receipt.use_count),
                );
                guard.set_mandate_matches(Some(true), Some(true), Some(true));
                guard.set_latencies(Some(authz_ms), None);
                guard.set_policy_context(tool_match.policy_context());
                guard.emit_allow(reason_codes::P_MANDATE_VALID);

                // Emit mandate.used lifecycle event (P0-B)
                // Only emit on first consumption, not on idempotent retries
                if receipt.was_new {
                    if let Some(ref lifecycle) = handler.lifecycle_emitter {
                        let event = mandate_used_event(&handler.config.event_source, &receipt);
                        lifecycle.emit(&event);
                    }
                }

                return emit::allow(
                    &handler.config.event_source,
                    tool_call_id,
                    tool_name,
                    reason_codes::P_MANDATE_VALID,
                    Some(receipt),
                    (effective_arguments != original_arguments).then_some(effective_arguments),
                    tool_match,
                );
            }
            Err(e) => {
                let (reason_code, reason) = handler.map_authz_error(&e);
                if reason_code == reason_codes::S_DB_ERROR {
                    let fail_code = runtime_dependency_error_code(&tool_match).to_string();
                    mark_fail_closed(
                        &mut tool_match,
                        FailClosedTrigger::RuntimeDependencyError,
                        fail_code,
                    );
                }
                guard.set_mandate_info(Some(mandate_data.mandate_id.clone()), None, None);
                guard.set_policy_context(tool_match.policy_context());
                guard.emit_deny(&reason_code, Some(reason.clone()));

                return emit::deny(
                    &handler.config.event_source,
                    tool_call_id,
                    tool_name,
                    &reason_code,
                    reason,
                    tool_match,
                );
            }
        }
    }

    // Step 7: No mandate required, policy allows
    let elapsed_ms = start.elapsed().as_millis() as u64;
    guard.set_latencies(Some(elapsed_ms), None);
    guard.set_policy_context(tool_match.policy_context());
    guard.emit_allow(reason_codes::P_POLICY_ALLOW);

    emit::allow(
        &handler.config.event_source,
        tool_call_id,
        tool_name,
        reason_codes::P_POLICY_ALLOW,
        None,
        (effective_arguments != original_arguments).then_some(effective_arguments),
        tool_match,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalFailure {
    MissingApproval,
    ExpiredApproval,
    BoundToolMismatch,
    BoundResourceMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestrictScopeFailure {
    TargetMissing,
    TargetMismatch,
    MatchModeUnsupported,
    TypeUnsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RedactArgsFailure {
    TargetMissing,
    ModeUnsupported,
    ScopeUnsupported,
    ApplyFailed,
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

impl std::fmt::Display for RestrictScopeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_reason())
    }
}

impl std::fmt::Display for RedactArgsFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_reason())
    }
}

fn validate_approval_required(
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
        super::super::decision::ObligationOutcomeStatus::Applied,
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
        super::super::decision::ObligationOutcomeStatus::Error,
        Some(failure.as_reason()),
        Some(failure.code()),
    );
    failure
}

fn mark_approval_outcome(
    tool_match: &mut emit::ToolMatchMetadata,
    status: super::super::decision::ObligationOutcomeStatus,
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

    tool_match
        .obligation_outcomes
        .push(super::super::decision::ObligationOutcome {
            obligation_type: "approval_required".to_string(),
            status,
            reason: reason.map(ToString::to_string),
            reason_code: reason_code.map(ToString::to_string),
            enforcement_stage: Some(OUTCOME_STAGE_HANDLER.to_string()),
            normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
        });
}

fn validate_restrict_scope(
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
        mark_restrict_scope_outcome(
            tool_match,
            super::super::decision::ObligationOutcomeStatus::Applied,
            None,
            None,
        );
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

fn validate_redact_args(
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
                super::super::decision::ObligationOutcomeStatus::Applied,
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
    contract: &super::super::policy::RedactArgsContract,
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
            *args = Value::Object(serde_json::Map::new());
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
        super::super::decision::ObligationOutcomeStatus::Error,
        Some(failure_code.as_str()),
        Some(failure_code.as_str()),
    );
    failure
}

fn mark_restrict_scope_outcome(
    tool_match: &mut emit::ToolMatchMetadata,
    status: super::super::decision::ObligationOutcomeStatus,
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

    tool_match
        .obligation_outcomes
        .push(super::super::decision::ObligationOutcome {
            obligation_type: "restrict_scope".to_string(),
            status,
            reason: reason.map(ToString::to_string),
            reason_code: reason_code.map(ToString::to_string),
            enforcement_stage: Some(OUTCOME_STAGE_HANDLER.to_string()),
            normalization_version: Some(OUTCOME_NORMALIZATION_VERSION.to_string()),
        });
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
        super::super::decision::ObligationOutcomeStatus::Error,
        Some(failure_code.as_str()),
        Some(failure_code.as_str()),
    );
    failure
}

fn mark_redact_args_outcome(
    tool_match: &mut emit::ToolMatchMetadata,
    status: super::super::decision::ObligationOutcomeStatus,
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

    tool_match
        .obligation_outcomes
        .push(super::super::decision::ObligationOutcome {
            obligation_type: "redact_args".to_string(),
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

fn requested_resource(args: &Value) -> Option<&str> {
    args.get("_meta")
        .and_then(|meta| meta.get("resource"))
        .and_then(Value::as_str)
        .or_else(|| args.get("resource").and_then(Value::as_str))
}

fn seed_fail_closed_context(tool_match: &mut emit::ToolMatchMetadata, op: OperationClass) {
    let tool_risk_class = match op {
        OperationClass::Read => ToolRiskClass::LowRiskRead,
        OperationClass::Write | OperationClass::Commit => ToolRiskClass::HighRisk,
    };
    let fail_closed_mode = match tool_risk_class {
        ToolRiskClass::LowRiskRead => FailClosedMode::DegradeReadOnly,
        ToolRiskClass::HighRisk | ToolRiskClass::Default => FailClosedMode::FailClosed,
    };
    tool_match.fail_closed = Some(FailClosedContext {
        tool_risk_class,
        fail_closed_mode,
        fail_closed_trigger: None,
        fail_closed_applied: false,
        fail_closed_error_code: None,
    });
}

fn runtime_dependency_error_code(tool_match: &emit::ToolMatchMetadata) -> &'static str {
    match tool_match
        .fail_closed
        .as_ref()
        .map(|ctx| ctx.fail_closed_mode)
        .unwrap_or(FailClosedMode::FailClosed)
    {
        FailClosedMode::DegradeReadOnly => DEGRADE_READ_ONLY_RUNTIME_DEPENDENCY_ERROR,
        FailClosedMode::FailClosed | FailClosedMode::FailSafeAllow => {
            FAIL_CLOSED_RUNTIME_DEPENDENCY_ERROR
        }
    }
}

fn mark_fail_closed(
    tool_match: &mut emit::ToolMatchMetadata,
    trigger: FailClosedTrigger,
    error_code: String,
) {
    if let Some(ctx) = tool_match.fail_closed.as_mut() {
        ctx.fail_closed_trigger = Some(trigger);
        ctx.fail_closed_applied = true;
        ctx.fail_closed_error_code = Some(error_code);
    }
}

impl ToolCallHandler {
    /// Extract tool_call_id from request (I4: idempotency key).
    pub(super) fn extract_tool_call_id(&self, request: &JsonRpcRequest) -> String {
        // Try to get from params._meta.tool_call_id (MCP standard)
        if let Some(params) = request.tool_params() {
            if let Some(meta) = params.arguments.get("_meta") {
                if let Some(id) = meta.get("tool_call_id").and_then(|v| v.as_str()) {
                    return id.to_string();
                }
            }
        }

        // Fall back to request.id if present
        if let Some(id) = &request.id {
            if let Some(s) = id.as_str() {
                return format!("req_{}", s);
            }
            if let Some(n) = id.as_i64() {
                return format!("req_{}", n);
            }
        }

        // Generate one if none found
        format!("gen_{}", uuid::Uuid::new_v4())
    }

    /// Check if a tool is classified as a commit operation.
    pub(super) fn is_commit_tool(&self, tool_name: &str) -> bool {
        self.config.commit_tools.iter().any(|pattern| {
            if pattern == "*" {
                return true;
            }
            if pattern.ends_with('*') {
                let prefix = pattern.trim_end_matches('*');
                tool_name.starts_with(prefix)
            } else {
                tool_name == pattern
            }
        })
    }

    /// Check if a tool is classified as a write operation (non-commit).
    fn is_write_tool(&self, tool_name: &str) -> bool {
        self.config.write_tools.iter().any(|pattern| {
            if pattern == "*" {
                return true;
            }
            if pattern.ends_with('*') {
                let prefix = pattern.trim_end_matches('*');
                tool_name.starts_with(prefix)
            } else {
                tool_name == pattern
            }
        })
    }

    /// Derive operation class from tool classification (commit_tools, write_tools, else Read).
    pub(super) fn operation_class_for_tool(&self, tool_name: &str) -> OperationClass {
        if self.is_commit_tool(tool_name) {
            OperationClass::Commit
        } else if self.is_write_tool(tool_name) {
            OperationClass::Write
        } else {
            OperationClass::Read
        }
    }

    /// Map policy error code to reason code.
    pub(super) fn map_policy_code_to_reason(&self, code: &str) -> String {
        match code {
            "E_TOOL_DENIED" => reason_codes::P_TOOL_DENIED.to_string(),
            "E_TOOL_NOT_ALLOWED" => reason_codes::P_TOOL_NOT_ALLOWED.to_string(),
            "E_ARG_SCHEMA" => reason_codes::P_ARG_SCHEMA.to_string(),
            "E_RATE_LIMIT" => reason_codes::P_RATE_LIMIT.to_string(),
            "E_TOOL_DRIFT" => reason_codes::P_TOOL_DRIFT.to_string(),
            _ => reason_codes::P_POLICY_DENY.to_string(),
        }
    }

    /// Map authorization error to reason code and message.
    pub(super) fn map_authz_error(&self, error: &AuthorizeError) -> (String, String) {
        match error {
            AuthorizeError::Policy(pe) => {
                use crate::runtime::PolicyError;
                match pe {
                    PolicyError::Expired { .. } => (
                        reason_codes::M_EXPIRED.to_string(),
                        "Mandate expired".to_string(),
                    ),
                    PolicyError::NotYetValid { .. } => (
                        reason_codes::M_NOT_YET_VALID.to_string(),
                        "Mandate not yet valid".to_string(),
                    ),
                    PolicyError::ToolNotInScope { tool } => (
                        reason_codes::M_TOOL_NOT_IN_SCOPE.to_string(),
                        format!("Tool '{}' not in mandate scope", tool),
                    ),
                    PolicyError::KindMismatch { kind, op_class } => (
                        reason_codes::M_KIND_MISMATCH.to_string(),
                        format!(
                            "Mandate kind '{}' does not allow operation class '{}'",
                            kind, op_class
                        ),
                    ),
                    PolicyError::AudienceMismatch { expected, actual } => (
                        reason_codes::M_AUDIENCE_MISMATCH.to_string(),
                        format!(
                            "Audience mismatch: expected '{}', got '{}'",
                            expected, actual
                        ),
                    ),
                    PolicyError::IssuerNotTrusted { issuer } => (
                        reason_codes::M_ISSUER_NOT_TRUSTED.to_string(),
                        format!("Issuer '{}' not in trusted list", issuer),
                    ),
                    PolicyError::MissingTransactionObject => (
                        reason_codes::M_TRANSACTION_REF_MISMATCH.to_string(),
                        "Transaction object required but not provided".to_string(),
                    ),
                    PolicyError::TransactionRefMismatch { expected, actual } => (
                        reason_codes::M_TRANSACTION_REF_MISMATCH.to_string(),
                        format!(
                            "Transaction ref mismatch: expected '{}', computed '{}'",
                            expected, actual
                        ),
                    ),
                }
            }
            AuthorizeError::Store(se) => {
                use crate::runtime::AuthzError;
                match se {
                    AuthzError::AlreadyUsed => (
                        reason_codes::M_ALREADY_USED.to_string(),
                        "Single-use mandate already consumed".to_string(),
                    ),
                    AuthzError::MaxUsesExceeded { max, current } => (
                        reason_codes::M_MAX_USES_EXCEEDED.to_string(),
                        format!("Max uses exceeded: {} of {} used", current, max),
                    ),
                    AuthzError::NonceReplay { nonce } => (
                        reason_codes::M_NONCE_REPLAY.to_string(),
                        format!("Nonce replay detected: {}", nonce),
                    ),
                    AuthzError::MandateNotFound { mandate_id } => (
                        reason_codes::M_NOT_FOUND.to_string(),
                        format!("Mandate not found: {}", mandate_id),
                    ),
                    AuthzError::Revoked { revoked_at } => (
                        reason_codes::M_REVOKED.to_string(),
                        format!("Mandate revoked at {}", revoked_at),
                    ),
                    AuthzError::MandateConflict { .. }
                    | AuthzError::InvalidConstraints { .. }
                    | AuthzError::Database(_) => (
                        reason_codes::S_DB_ERROR.to_string(),
                        format!("Database error: {}", se),
                    ),
                }
            }
            AuthorizeError::TransactionRef(msg) => (
                reason_codes::M_TRANSACTION_REF_MISMATCH.to_string(),
                format!("Transaction ref error: {}", msg),
            ),
        }
    }
}
