use super::super::identity::ToolIdentity;
use super::super::jsonrpc::JsonRpcRequest;
use super::super::tool_match::MatchBasis;
use super::{
    matches_tool_pattern, McpPolicy, PolicyDecision, PolicyEvaluation, PolicyMatchMetadata,
    PolicyState, UnconstrainedMode,
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
        return finalize_evaluation(policy, metadata, PolicyDecision::Allow);
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
    metadata.obligations = typed_contract.obligations;

    PolicyEvaluation { decision, metadata }
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
