//! P61e-c1/c2: the enforcing-proxy policy decision point — caller-allowance + credential-scope gates,
//! deny-only. Spec: docs/reference/mcp-upstream-proxy-enforcement.md.
//!
//! Scope so far: parse the `--enforce-policy` file, classify an observed `tools/call`, and decide a
//! DENY with a precedence-pinned reason. c1 added the caller-allowance gate; c2 adds the
//! credential-scope gate (the declared upstream credential must cover the action's required scope).
//! There is still no allow path and no forwarding (that arrives in c3, with the drift gate). A
//! `tools/call` that passes every enabled gate is still denied with `pdp_gate_unavailable`, a temporary
//! rollout reason removed when c3 lands.
//!
//! Caller identity is the static `caller.id` from the policy only — no transport/env/request
//! inference — so a single stdio session is bound to one configured caller and `unknown_caller`
//! cannot occur at runtime (a policy without `caller.id` fails startup).

use anyhow::{bail, Context, Result};
use assay_core::mcp::jcs;
use assay_mcp_server::cache::sha256_hex;
use assay_mcp_server::tool_decision::{classify, required_scope_for};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct EnforcePolicy {
    pub caller: Caller,
    /// The single upstream credential the proxy holds for this session. The credential-scope gate
    /// (c2) reads its `scopes`; `None` means no credential is declared, which is a fail-closed
    /// `credential_scope_unknown` (coverage cannot be determined), never a silent pass.
    #[serde(default)]
    pub upstream_credential: Option<UpstreamCredential>,
    #[serde(default)]
    pub allowances: Vec<Allowance>,
}

