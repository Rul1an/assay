use super::allowance::{allowance_matches, target_digest};
use super::credential_scope::credential_scope_gate;
use super::manifest::{DeclaredManifest, ObservedToolDigest};
use super::policy::EnforcePolicy;
use assay_mcp_server::tool_decision::classify;
use serde_json::Value;

/// A PDP decision. `allow == true` means forward the call to the upstream; otherwise `reason` is the
/// precedence-pinned deny reason. When `allow` is true `reason` is the constant `"allow"`.
pub struct Decision {
    pub allow: bool,
    pub reason: &'static str,
    pub action_class: Option<String>,
    pub target_digest: Option<String>,
}

impl Decision {
    fn deny(
        reason: &'static str,
        action_class: Option<String>,
        target_digest: Option<String>,
    ) -> Decision {
        Decision {
            allow: false,
            reason,
            action_class,
            target_digest,
        }
    }
}

/// The PDP. Precedence (first failing gate wins); only a call that clears EVERY gate is allowed:
/// 1. classification (before allowance, so a missing-target call reads as classification_incomplete,
///    never as a target mismatch);
/// 2. caller-allowance match (github_deploy_key {owner, repo} only so far);
/// 3. credential-scope (c2): the declared upstream credential must cover the action's required scope,
///    else credential_scope_insufficient / credential_scope_unknown;
/// 4. drift (c3): the current observed per-tool digest must equal the approved baseline digest, with
///    BOTH a baseline (else manifest_baseline_missing) and a current complete observation (else
///    manifest_current_observation_incomplete) required; a digest change is manifest_drifted_since_approval;
/// 5. all gates passed -> ALLOW (forward). There is no `pdp_gate_unavailable` — c3 removed it.
pub fn decide(
    policy: &EnforcePolicy,
    baseline: &DeclaredManifest,
    observed: &ObservedToolDigest,
    tool_name: &str,
    args: &Value,
) -> Decision {
    let c = classify(tool_name, args);

    // 1. classification gate — fail-closed before any allowance matching.
    if c.category.is_none() {
        return Decision::deny("unclassified_tool_call", None, None);
    }
    if c.state != "classified" {
        // classified_incomplete / redaction_failed / any non-final state -> not enough to authorize.
        return Decision::deny(
            "classification_incomplete",
            c.category.map(|s| s.to_string()),
            Some(target_digest(&c.target)),
        );
    }

    let action_class = c.category.unwrap();
    let tdig = target_digest(&c.target);

    // 2. caller-allowance gate.
    let matched = policy
        .allowances
        .iter()
        .any(|a| a.action_class == action_class && allowance_matches(a, action_class, &c.target));
    if !matched {
        return Decision::deny(
            "no_declared_allowance",
            Some(action_class.to_string()),
            Some(tdig),
        );
    }

    // 3. credential-scope gate (c2): the declared upstream credential must cover the action's required
    // scope. Fail-closed — an absent credential, an unrecognized scope, or a too-coarse scope is a
    // credential_scope_unknown (coverage cannot be determined), never a silent pass.
    if let Some(reason) = credential_scope_gate(policy, action_class) {
        return Decision::deny(reason, Some(action_class.to_string()), Some(tdig));
    }

    // 4. drift gate (c3): the tool the caller is invoking must be the one approved. Both inputs are
    // required and fail-closed: the approved baseline (never the first observed session manifest) AND a
    // current COMPLETE observation of this tool's surface. "no drift observed" is never "no drift".
    let baseline_digest = match baseline.tool_digest_for(tool_name) {
        Some(d) => d,
        None => {
            return Decision::deny(
                "manifest_baseline_missing",
                Some(action_class.to_string()),
                Some(tdig),
            )
        }
    };
    let observed_digest = match observed {
        ObservedToolDigest::Present(d) => d.as_str(),
        // A duplicate-name (ambiguous) observed manifest is inconclusive — deny with a distinct reason
        // rather than compare against one of the colliding digests.
        ObservedToolDigest::Ambiguous => {
            return Decision::deny(
                "manifest_observation_ambiguous",
                Some(action_class.to_string()),
                Some(tdig),
            )
        }
        // No complete observation (or one invalidated by a later tools/list_changed), or the tool is
        // absent from the complete manifest: either way there is no current digest to compare, so
        // drift cannot be ruled out -> fail closed.
        ObservedToolDigest::NoCompleteManifest | ObservedToolDigest::CompleteButToolAbsent => {
            return Decision::deny(
                "manifest_current_observation_incomplete",
                Some(action_class.to_string()),
                Some(tdig),
            )
        }
    };
    if baseline_digest != observed_digest {
        return Decision::deny(
            "manifest_drifted_since_approval",
            Some(action_class.to_string()),
            Some(tdig),
        );
    }

    // 5. every gate passed -> ALLOW (forward). This is the only allow path, and it is deliberately the
    // narrowest one: exact caller allowance + covered credential scope + an approved baseline whose
    // per-tool digest exactly equals the current complete observed digest.
    Decision {
        allow: true,
        reason: "allow",
        action_class: Some(action_class.to_string()),
        target_digest: Some(tdig),
    }
}

/// The drift-gate state for the evidence record, derived from the decision (so `decide` stays
/// unchanged). `satisfied` on allow; the specific drift reason when the drift gate denied;
/// `not_evaluated` when an earlier gate denied before the drift gate ran.
pub(crate) fn drift_state(decision: &Decision) -> &'static str {
    if decision.allow {
        return "satisfied";
    }
    match decision.reason {
        "manifest_baseline_missing" => "baseline_missing",
        "manifest_current_observation_incomplete" => "current_observation_incomplete",
        "manifest_observation_ambiguous" => "observation_ambiguous",
        "manifest_drifted_since_approval" => "drifted",
        _ => "not_evaluated",
    }
}
