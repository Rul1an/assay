use super::super::identity::ToolIdentity;
use super::super::jsonrpc::JsonRpcRequest;
use super::super::tool_match::MatchBasis;
use super::{
    matches_tool_pattern, McpPolicy, PolicyDecision, PolicyEvaluation, PolicyMatchMetadata,
    PolicyObligation, PolicyState, RedactArgsContract, RestrictScopeContract, UnconstrainedMode,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;

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

    // 0. Tool Integrity Check (Phase 9)
    if let Some(pinned) = policy.tool_pins.get(tool_name) {
        if let Some(runtime) = runtime_identity {
            if pinned != runtime {
                return finalize_evaluation(
                    policy,
                    metadata,
                    PolicyDecision::Deny {
                        tool: tool_name.to_string(),
                        code: "E_TOOL_DRIFT".to_string(),
                        reason: format!(
                            "Tool integrity failure: identity drifted from pinned version. (Runtime: {}, Pinned: {})",
                            runtime.fingerprint(),
                            pinned.fingerprint()
                        ),
                        contract: format_deny_contract(
                            tool_name,
                            "E_TOOL_DRIFT",
                            "Tool metadata or schema has changed without policy update (SOTA Moat)",
                        ),
                    },
                );
            }
        }
    }

    // 1. Rate limits
    if let Some(decision) = check_rate_limits(policy, state) {
        return finalize_evaluation(policy, metadata, decision);
    }

    let deny_name_match = is_denied(policy, tool_name);
    let deny_class_matches = matched_deny_classes(policy, &tool_classes);
    if deny_name_match || !deny_class_matches.is_empty() {
        metadata.matched_tool_classes = deny_class_matches.clone();
        metadata.match_basis =
            classify_match_basis(deny_name_match, !deny_class_matches.is_empty());
        metadata.matched_rule = Some(matched_rule_name(
            "tools.deny",
            "tools.deny_classes",
            &metadata,
        ));

        let deny_reason = if deny_name_match && !deny_class_matches.is_empty() {
            "Tool is explicitly denylisted by name and class"
        } else if deny_name_match {
            "Tool is explicitly denylisted by name"
        } else {
            "Tool is explicitly denylisted by class"
        };

        return finalize_evaluation(
            policy,
            metadata,
            PolicyDecision::Deny {
                tool: tool_name.to_string(),
                code: "E_TOOL_DENIED".to_string(),
                reason: deny_reason.to_string(),
                contract: format_deny_contract(tool_name, "E_TOOL_DENIED", deny_reason),
            },
        );
    }

    let allow_name_match = is_allowed(policy, tool_name);
    let allow_class_matches = matched_allow_classes(policy, &tool_classes);
    if has_allowlist(policy) && !allow_name_match && allow_class_matches.is_empty() {
        return finalize_evaluation(
            policy,
            metadata,
            PolicyDecision::Deny {
                tool: tool_name.to_string(),
                code: "E_TOOL_NOT_ALLOWED".to_string(),
                reason: "Tool is not in the allowlist".to_string(),
                contract: format_deny_contract(
                    tool_name,
                    "E_TOOL_NOT_ALLOWED",
                    "Tool is not in allowlist",
                ),
            },
        );
    }

    if allow_name_match || !allow_class_matches.is_empty() {
        metadata.matched_tool_classes = allow_class_matches;
        metadata.match_basis =
            classify_match_basis(allow_name_match, !metadata.matched_tool_classes.is_empty());
        metadata.matched_rule = Some(matched_rule_name(
            "tools.allow",
            "tools.allow_classes",
            &metadata,
        ));
    }

    // 4. Schema Validation
    let compiled = policy.compiled_schemas();
    if let Some(validator) = compiled.get(tool_name) {
        if !validator.is_valid(args) {
            let violations: Vec<Value> = validator
                .iter_errors(args)
                .map(|e| {
                    json!({
                        "path": e.instance_path().to_string(),
                        "message": e.to_string(),
                    })
                })
                .collect();
            return finalize_evaluation(
                policy,
                metadata,
                PolicyDecision::Deny {
                    tool: tool_name.to_string(),
                    code: "E_ARG_SCHEMA".to_string(),
                    reason: "JSON Schema validation failed".to_string(),
                    contract: json!({
                        "status": "deny",
                        "error_code": "E_ARG_SCHEMA",
                        "tool": tool_name,
                        "violations": violations,
                    }),
                },
            );
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

    // 5. Unconstrained Mode
    let decision = match policy.enforcement.unconstrained_tools {
        UnconstrainedMode::Deny => PolicyDecision::Deny {
            tool: tool_name.to_string(),
            code: "E_TOOL_UNCONSTRAINED".to_string(),
            reason: "Tool has no schema (enforcement: deny)".to_string(),
            contract: format_deny_contract(
                tool_name,
                "E_TOOL_UNCONSTRAINED",
                "Tool has no schema (enforcement: deny)",
            ),
        },
        UnconstrainedMode::Warn => PolicyDecision::AllowWithWarning {
            tool: tool_name.to_string(),
            code: "E_TOOL_UNCONSTRAINED".to_string(),
            reason: "Tool allowed but has no schema".to_string(),
        },
        UnconstrainedMode::Allow => PolicyDecision::Allow,
    };

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

fn check_rate_limits(policy: &McpPolicy, state: &mut PolicyState) -> Option<PolicyDecision> {
    state.requests_count += 1;
    state.tool_calls_count += 1; // Simplified: Assumes evaluate called on tool call

    if let Some(limits) = &policy.limits {
        if let Some(max) = limits.max_requests_total {
            // Note: requests_count tracks total JSON-RPC, which we might not have here accurately
            // unless state is persistent session state.
            // For now, allow it to increment, assuming state is managing session.
            if state.requests_count > max {
                return Some(PolicyDecision::Deny {
                    tool: "ALL".to_string(),
                    code: "E_RATE_LIMIT".to_string(),
                    reason: "Rate limit exceeded (total requests)".to_string(),
                    contract: json!({ "status": "deny", "error_code": "E_RATE_LIMIT" }),
                });
            }
        }

        if let Some(max) = limits.max_tool_calls_total {
            if state.tool_calls_count > max {
                return Some(PolicyDecision::Deny {
                    tool: "ALL".to_string(),
                    code: "E_RATE_LIMIT".to_string(),
                    reason: "Rate limit exceeded (tool calls)".to_string(),
                    contract: json!({ "status": "deny", "error_code": "E_RATE_LIMIT" }),
                });
            }
        }
    }
    None
}

fn finalize_evaluation(
    policy: &McpPolicy,
    mut metadata: PolicyMatchMetadata,
    decision: PolicyDecision,
) -> PolicyEvaluation {
    metadata.policy_version = Some(if policy.version.trim().is_empty() {
        "unspecified".to_string()
    } else {
        policy.version.clone()
    });
    metadata.policy_digest = policy.policy_digest();
    let typed_contract = decision.typed_contract();
    metadata.typed_decision = Some(typed_contract.decision);
    let mut obligations = typed_contract.obligations;
    for obligation in metadata.obligations {
        if !obligations.iter().any(|existing| existing == &obligation) {
            obligations.push(obligation);
        }
    }
    metadata.obligations = obligations;

    PolicyEvaluation { decision, metadata }
}

fn apply_approval_required_obligation(
    policy: &McpPolicy,
    tool_name: &str,
    tool_classes: &BTreeSet<String>,
    decision: &PolicyDecision,
    metadata: &mut PolicyMatchMetadata,
) {
    if !matches!(
        decision,
        PolicyDecision::Allow | PolicyDecision::AllowWithWarning { .. }
    ) {
        return;
    }

    let name_required = policy
        .tools
        .approval_required
        .as_ref()
        .is_some_and(|patterns| {
            patterns
                .iter()
                .any(|pattern| matches_tool_pattern(tool_name, pattern))
        });
    let class_required = !match_classes(
        tool_classes,
        policy.tools.approval_required_classes.as_ref(),
    )
    .is_empty();

    if name_required || class_required {
        metadata.obligations.push(PolicyObligation {
            obligation_type: "approval_required".to_string(),
            detail: Some("runtime approval artifact required".to_string()),
            restrict_scope: None,
            redact_args: None,
        });
        metadata.approval_state = Some("required".to_string());
    }
}

fn apply_restrict_scope_obligation(
    policy: &McpPolicy,
    tool_name: &str,
    args: &Value,
    tool_classes: &BTreeSet<String>,
    decision: &PolicyDecision,
    metadata: &mut PolicyMatchMetadata,
) {
    if !matches!(
        decision,
        PolicyDecision::Allow | PolicyDecision::AllowWithWarning { .. }
    ) {
        return;
    }

    let name_scoped = policy
        .tools
        .restrict_scope
        .as_ref()
        .is_some_and(|patterns| {
            patterns
                .iter()
                .any(|pattern| matches_tool_pattern(tool_name, pattern))
        });
    let class_scoped =
        !match_classes(tool_classes, policy.tools.restrict_scope_classes.as_ref()).is_empty();

    if !name_scoped && !class_scoped {
        return;
    }

    let contract = policy
        .tools
        .restrict_scope_contract
        .clone()
        .unwrap_or_else(default_restrict_scope_contract);

    let evaluation = evaluate_restrict_scope_contract(&contract, tool_name, tool_classes, args);
    metadata.obligations.push(PolicyObligation::restrict_scope(
        contract.clone(),
        Some("restrict_scope contract captured; runtime enforcement deferred".to_string()),
    ));

    metadata.scope_type = Some(contract.scope_type.clone());
    metadata.scope_value = Some(contract.scope_value.clone());
    metadata.scope_match_mode = Some(contract.scope_match_mode.clone());
    metadata.scope_evaluation_state = Some(evaluation.state.clone());
    metadata.scope_failure_reason = evaluation.reason.clone();
    metadata.restrict_scope_present = Some(true);
    metadata.restrict_scope_target = Some(evaluation.target);
    metadata.restrict_scope_match = Some(evaluation.is_match);
    metadata.restrict_scope_reason = evaluation.reason;
}

fn apply_redact_args_obligation(
    policy: &McpPolicy,
    tool_name: &str,
    args: &Value,
    tool_classes: &BTreeSet<String>,
    decision: &PolicyDecision,
    metadata: &mut PolicyMatchMetadata,
) {
    if !matches!(
        decision,
        PolicyDecision::Allow | PolicyDecision::AllowWithWarning { .. }
    ) {
        return;
    }

    let name_redact = policy.tools.redact_args.as_ref().is_some_and(|patterns| {
        patterns
            .iter()
            .any(|pattern| matches_tool_pattern(tool_name, pattern))
    });
    let class_redact =
        !match_classes(tool_classes, policy.tools.redact_args_classes.as_ref()).is_empty();

    if !name_redact && !class_redact {
        return;
    }

    let contract = policy
        .tools
        .redact_args_contract
        .clone()
        .unwrap_or_else(default_redact_args_contract);

    let evaluation = evaluate_redact_args_contract(&contract, args);
    metadata.obligations.push(PolicyObligation::redact_args(
        contract.clone(),
        Some("redact_args contract captured; runtime redaction deferred".to_string()),
    ));

    metadata.redaction_target = Some(contract.redaction_target.clone());
    metadata.redaction_mode = Some(contract.redaction_mode.clone());
    metadata.redaction_scope = Some(contract.redaction_scope.clone());
    metadata.redaction_applied_state = Some(evaluation.state.clone());
    metadata.redaction_reason = evaluation.reason.clone();
    metadata.redaction_failure_reason = evaluation.failure_reason.clone();
    metadata.redact_args_present = Some(true);
    metadata.redact_args_target = Some(contract.redaction_target.clone());
    metadata.redact_args_mode = Some(contract.redaction_mode.clone());
    metadata.redact_args_result = Some(evaluation.state);
    metadata.redact_args_reason = evaluation.reason;
}

#[derive(Debug)]
struct RestrictScopeEvaluation {
    target: String,
    is_match: bool,
    state: String,
    reason: Option<String>,
}

#[derive(Debug)]
struct RedactArgsEvaluation {
    state: String,
    reason: Option<String>,
    failure_reason: Option<String>,
}

fn default_restrict_scope_contract() -> RestrictScopeContract {
    RestrictScopeContract {
        scope_type: "resource".to_string(),
        scope_value: "*".to_string(),
        scope_match_mode: "exact".to_string(),
    }
}

fn default_redact_args_contract() -> RedactArgsContract {
    RedactArgsContract {
        redaction_target: "body".to_string(),
        redaction_mode: "mask".to_string(),
        redaction_scope: "request".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DelegationEvidenceContext {
    delegated_from: String,
    delegation_depth: Option<u32>,
}

fn apply_delegation_context(args: &Value, metadata: &mut PolicyMatchMetadata) {
    let Some(context) = parse_delegation_context(args) else {
        return;
    };

    metadata.delegated_from = Some(context.delegated_from);
    metadata.delegation_depth = context.delegation_depth;
}

fn parse_delegation_context(args: &Value) -> Option<DelegationEvidenceContext> {
    let delegation = args.get("_meta")?.get("delegation")?.as_object()?;

    let delegated_from = delegation
        .get("delegated_from")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();

    let delegation_depth = delegation
        .get("delegation_depth")
        .and_then(Value::as_u64)
        .filter(|depth| *depth >= 1 && *depth <= u32::MAX as u64)
        .map(|depth| depth as u32);

    Some(DelegationEvidenceContext {
        delegated_from,
        delegation_depth,
    })
}

fn evaluate_restrict_scope_contract(
    contract: &RestrictScopeContract,
    tool_name: &str,
    tool_classes: &BTreeSet<String>,
    args: &Value,
) -> RestrictScopeEvaluation {
    let scope_type = contract.scope_type.trim().to_ascii_lowercase();
    let match_mode = contract.scope_match_mode.trim().to_ascii_lowercase();
    let expected = contract.scope_value.as_str();

    let observed = match scope_type.as_str() {
        "tool" => Some(tool_name.to_string()),
        "resource" => args
            .get("_meta")
            .and_then(|meta| meta.get("resource"))
            .and_then(Value::as_str)
            .or_else(|| args.get("resource").and_then(Value::as_str))
            .map(ToString::to_string),
        "tool_class" => tool_classes.iter().next().cloned(),
        _ => None,
    };

    let target = match observed.as_deref() {
        Some(value) => format!("{scope_type}:{value}"),
        None => format!("{scope_type}:<missing>"),
    };

    let result = match (scope_type.as_str(), observed.as_deref()) {
        ("tool" | "resource" | "tool_class", None) => (
            false,
            "mismatch".to_string(),
            Some("scope_target_missing".to_string()),
        ),
        ("tool" | "resource" | "tool_class", Some(actual)) => match match_mode.as_str() {
            "exact" => (
                actual == expected,
                if actual == expected {
                    "matched"
                } else {
                    "mismatch"
                }
                .to_string(),
                if actual == expected {
                    None
                } else {
                    Some("scope_target_mismatch".to_string())
                },
            ),
            "prefix" => (
                actual.starts_with(expected),
                if actual.starts_with(expected) {
                    "matched"
                } else {
                    "mismatch"
                }
                .to_string(),
                if actual.starts_with(expected) {
                    None
                } else {
                    Some("scope_target_mismatch".to_string())
                },
            ),
            "contains" => (
                actual.contains(expected),
                if actual.contains(expected) {
                    "matched"
                } else {
                    "mismatch"
                }
                .to_string(),
                if actual.contains(expected) {
                    None
                } else {
                    Some("scope_target_mismatch".to_string())
                },
            ),
            "any" => (true, "matched".to_string(), None),
            _ => (
                false,
                "not_evaluated".to_string(),
                Some("scope_match_mode_unsupported".to_string()),
            ),
        },
        _ => (
            false,
            "not_evaluated".to_string(),
            Some("scope_type_unsupported".to_string()),
        ),
    };

    RestrictScopeEvaluation {
        target,
        is_match: result.0,
        state: result.1,
        reason: result.2,
    }
}

fn evaluate_redact_args_contract(
    contract: &RedactArgsContract,
    args: &Value,
) -> RedactArgsEvaluation {
    let target = contract.redaction_target.trim().to_ascii_lowercase();
    let mode = contract.redaction_mode.trim().to_ascii_lowercase();
    let scope = contract.redaction_scope.trim().to_ascii_lowercase();

    let mode_supported = matches!(mode.as_str(), "mask" | "hash" | "drop" | "partial");
    let scope_supported = matches!(scope.as_str(), "request" | "args" | "metadata");

    if !mode_supported {
        return RedactArgsEvaluation {
            state: "not_evaluated".to_string(),
            reason: Some("redaction_mode_unsupported".to_string()),
            failure_reason: Some("redaction_mode_unsupported".to_string()),
        };
    }

    if !scope_supported {
        return RedactArgsEvaluation {
            state: "not_evaluated".to_string(),
            reason: Some("redaction_scope_unsupported".to_string()),
            failure_reason: Some("redaction_scope_unsupported".to_string()),
        };
    }

    let Some(target_value) = redaction_target_value(args, &target) else {
        return RedactArgsEvaluation {
            state: "not_applied".to_string(),
            reason: Some("redaction_target_missing".to_string()),
            failure_reason: Some("redaction_target_missing".to_string()),
        };
    };

    if can_apply_redaction(&mode, target_value) {
        RedactArgsEvaluation {
            state: "applied".to_string(),
            reason: None,
            failure_reason: None,
        }
    } else {
        RedactArgsEvaluation {
            state: "not_applied".to_string(),
            reason: Some("redaction_apply_failed".to_string()),
            failure_reason: Some("redaction_apply_failed".to_string()),
        }
    }
}

fn redaction_target_value<'a>(args: &'a Value, target: &str) -> Option<&'a Value> {
    match target {
        "path" => args.get("path"),
        "query" => args.get("query"),
        "headers" => args.get("headers"),
        "body" => args.get("body"),
        "metadata" => args.get("_meta"),
        "args" => Some(args),
        _ => None,
    }
}

fn can_apply_redaction(mode: &str, value: &Value) -> bool {
    match mode {
        "mask" | "hash" | "drop" => !value.is_null(),
        "partial" => value.is_string(),
        _ => false,
    }
}

fn is_denied(policy: &McpPolicy, tool_name: &str) -> bool {
    let root_deny = policy.deny.as_ref();
    let tools_deny = policy.tools.deny.as_ref();
    root_deny
        .iter()
        .flat_map(|v| v.iter())
        .chain(tools_deny.iter().flat_map(|v| v.iter()))
        .any(|pattern| matches_tool_pattern(tool_name, pattern))
}

fn has_allowlist(policy: &McpPolicy) -> bool {
    policy.allow.is_some() || policy.tools.allow.is_some() || policy.tools.allow_classes.is_some()
}

fn is_allowed(policy: &McpPolicy, tool_name: &str) -> bool {
    let root_allow = policy.allow.as_ref();
    let tools_allow = policy.tools.allow.as_ref();
    root_allow
        .iter()
        .flat_map(|v| v.iter())
        .chain(tools_allow.iter().flat_map(|v| v.iter()))
        .any(|pattern| matches_tool_pattern(tool_name, pattern))
}

fn format_deny_contract(tool: &str, code: &str, reason: &str) -> Value {
    json!({
        "status": "deny",
        "error_code": code,
        "tool": tool,
        "reason": reason
    })
}

fn matched_deny_classes(policy: &McpPolicy, tool_classes: &BTreeSet<String>) -> Vec<String> {
    match_classes(tool_classes, policy.tools.deny_classes.as_ref())
}

fn matched_allow_classes(policy: &McpPolicy, tool_classes: &BTreeSet<String>) -> Vec<String> {
    match_classes(tool_classes, policy.tools.allow_classes.as_ref())
}

fn match_classes(tool_classes: &BTreeSet<String>, configured: Option<&Vec<String>>) -> Vec<String> {
    let mut matched = BTreeSet::new();
    if let Some(configured_classes) = configured {
        for class_name in configured_classes {
            if tool_classes.contains(class_name) {
                matched.insert(class_name.clone());
            }
        }
    }
    matched.into_iter().collect()
}

fn classify_match_basis(name_match: bool, class_match: bool) -> MatchBasis {
    match (name_match, class_match) {
        (true, true) => MatchBasis::NameAndClass,
        (true, false) => MatchBasis::Name,
        (false, true) => MatchBasis::Class,
        (false, false) => MatchBasis::None,
    }
}

fn matched_rule_name(
    name_field: &str,
    class_field: &str,
    metadata: &PolicyMatchMetadata,
) -> String {
    match metadata.match_basis {
        MatchBasis::NameAndClass => format!("{name_field}+{class_field}"),
        MatchBasis::Name => name_field.to_string(),
        MatchBasis::Class => class_field.to_string(),
        MatchBasis::None => name_field.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_delegation_context;
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
