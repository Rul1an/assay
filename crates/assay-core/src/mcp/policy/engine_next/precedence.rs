use super::super::{McpPolicy, PolicyDecision, PolicyMatchMetadata};
use super::diagnostics::format_deny_contract;
use super::matcher::{
    classify_match_basis, has_allowlist, is_allowed, is_denied, matched_allow_classes,
    matched_deny_classes, matched_rule_name,
};
use std::collections::BTreeSet;

pub(in crate::mcp::policy) fn deny_match_decision(
    policy: &McpPolicy,
    tool_name: &str,
    tool_classes: &BTreeSet<String>,
    metadata: &mut PolicyMatchMetadata,
) -> Option<PolicyDecision> {
    let deny_name_match = is_denied(policy, tool_name);
    let deny_class_matches = matched_deny_classes(policy, tool_classes);
    if !deny_name_match && deny_class_matches.is_empty() {
        return None;
    }

    metadata.matched_tool_classes = deny_class_matches.clone();
    metadata.match_basis = classify_match_basis(deny_name_match, !deny_class_matches.is_empty());
    metadata.matched_rule = Some(matched_rule_name(
        "tools.deny",
        "tools.deny_classes",
        metadata,
    ));

    let deny_reason = if deny_name_match && !deny_class_matches.is_empty() {
        "Tool is explicitly denylisted by name and class"
    } else if deny_name_match {
        "Tool is explicitly denylisted by name"
    } else {
        "Tool is explicitly denylisted by class"
    };

    Some(PolicyDecision::Deny {
        tool: tool_name.to_string(),
        code: "E_TOOL_DENIED".to_string(),
        reason: deny_reason.to_string(),
        contract: format_deny_contract(tool_name, "E_TOOL_DENIED", deny_reason),
    })
}

pub(in crate::mcp::policy) fn apply_allow_precedence(
    policy: &McpPolicy,
    tool_name: &str,
    tool_classes: &BTreeSet<String>,
    metadata: &mut PolicyMatchMetadata,
) -> Option<PolicyDecision> {
    let allow_name_match = is_allowed(policy, tool_name);
    let allow_class_matches = matched_allow_classes(policy, tool_classes);

    if has_allowlist(policy) && !allow_name_match && allow_class_matches.is_empty() {
        return Some(PolicyDecision::Deny {
            tool: tool_name.to_string(),
            code: "E_TOOL_NOT_ALLOWED".to_string(),
            reason: "Tool is not in the allowlist".to_string(),
            contract: format_deny_contract(
                tool_name,
                "E_TOOL_NOT_ALLOWED",
                "Tool is not in allowlist",
            ),
        });
    }

    if allow_name_match || !allow_class_matches.is_empty() {
        metadata.matched_tool_classes = allow_class_matches;
        metadata.match_basis =
            classify_match_basis(allow_name_match, !metadata.matched_tool_classes.is_empty());
        metadata.matched_rule = Some(matched_rule_name(
            "tools.allow",
            "tools.allow_classes",
            metadata,
        ));
    }

    None
}
