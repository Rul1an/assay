use super::super::identity::ToolIdentity;
use super::super::jsonrpc::JsonRpcRequest;
use super::engine_next::{
    apply_allow_precedence, apply_approval_required_obligation, apply_delegation_context,
    apply_redact_args_obligation, apply_restrict_scope_obligation, check_rate_limits,
    deny_match_decision, finalize_evaluation, schema_violation_decision, tool_drift_decision,
    unconstrained_decision,
};
use super::{McpPolicy, PolicyDecision, PolicyEvaluation, PolicyMatchMetadata, PolicyState};
use serde_json::Value;

pub(super) fn evaluate_with_metadata(
    policy: &McpPolicy,
    tool_name: &str,
    args: &Value,
    state: &mut PolicyState,
    runtime_identity: Option<&ToolIdentity>,
) -> PolicyEvaluation {
    let tool_classes = policy.tool_taxonomy.classes_for(tool_name);
    let tool_classes_vec: Vec<String> = tool_classes.iter().cloned().collect();
    let mut metadata = PolicyMatchMetadata {
        tool_classes: tool_classes_vec,
        ..PolicyMatchMetadata::default()
    };
    apply_delegation_context(args, &mut metadata);

    if let Some(decision) = tool_drift_decision(policy, tool_name, runtime_identity) {
        return finalize_evaluation(policy, metadata, decision);
    }

    if let Some(decision) = check_rate_limits(policy, state) {
        return finalize_evaluation(policy, metadata, decision);
    }

    if let Some(decision) = deny_match_decision(policy, tool_name, &tool_classes, &mut metadata) {
        return finalize_evaluation(policy, metadata, decision);
    }

    if let Some(decision) = apply_allow_precedence(policy, tool_name, &tool_classes, &mut metadata)
    {
        return finalize_evaluation(policy, metadata, decision);
    }

    let compiled = policy.compiled_schemas();
    if let Some(validator) = compiled.get(tool_name) {
        if let Some(decision) = schema_violation_decision(tool_name, args, validator.as_ref()) {
            return finalize_evaluation(policy, metadata, decision);
        }

        let decision = PolicyDecision::Allow;
        apply_approval_required_obligation(
            policy,
            tool_name,
            &tool_classes,
            &decision,
            &mut metadata,
        );
        apply_restrict_scope_obligation(
            policy,
            tool_name,
            args,
            &tool_classes,
            &decision,
            &mut metadata,
        );
        apply_redact_args_obligation(
            policy,
            tool_name,
            args,
            &tool_classes,
            &decision,
            &mut metadata,
        );
        return finalize_evaluation(policy, metadata, decision);
    }

    let decision = unconstrained_decision(policy, tool_name);

    apply_approval_required_obligation(policy, tool_name, &tool_classes, &decision, &mut metadata);
    apply_restrict_scope_obligation(
        policy,
        tool_name,
        args,
        &tool_classes,
        &decision,
        &mut metadata,
    );
    apply_redact_args_obligation(
        policy,
        tool_name,
        args,
        &tool_classes,
        &decision,
        &mut metadata,
    );
    finalize_evaluation(policy, metadata, decision)
}

pub(super) fn check(
    policy: &McpPolicy,
    request: &JsonRpcRequest,
    state: &mut PolicyState,
) -> PolicyDecision {
    if !request.is_tool_call() {
        state.requests_count += 1;
        return PolicyDecision::Allow;
    }
    if let Some(params) = request.tool_params() {
        // evaluate() increments counts, so we don't need to increment requests_count here
        // Note: In strict mode, we might want to pass the runtime identity here.
        // For now, identity check is performed by the proxy which manages the identity cache.
        policy.evaluate(&params.name, &params.arguments, state, None)
    } else {
        // Ordinary request, just count it
        state.requests_count += 1;
        PolicyDecision::Allow
    }
}

#[cfg(test)]
mod tests {
    use crate::mcp::policy::engine_next::parse_delegation_context;
    use serde_json::json;

    #[test]
    fn parse_delegation_context_requires_explicit_delegated_from() {
        let args = json!({
            "_meta": {
                "delegation": {
                    "note": "planner forwarded the request"
                }
            }
        });

        assert!(parse_delegation_context(&args).is_none());
    }

    #[test]
    fn parse_delegation_context_uses_explicit_depth_only() {
        let args = json!({
            "_meta": {
                "delegation": {
                    "delegated_from": "agent:planner",
                    "delegation_depth": 1
                }
            }
        });

        let parsed = parse_delegation_context(&args).expect("expected delegation context");
        assert_eq!(parsed.delegated_from, "agent:planner");
        assert_eq!(parsed.delegation_depth, Some(1));
    }

    #[test]
    fn parse_delegation_context_does_not_infer_depth_from_invalid_value() {
        let args = json!({
            "_meta": {
                "delegation": {
                    "delegated_from": "agent:planner",
                    "delegation_depth": "1"
                }
            }
        });

        let parsed = parse_delegation_context(&args).expect("expected delegation context");
        assert_eq!(parsed.delegated_from, "agent:planner");
        assert_eq!(parsed.delegation_depth, None);
    }
}
