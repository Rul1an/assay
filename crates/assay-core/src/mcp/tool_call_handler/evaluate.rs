use super::super::decision::{reason_codes, DecisionEmitterGuard};
use super::super::identity::ToolIdentity;
use super::super::jsonrpc::JsonRpcRequest;
use super::super::lifecycle::mandate_used_event;
use super::super::obligations;
use super::super::policy::{FailClosedTrigger, PolicyDecision, PolicyState};
use super::emit;
use super::evaluate_next::{
    approval::validate_approval_required,
    fail_closed::{mark_fail_closed, runtime_dependency_error_code, seed_fail_closed_context},
    redaction::validate_redact_args,
    scope::validate_restrict_scope,
};
use super::types::{HandleResult, ToolCallHandler};
use crate::runtime::{MandateData, ToolCallData};
use serde_json::Value;
use std::time::Instant;

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
    let mut policy_eval = handler.policy.evaluate_with_metadata(
        &tool_name,
        &effective_arguments,
        state,
        runtime_identity,
    );
    if let Some(proj) = &handler.config.auth_context_projection {
        proj.merge_into_metadata(&mut policy_eval.metadata);
    }
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
