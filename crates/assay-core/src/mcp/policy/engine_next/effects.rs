use super::super::{
    McpPolicy, PolicyDecision, PolicyMatchMetadata, PolicyObligation, RedactArgsContract,
    RestrictScopeContract,
};
use super::matcher::match_classes;
use serde_json::Value;
use std::collections::BTreeSet;

pub(in crate::mcp::policy) fn apply_approval_required_obligation(
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
                .any(|pattern| super::super::matches_tool_pattern(tool_name, pattern))
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

pub(in crate::mcp::policy) fn apply_restrict_scope_obligation(
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
                .any(|pattern| super::super::matches_tool_pattern(tool_name, pattern))
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

pub(in crate::mcp::policy) fn apply_redact_args_obligation(
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
            .any(|pattern| super::super::matches_tool_pattern(tool_name, pattern))
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