#[derive(Debug, Deserialize)]
pub struct Caller {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct UpstreamCredential {
    /// Referenced in evidence by alias, never by value (P61e-d enforcement_decision record);
    /// not read by the c2 gate, which only compares `scopes`.
    #[allow(dead_code)]
    pub alias: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Allowance {
    pub action_class: String,
    #[serde(default)]
    pub targets: Vec<Target>,
}

/// c1 supports the `github_deploy_key` target shape only.
#[derive(Debug, Deserialize)]
pub struct Target {
    pub owner: String,
    pub repo: String,
}

/// Load + validate the enforce policy. Any failure here is a STARTUP failure (the caller surfaces it
/// as a non-zero exit), never a runtime deny: an enforcing proxy without a valid policy is a
/// misconfigured service and must not start.
pub fn load(path: &Path) -> Result<EnforcePolicy> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading --enforce-policy {}", path.display()))?;
    let policy: EnforcePolicy =
        serde_yaml::from_str(&text).with_context(|| "parsing --enforce-policy YAML")?;
    if policy.caller.id.trim().is_empty() {
        bail!("--enforce-policy: caller.id must be a non-empty string");
    }
    Ok(policy)
}

/// Domain-separated, canonical digest of a projected target for the DIAGNOSTIC decision log only.
/// Stable for correlation, never a raw target, never an evidence artifact.
pub fn target_digest(target: &Value) -> String {
    let mut preimage = b"assay.mcp.target.v0\0".to_vec();
    preimage.extend_from_slice(&jcs::to_vec(target).unwrap_or_default());
    format!("sha256:{}", sha256_hex(&preimage))
}

/// A c1 decision. There is no `Allow` in c1 — every outcome is a deny with a reason.
pub struct Decision {
    pub reason: &'static str,
    pub action_class: Option<String>,
    pub target_digest: Option<String>,
}

/// The PDP. Precedence (first failing gate wins), all still deny (no allow/forward before c3):
/// 1. classification (before allowance, so a missing-target call reads as classification_incomplete,
///    never as a target mismatch);
/// 2. caller-allowance match (github_deploy_key {owner, repo} only so far);
/// 3. credential-scope (c2): the declared upstream credential must cover the action's required scope,
///    else credential_scope_insufficient / credential_scope_unknown;
/// 4. all enabled gates passed -> pdp_gate_unavailable (the drift gate c3 + allow path are not enabled).
pub fn decide(policy: &EnforcePolicy, tool_name: &str, args: &Value) -> Decision {
    let c = classify(tool_name, args);

    // 1. classification gate — fail-closed before any allowance matching.
    if c.category.is_none() {
        return Decision {
            reason: "unclassified_tool_call",
            action_class: None,
            target_digest: None,
        };
    }
    if c.state != "classified" {
        // classified_incomplete / redaction_failed / any non-final state -> not enough to authorize.
        return Decision {
            reason: "classification_incomplete",
            action_class: c.category.map(|s| s.to_string()),
            target_digest: Some(target_digest(&c.target)),
        };
    }

    let action_class = c.category.unwrap();
    let tdig = target_digest(&c.target);

    // 2. caller-allowance gate.
    let matched = policy
        .allowances
        .iter()
        .any(|a| a.action_class == action_class && allowance_matches(a, action_class, &c.target));
    if !matched {
        return Decision {
            reason: "no_declared_allowance",
            action_class: Some(action_class.to_string()),
            target_digest: Some(tdig),
        };
    }

    // 3. credential-scope gate (c2): the declared upstream credential must cover the action's required
    // scope. Fail-closed — an absent credential, an unrecognized scope, or a too-coarse scope is a
    // credential_scope_unknown (coverage cannot be determined), never a silent pass.
    if let Some(reason) = credential_scope_gate(policy, action_class) {
        return Decision {
            reason,
            action_class: Some(action_class.to_string()),
            target_digest: Some(tdig),
        };
    }

    // 4. c2 stops here: the drift gate (c3) and the allow/forward path are not enabled yet, so a call
    // that clears every enabled gate is still denied.
    Decision {
        reason: "pdp_gate_unavailable",
        action_class: Some(action_class.to_string()),
        target_digest: Some(tdig),
    }
}

/// Coverage of a declared credential's scopes against an action's required scope.
enum ScopeCoverage {
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
fn credential_scope_gate(policy: &EnforcePolicy, action_class: &str) -> Option<&'static str> {
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

/// The non-required scope vocabulary for one action class. `required` itself comes from
/// `required_scope_for` (one source of truth); this lattice only classifies the OTHER recognized
/// scopes, matching the E10 credential-overbreadth experiment so the gate and the measurement agree.
struct ScopeLattice {
    /// Covers the required scope without admin breadth.
    broader_ok: &'static [&'static str],
    /// Covers via admin/wildcard breadth (overbroad — still covers; a recommendation, not a block).
    overbroad: &'static [&'static str],
    /// Recognized but does not cover.
    non_covering: &'static [&'static str],
    /// Recognized but too coarse to tell action-specific from admin (-> Unknown, never forced).
    ambiguous: &'static [&'static str],
}

fn lattice_for(action_class: &str) -> Option<ScopeLattice> {
    match action_class {
        "github_deploy_key" => Some(ScopeLattice {
            broader_ok: &["repo:write"],
            overbroad: &["repo:admin", "admin", "*"],
            non_covering: &["repo:read", "repo:metadata"],
            ambiguous: &["repo"],
        }),
        "slack_add_member" => Some(ScopeLattice {
            broader_ok: &["conversations:write"],
            overbroad: &["admin", "workspace:admin", "*"],
            non_covering: &["conversations:read"],
            ambiguous: &["conversations"],
        }),
        "workspace_admin" => Some(ScopeLattice {
            // required is already admin-level: there is no non-admin broader scope.
            broader_ok: &[],
            overbroad: &["*", "org:admin", "superadmin"],
            non_covering: &["workspace:read", "member"],
            ambiguous: &["workspace"],
        }),
        _ => None,
    }
}

