use super::policy::EnforcePolicy;
use assay_mcp_server::tool_decision::required_scope_for;

/// Coverage of a declared credential's scopes against an action's required scope.
pub(crate) enum ScopeCoverage {
    /// The required scope is covered (exactly, by a broader non-admin scope, or by an admin/wildcard
    /// scope — overbroad still covers; overbroad is a recommendation, never a block in v0).
    Covered,
    /// A recognized declared scope set that does not cover the required scope.
    Insufficient,
    /// Coverage cannot be determined: no lattice for the class, an unrecognized scope, or a
    /// too-coarse (ambiguous) scope. An unknown is NOT an insufficiency (spec §8).
    Unknown,
}

/// c2 credential-scope gate. Returns the deny reason, or `None` when the declared credential covers the
/// required scope (the call falls through to the next gate). Deterministic; no provider query; the
/// declared scopes are operator config, never a provider-verified grant.
pub(crate) fn credential_scope_gate(
    policy: &EnforcePolicy,
    action_class: &str,
) -> Option<&'static str> {
    // Required scope is a deterministic function of the action category (P59) — Assay's static claim
    // of what the action needs. An unknown required scope is fail-closed (deny, not a silent pass) —
    // do NOT use `?` here, which would return None (= covered) and fail OPEN.
    let required = match required_scope_for(Some(action_class)) {
        Some(r) => r,
        None => return Some("credential_scope_unknown"),
    };
    let cred = match &policy.upstream_credential {
        Some(c) => c,
        // No declared credential: coverage cannot be determined.
        None => return Some("credential_scope_unknown"),
    };
    match scope_covers(action_class, required, &cred.scopes) {
        ScopeCoverage::Covered => None,
        ScopeCoverage::Insufficient => Some("credential_scope_insufficient"),
        ScopeCoverage::Unknown => Some("credential_scope_unknown"),
    }
}

/// The non-required scope vocabulary for one action class, kept identical to the AUTHORITATIVE P59
/// credential-scope contract (docs/reference/credential-scope.md), NOT the richer E10 measurement
/// vocabulary. The enforcement gate must never cover a scope the documented contract says it should
/// not — "broadening the lattice is a deliberate, fixture-backed change, not a guess." `required`
/// itself comes from `required_scope_for` (one source of truth); this lattice classifies the OTHER
/// recognized scopes. Any scope not listed here is unrecognized -> Unknown (never silently covered).
struct ScopeLattice {
    /// Covers the required scope without admin breadth.
    broader_ok: &'static [&'static str],
    /// Covers via admin breadth (overbroad — still covers; a recommendation, not a block).
    overbroad: &'static [&'static str],
    /// Recognized but does not cover.
    non_covering: &'static [&'static str],
    /// Recognized but too coarse to tell action-specific from admin (-> Unknown, never forced).
    ambiguous: &'static [&'static str],
}

/// Only `github_deploy_key` has a documented coverage contract today (credential-scope.md §"initial
/// GitHub lattice"). Any other classified privileged action has no documented lattice yet, so it is
/// fail-closed (`Unknown` -> `credential_scope_unknown`) until its own contract slice lands — never a
/// guessed coverage. (Such classes also cannot currently reach this gate: c1's allowance matcher only
/// admits `github_deploy_key`.)
fn lattice_for(action_class: &str) -> Option<ScopeLattice> {
    match action_class {
        // Matches credential-scope.md exactly: covered by {repo:deploy_key:write, repo:admin};
        // NOT covered by {repo:read, repo:metadata, repo:contents:read}; everything else unknown.
        // repo:write is deliberately NOT a covering scope (it is not in the documented contract).
        "github_deploy_key" => Some(ScopeLattice {
            broader_ok: &[],
            overbroad: &["repo:admin"],
            non_covering: &["repo:read", "repo:metadata", "repo:contents:read"],
            ambiguous: &[],
        }),
        _ => None,
    }
}

/// Deterministic scope coverage. A too-coarse (ambiguous) or unrecognized scope yields Unknown BEFORE
/// any insufficiency verdict ("unknown is not insufficient").
pub(crate) fn scope_covers(
    action_class: &str,
    required: &str,
    declared: &[String],
) -> ScopeCoverage {
    let lat = match lattice_for(action_class) {
        Some(l) => l,
        // No lattice for this class: coverage cannot be determined (fail-closed).
        None => return ScopeCoverage::Unknown,
    };
    let any_in = |set: &[&str]| declared.iter().any(|s| set.contains(&s.as_str()));
    // A too-coarse scope means coverage cannot be determined — takes precedence over everything else.
    if any_in(lat.ambiguous) {
        return ScopeCoverage::Unknown;
    }
    // Any scope the lattice does not recognize -> cannot determine coverage.
    let recognized = |s: &str| {
        s == required
            || lat.broader_ok.contains(&s)
            || lat.overbroad.contains(&s)
            || lat.non_covering.contains(&s)
    };
    if declared.iter().any(|s| !recognized(s.as_str())) {
        return ScopeCoverage::Unknown;
    }
    let covers =
        declared.iter().any(|s| s == required) || any_in(lat.broader_ok) || any_in(lat.overbroad);
    if !covers {
        return ScopeCoverage::Insufficient;
    }
    ScopeCoverage::Covered
}
