use super::decision::{drift_state, Decision};
use super::policy::EnforcePolicy;
use assay_mcp_server::tool_decision::{classify, sanitize};
use serde_json::{json, Value};

/// Build the per-call `assay.enforcement_decision.v0` evidence record (P61e-d). Emitted by the
/// enforcing path for BOTH allow and deny; deterministic (no timestamp). It records the policy
/// decision only — it never asserts or verifies the upstream side effect (which stays `asserted` on
/// the E9 ladder). The credential is referenced by alias, never by value, and the projected target
/// carries only the classifier's allowlisted fields (sensitive ids already hashed). This is NOT the
/// observation artifact (`assay.mcp_manifest_observed.v0`) nor the mechanism artifact
/// (`assay.enforcement_health.v0`); the three carriers stay separate.
pub fn decision_record(
    policy: &EnforcePolicy,
    decision: &Decision,
    tool_name: &str,
    args: &Value,
) -> Value {
    let c = classify(tool_name, args);
    json!({
        "schema": "assay.enforcement_decision.v0",
        "caller": { "id": sanitize(&policy.caller.id) },
        "tool": {
            "name": sanitize(tool_name),
            "action_class": decision.action_class,
        },
        "action": {
            "verb": c.verb,
            "resource_type": c.resource_type,
            // Projected target: allowlisted fields only, sensitive ids hashed by the classifier.
            "target": c.target,
            "target_digest": decision.target_digest,
        },
        // The proxy's POLICY decision, true at write time. It is written before the forward (so an
        // allowed call is never forwarded unrecorded), and therefore deliberately does NOT carry a
        // transport-outcome field: it must not claim the call reached the upstream. A forward that
        // then fails surfaces to the caller as `proxy_failed` (§12), never as a delivery claim here.
        "decision": if decision.allow { "allow" } else { "deny" },
        "reason": decision.reason,
        // The PDP is fail-closed by construction: every deny is a fail-closed outcome.
        "fail_closed": !decision.allow,
        "drift_state": drift_state(decision),
        // Operator config reference only — never the token, never the declared scopes.
        "credential_alias": policy
            .upstream_credential
            .as_ref()
            .map(|c| sanitize(&c.alias)),
        "non_claims": [
            "policy decision only; does not assert or verify the upstream side effect (stays asserted, E9 ladder)",
            "an allow is the decision to forward; it does not assert the call reached or was performed by the upstream (a transport failure surfaces as proxy_failed, not here)",
            "credential referenced by alias only, never the token or declared scopes",
            "deny is fail-closed caution and allow is a policy decision — neither is a maliciousness verdict",
            "not the observation artifact (assay.mcp_manifest_observed.v0) and not the mechanism artifact (assay.enforcement_health.v0)"
        ]
    })
}