/// Deterministic scope coverage, mirroring the E10 lattice precedence: a too-coarse (ambiguous) or
/// unrecognized scope yields Unknown BEFORE any insufficiency verdict ("unknown is not insufficient").
fn scope_covers(action_class: &str, required: &str, declared: &[String]) -> ScopeCoverage {
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

/// c1 only knows the `github_deploy_key` target shape ({owner, repo}, projected plain by the P57c
/// classifier — owner/repo are sanitized for control chars only, never hashed, so a plain string
/// compare against the declared allowance is correct). Any other action_class has no verifiable
/// matcher in c1, so it is fail-closed (no match) and its allowance arrives with that class's own slice.
fn allowance_matches(a: &Allowance, action_class: &str, target: &Value) -> bool {
    if action_class != "github_deploy_key" {
        return false;
    }
    let owner = target.get("owner").and_then(|v| v.as_str());
    let repo = target.get("repo").and_then(|v| v.as_str());
    match (owner, repo) {
        (Some(o), Some(r)) => a.targets.iter().any(|t| t.owner == o && t.repo == r),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn policy_from(yaml: &str) -> Result<EnforcePolicy> {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(yaml.as_bytes()).unwrap();
        load(f.path())
    }

    const VALID: &str = r#"
caller:
  id: "ci-agent"
upstream_credential:
  alias: "gh-deploy"
  scopes: ["repo:deploy_key:write"]
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

    /// An allow-acme policy with a custom upstream_credential block (pass "" for no credential).
    fn allow_acme_with_cred(cred_block: &str) -> EnforcePolicy {
        let yaml = format!(
            "caller:\n  id: \"ci-agent\"\n{cred_block}allowances:\n  - action_class: \"github_deploy_key\"\n    targets:\n      - {{ owner: \"acme\", repo: \"prod-app\" }}\n"
        );
        policy_from(&yaml).unwrap()
    }

    /// The one tools/call that matches the acme allowance (so the credential-scope gate is reached).
    fn acme_call() -> Value {
        json!({"owner": "acme", "repo": "prod-app"})
    }

    #[test]
    fn loads_a_valid_policy() {
        let p = policy_from(VALID).unwrap();
        assert_eq!(p.caller.id, "ci-agent");
        assert_eq!(p.allowances.len(), 1);
        assert!(p.upstream_credential.is_some());
    }

    #[test]
    fn missing_caller_id_fails_load() {
        assert!(policy_from("allowances: []\n").is_err());
        assert!(policy_from("caller:\n  id: \"\"\n").is_err());
    }

    #[test]
    fn malformed_yaml_fails_load() {
        assert!(policy_from("caller: : :\n").is_err());
    }

    #[test]
    fn unclassified_tool_is_denied_unclassified() {
        let p = policy_from(VALID).unwrap();
        let d = decide(&p, "misc.do_thing", &json!({}));
        assert_eq!(d.reason, "unclassified_tool_call");
    }

    #[test]
    fn incomplete_classification_is_denied_before_allowance() {
        // Missing repo -> classified_incomplete; must read as classification_incomplete, NOT
        // no_declared_allowance/target-mismatch.
        let p = policy_from(VALID).unwrap();
        let d = decide(&p, "github.add_deploy_key", &json!({"owner": "acme"}));
        assert_eq!(d.reason, "classification_incomplete");
    }

    #[test]
    fn classified_without_allowance_is_denied_no_declared_allowance() {
        let p = policy_from(VALID).unwrap();
        let d = decide(
            &p,
            "github.add_deploy_key",
            &json!({"owner": "other", "repo": "x"}),
        );
        assert_eq!(d.reason, "no_declared_allowance");
        assert_eq!(d.action_class.as_deref(), Some("github_deploy_key"));
    }

    #[test]
    fn matching_allowance_and_covering_scope_reaches_pdp_gate_unavailable() {
        // VALID declares a credential whose scope exactly covers the required scope, so the call
        // clears the allowance AND credential-scope gates — and is still denied (no allow path yet).
        let p = policy_from(VALID).unwrap();
        let d = decide(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(
            d.reason, "pdp_gate_unavailable",
            "every enabled gate passed; there is no allow path before c3"
        );
    }

    // --- c2 credential-scope gate (runs only after the allowance matches) -----------------------

    #[test]
    fn no_declared_credential_is_credential_scope_unknown() {
        // Allowance matches, but no credential is declared -> coverage cannot be determined.
        let p = allow_acme_with_cred("");
        let d = decide(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "credential_scope_unknown");
    }

    #[test]
    fn non_covering_scope_is_credential_scope_insufficient() {
        let p = allow_acme_with_cred(
            "upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:read\"]\n",
        );
        let d = decide(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "credential_scope_insufficient");
    }

    #[test]
    fn too_coarse_scope_is_credential_scope_unknown_not_insufficient() {
        // "repo" is ambiguous (can't tell action-specific from admin) -> unknown takes precedence.
        let p =
            allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo\"]\n");
        let d = decide(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "credential_scope_unknown");
    }

    #[test]
    fn unrecognized_scope_is_credential_scope_unknown() {
        let p =
            allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"banana\"]\n");
        let d = decide(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "credential_scope_unknown");
    }

    #[test]
    fn broader_non_admin_scope_covers_and_reaches_pdp() {
        let p = allow_acme_with_cred(
            "upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:write\"]\n",
        );
        let d = decide(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "pdp_gate_unavailable");
    }

    #[test]
    fn overbroad_scope_still_covers_and_reaches_pdp() {
        // overbroad covers the required scope; overbroad is a recommendation, never a block in v0.
        let p = allow_acme_with_cred(
            "upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:admin\"]\n",
        );
        let d = decide(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "pdp_gate_unavailable");
    }

    #[test]
    fn unknown_required_scope_fails_closed() {
        // Defensive: a class with no required_scope must deny (unknown), never pass — guards against
        // the `?` fail-open trap in credential_scope_gate.
        let p = policy_from(VALID).unwrap();
        assert_eq!(
            credential_scope_gate(&p, "not_a_real_class"),
            Some("credential_scope_unknown")
        );
    }

    #[test]
    fn credential_gate_runs_after_allowance_not_before() {
        // A non-matching target denies at the allowance gate even with a fully insufficient credential
        // (precedence: allowance before credential-scope).
        let p = allow_acme_with_cred(
            "upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:read\"]\n",
        );
        let d = decide(
            &p,
            "github.add_deploy_key",
            &json!({"owner": "other", "repo": "x"}),
        );
        assert_eq!(d.reason, "no_declared_allowance");
    }

    #[test]
    fn scope_covers_matches_the_e10_lattice() {
        let req = "repo:deploy_key:write";
        let cov = |s: &[&str]| {
            matches!(
                scope_covers(
                    "github_deploy_key",
                    req,
                    &s.iter().map(|x| x.to_string()).collect::<Vec<_>>()
                ),
                ScopeCoverage::Covered
            )
        };
        assert!(cov(&["repo:deploy_key:write"]), "exact");
        assert!(cov(&["repo:write"]), "broader_ok");
        assert!(cov(&["repo:admin"]), "overbroad covers");
        assert!(matches!(
            scope_covers("github_deploy_key", req, &["repo:read".to_string()]),
            ScopeCoverage::Insufficient
        ));
        assert!(matches!(
            scope_covers("github_deploy_key", req, &["repo".to_string()]),
            ScopeCoverage::Unknown
        ));
        assert!(
            matches!(
                scope_covers(
                    "github_deploy_key",
                    req,
                    &["repo".to_string(), "repo:read".to_string()]
                ),
                ScopeCoverage::Unknown
            ),
            "ambiguous takes precedence over insufficient"
        );
        // No lattice for an unknown class -> Unknown (fail-closed).
        assert!(matches!(
            scope_covers("nope", req, &["repo:write".to_string()]),
            ScopeCoverage::Unknown
        ));
    }

    #[test]
    fn target_digest_is_domain_separated_and_stable() {
        let a = target_digest(&json!({"owner": "acme", "repo": "prod-app"}));
        let b = target_digest(&json!({"repo": "prod-app", "owner": "acme"}));
        assert_eq!(a, b, "canonical: key order independent");
        assert!(a.starts_with("sha256:"));
        // Domain separation: the same bytes under a different domain would differ (sanity).
        let raw = format!(
            "sha256:{}",
            sha256_hex(&jcs::to_vec(&json!({"owner": "acme", "repo": "prod-app"})).unwrap())
        );
        assert_ne!(
            a, raw,
            "domain prefix must change the digest vs a bare hash"
        );
    }
}
