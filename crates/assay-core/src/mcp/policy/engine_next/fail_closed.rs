use super::super::super::identity::ToolIdentity;
use super::super::{McpPolicy, PolicyDecision, PolicyState, ToolObservation, UnconstrainedMode};
use super::diagnostics::format_deny_contract;
use serde_json::{json, Value};

pub(in crate::mcp::policy) fn tool_drift_decision(
    policy: &McpPolicy,
    tool_name: &str,
    runtime_identity: Option<&ToolIdentity>,
) -> Option<PolicyDecision> {
    let pinned = policy.tool_pins.get(tool_name)?;
    let runtime = runtime_identity?;

    if pinned == runtime {
        return None;
    }

    Some(PolicyDecision::Deny {
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
            "Tool metadata or schema has changed without policy update",
        ),
    })
}

/// At a session boundary a pin compares against what this client holds, so a pinned tool with no
/// single current definition cannot be shown unchanged.
pub(in crate::mcp::policy) fn tool_unobserved_decision(
    policy: &McpPolicy,
    tool_name: &str,
    observation: &ToolObservation,
) -> Option<PolicyDecision> {
    policy.tool_pins.get(tool_name)?;
    let contract_reason = match observation {
        ToolObservation::Definition(_) => return None,
        ToolObservation::Absent => {
            "No current definition of this pinned tool was received by this client"
        }
        ToolObservation::Conflicting => {
            "This client received conflicting definitions of this pinned tool"
        }
    };
    Some(PolicyDecision::Deny {
        tool: tool_name.to_string(),
        code: "E_TOOL_UNOBSERVED".to_string(),
        reason: format!("Tool integrity failure: {contract_reason}; list the tools and retry."),
        contract: format_deny_contract(tool_name, "E_TOOL_UNOBSERVED", contract_reason),
    })
}

pub(in crate::mcp::policy) fn check_rate_limits(
    policy: &McpPolicy,
    state: &mut PolicyState,
) -> Option<PolicyDecision> {
    state.requests_count += 1;
    state.tool_calls_count += 1;

    if let Some(limits) = &policy.limits {
        if let Some(max) = limits.max_requests_total {
            if state.requests_count > max {
                return Some(PolicyDecision::Deny {
                    tool: "ALL".into(),
                    code: "E_RATE_LIMIT".to_string(),
                    reason: "Rate limit exceeded (total requests)".to_string(),
                    contract: json!({ "status": "deny", "error_code": "E_RATE_LIMIT" }),
                });
            }
        }

        if let Some(max) = limits.max_tool_calls_total {
            if state.tool_calls_count > max {
                return Some(PolicyDecision::Deny {
                    tool: "ALL".into(),
                    code: "E_RATE_LIMIT".to_string(),
                    reason: "Rate limit exceeded (tool calls)".to_string(),
                    contract: json!({ "status": "deny", "error_code": "E_RATE_LIMIT" }),
                });
            }
        }
    }
    None
}

pub(in crate::mcp::policy) fn schema_violation_decision(
    tool_name: &str,
    args: &Value,
    validator: &jsonschema::Validator,
) -> Option<PolicyDecision> {
    if validator.is_valid(args) {
        return None;
    }

    let violations: Vec<Value> = validator
        .iter_errors(args)
        .map(|e| {
            json!({
                "path": e.instance_path().to_string(),
                "message": e.to_string(),
            })
        })
        .collect();

    Some(PolicyDecision::Deny {
        tool: tool_name.to_string(),
        code: "E_ARG_SCHEMA".to_string(),
        reason: "JSON Schema validation failed".to_string(),
        contract: json!({
            "status": "deny",
            "error_code": "E_ARG_SCHEMA",
            "tool": tool_name,
            "violations": violations,
        }),
    })
}

pub(in crate::mcp::policy) fn unconstrained_decision(
    policy: &McpPolicy,
    tool_name: &str,
) -> PolicyDecision {
    match policy.enforcement.unconstrained_tools {
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
    }
}
