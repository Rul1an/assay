use super::super::{McpPolicy, PolicyDecision, PolicyEvaluation, PolicyMatchMetadata};
use serde_json::{json, Value};

pub(in crate::mcp::policy) fn finalize_evaluation(
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

pub(in crate::mcp::policy) fn format_deny_contract(tool: &str, code: &str, reason: &str) -> Value {
    json!({
        "status": "deny",
        "error_code": code,
        "tool": tool,
        "reason": reason
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mcp::policy) struct DelegationEvidenceContext {
    pub(in crate::mcp::policy) delegated_from: String,
    pub(in crate::mcp::policy) delegation_depth: Option<u32>,
}

pub(in crate::mcp::policy) fn apply_delegation_context(
    args: &Value,
    metadata: &mut PolicyMatchMetadata,
) {
    let Some(context) = parse_delegation_context(args) else {
        return;
    };

    metadata.delegated_from = Some(context.delegated_from);
    metadata.delegation_depth = context.delegation_depth;
}

pub(in crate::mcp::policy) fn parse_delegation_context(
    args: &Value,
) -> Option<DelegationEvidenceContext> {
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
