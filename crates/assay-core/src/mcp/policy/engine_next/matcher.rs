use super::super::super::tool_match::MatchBasis;
use super::super::{matches_tool_pattern, McpPolicy, PolicyMatchMetadata};
use std::collections::BTreeSet;

pub(in crate::mcp::policy) fn is_denied(policy: &McpPolicy, tool_name: &str) -> bool {
    let root_deny = policy.deny.as_ref();
    let tools_deny = policy.tools.deny.as_ref();
    root_deny
        .iter()
        .flat_map(|v| v.iter())
        .chain(tools_deny.iter().flat_map(|v| v.iter()))
        .any(|pattern| matches_tool_pattern(tool_name, pattern))
}

pub(in crate::mcp::policy) fn has_allowlist(policy: &McpPolicy) -> bool {
    policy.allow.is_some() || policy.tools.allow.is_some() || policy.tools.allow_classes.is_some()
}

pub(in crate::mcp::policy) fn is_allowed(policy: &McpPolicy, tool_name: &str) -> bool {
    let root_allow = policy.allow.as_ref();
    let tools_allow = policy.tools.allow.as_ref();
    root_allow
        .iter()
        .flat_map(|v| v.iter())
        .chain(tools_allow.iter().flat_map(|v| v.iter()))
        .any(|pattern| matches_tool_pattern(tool_name, pattern))
}

pub(in crate::mcp::policy) fn matched_deny_classes(
    policy: &McpPolicy,
    tool_classes: &BTreeSet<String>,
) -> Vec<String> {
    match_classes(tool_classes, policy.tools.deny_classes.as_ref())
}

pub(in crate::mcp::policy) fn matched_allow_classes(
    policy: &McpPolicy,
    tool_classes: &BTreeSet<String>,
) -> Vec<String> {
    match_classes(tool_classes, policy.tools.allow_classes.as_ref())
}

pub(in crate::mcp::policy) fn match_classes(
    tool_classes: &BTreeSet<String>,
    configured: Option<&Vec<String>>,
) -> Vec<String> {
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

pub(in crate::mcp::policy) fn classify_match_basis(
    name_match: bool,
    class_match: bool,
) -> MatchBasis {
    match (name_match, class_match) {
        (true, true) => MatchBasis::NameAndClass,
        (true, false) => MatchBasis::Name,
        (false, true) => MatchBasis::Class,
        (false, false) => MatchBasis::None,
    }
}

pub(in crate::mcp::policy) fn matched_rule_name(
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
