//! P61e-c1/c2/c3: the enforcing-proxy policy decision point — caller-allowance + credential-scope +
//! drift gates, plus the first allow/forward path. Spec: docs/reference/mcp-upstream-proxy-enforcement.md.
//!
//! Scope: parse the `--enforce-policy` file and the `--declared-mcp-manifest` baseline, classify an
//! observed `tools/call`, and decide. c1 added the caller-allowance gate; c2 the credential-scope gate;
//! c3 the drift gate (the current observed per-tool digest must equal the approved baseline digest,
//! with both a baseline and a current COMPLETE observation required) AND the first allow path: a call
//! that clears every gate is forwarded. The temporary `pdp_gate_unavailable` reason is gone — every
//! outcome is now either a precedence-pinned deny or an allow.
//!
//! Caller identity is the static `caller.id` from the policy only — no transport/env/request
//! inference — so a single stdio session is bound to one configured caller and `unknown_caller`
//! cannot occur at runtime (a policy without `caller.id` fails startup).

use anyhow::{bail, Context, Result};
use assay_core::mcp::jcs;
use assay_mcp_server::cache::sha256_hex;
use assay_mcp_server::tool_decision::{classify, required_scope_for, sanitize};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The enforce-mode inputs to the proxy, grouped so `run` stays within a sane arity. All fields are
/// absent in observe mode; `policy` and `baseline` are always present in enforce mode (loaded at
/// startup) and `decision_out` is the optional P61e-d evidence path.
#[derive(Default)]
pub struct EnforceInputs {
    pub policy: Option<EnforcePolicy>,
    pub baseline: Option<DeclaredManifest>,
    pub decision_out: Option<PathBuf>,
}

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

const DECLARED_MANIFEST_SCHEMA: &str = "assay.declared_mcp_manifest.v0";

/// The operator-pinned approval-time baseline (`assay.declared_mcp_manifest.v0`): the per-tool
/// `tool_digest` the caller approved. The drift gate (c3) compares the current observed per-tool digest
/// against this. It is the ONLY source of the approval baseline — never the first observed session
/// manifest (spec §16-B).
#[derive(Debug, Deserialize)]
pub struct DeclaredManifest {
    pub schema: String,
    #[serde(default)]
    pub tools: Vec<BaselineTool>,
}

#[derive(Debug, Deserialize)]
pub struct BaselineTool {
    pub name: String,
    pub tool_digest: String,
}

impl DeclaredManifest {
    /// The approved `tool_digest` for `name`, or `None` if this tool has no approved baseline.
    pub fn tool_digest_for(&self, name: &str) -> Option<&str> {
        self.tools
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.tool_digest.as_str())
    }
}

/// The current observed per-tool digest for the invoked tool, computed by the proxy from its own
/// observed `tools/list` (P61c). Distinguishes "no complete manifest observed this session" from
/// "observed complete but this tool is absent" — both are fail-closed, never an allow.
pub enum ObservedToolDigest {
    /// No COMPLETE `tools/list` has been observed this session, or the last complete observation was
    /// invalidated by a later `tools/list_changed` and not yet re-observed.
    NoCompleteManifest,
    /// The complete observed manifest has duplicate tool names (`status: ambiguous`): inconclusive, so
    /// the drift gate must deny rather than pick one of the colliding per-tool digests.
    Ambiguous,
    /// A complete manifest was observed, but it does not contain the invoked tool.
    CompleteButToolAbsent,
    /// The current observed `tool_digest` for the invoked tool.
    Present(String),
}

/// Load + STRICTLY validate the declared-manifest baseline. Like the enforce policy, any failure here
/// is a STARTUP failure (non-zero exit), never a runtime deny: in enforcing mode an approval baseline
/// is required, and a proxy that would forward privileged calls without a valid baseline must not start.
pub fn load_declared_manifest(path: &Path) -> Result<DeclaredManifest> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading --declared-mcp-manifest {}", path.display()))?;
    let manifest: DeclaredManifest =
        serde_json::from_str(&text).with_context(|| "parsing --declared-mcp-manifest JSON")?;
    if manifest.schema != DECLARED_MANIFEST_SCHEMA {
        bail!(
            "--declared-mcp-manifest: schema must be {DECLARED_MANIFEST_SCHEMA}, got {:?}",
            manifest.schema
        );
    }
    if manifest.tools.is_empty() {
        bail!("--declared-mcp-manifest: tools must be a non-empty array");
    }
    let mut seen = std::collections::HashSet::new();
    for t in &manifest.tools {
        if t.name.trim().is_empty() {
            bail!("--declared-mcp-manifest: every tool must have a non-empty name");
        }
        if !t.tool_digest.starts_with("sha256:") {
            bail!(
                "--declared-mcp-manifest: tool {:?} tool_digest must be a sha256: digest, got {:?}",
                t.name,
                t.tool_digest
            );
        }
        // Duplicate declared names are `declared_mcp_manifest_ambiguous` (manifest-drift contract): a
        // first-match-wins lookup over an ambiguous approval baseline is unsafe, so fail startup.
        if !seen.insert(t.name.as_str()) {
            bail!(
                "--declared-mcp-manifest: duplicate tool name {:?} (an approval baseline must be unambiguous)",
                t.name
            );
        }
    }
    Ok(manifest)
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
fn drift_state(decision: &Decision) -> &'static str {
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

    const TOOL: &str = "github.add_deploy_key";
    const APPROVED: &str = "sha256:approved-digest";

    /// A single-tool declared baseline (loaded + strictly validated like at startup).
    fn baseline_with(name: &str, digest: &str) -> DeclaredManifest {
        let j = format!(
            r#"{{"schema":"assay.declared_mcp_manifest.v0","tools":[{{"name":"{name}","tool_digest":"{digest}"}}]}}"#
        );
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(j.as_bytes()).unwrap();
        load_declared_manifest(f.path()).unwrap()
    }

    /// `decide` with a baseline + observed digest that MATCH for `github.add_deploy_key`, so only the
    /// gates BEFORE the drift gate can produce a deny (the classification/allowance/credential tests).
    /// A call that also clears those earlier gates therefore reaches an ALLOW here.
    fn decide_match(p: &EnforcePolicy, tool: &str, args: &Value) -> Decision {
        decide(
            p,
            &baseline_with(TOOL, APPROVED),
            &ObservedToolDigest::Present(APPROVED.to_string()),
            tool,
            args,
        )
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
        let d = decide_match(&p, "misc.do_thing", &json!({}));
        assert_eq!(d.reason, "unclassified_tool_call");
    }

    #[test]
    fn incomplete_classification_is_denied_before_allowance() {
        // Missing repo -> classified_incomplete; must read as classification_incomplete, NOT
        // no_declared_allowance/target-mismatch.
        let p = policy_from(VALID).unwrap();
        let d = decide_match(&p, "github.add_deploy_key", &json!({"owner": "acme"}));
        assert_eq!(d.reason, "classification_incomplete");
    }

    #[test]
    fn classified_without_allowance_is_denied_no_declared_allowance() {
        let p = policy_from(VALID).unwrap();
        let d = decide_match(
            &p,
            "github.add_deploy_key",
            &json!({"owner": "other", "repo": "x"}),
        );
        assert_eq!(d.reason, "no_declared_allowance");
        assert_eq!(d.action_class.as_deref(), Some("github_deploy_key"));
    }

    #[test]
    fn all_gates_pass_with_matching_digest_allows() {
        // VALID clears classification + allowance + credential-scope; with a baseline + observed digest
        // that match (decide_match), the drift gate passes too -> the one and only allow path fires.
        let p = policy_from(VALID).unwrap();
        let d = decide_match(&p, "github.add_deploy_key", &acme_call());
        assert!(d.allow, "every gate passed -> forward");
        assert_eq!(d.reason, "allow");
    }

    // --- c2 credential-scope gate (runs only after the allowance matches) -----------------------

    #[test]
    fn no_declared_credential_is_credential_scope_unknown() {
        // Allowance matches, but no credential is declared -> coverage cannot be determined.
        let p = allow_acme_with_cred("");
        let d = decide_match(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "credential_scope_unknown");
    }

    #[test]
    fn non_covering_scope_is_credential_scope_insufficient() {
        let p = allow_acme_with_cred(
            "upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:read\"]\n",
        );
        let d = decide_match(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "credential_scope_insufficient");
    }

    #[test]
    fn coarse_repo_scope_is_unrecognized_and_unknown() {
        // "repo" is NOT in the documented contract (only repo:deploy_key:write / repo:admin cover,
        // repo:read/metadata/contents:read are recognized-non-covering); so "repo" is unrecognized
        // -> unknown, never silently covered and never "insufficient".
        let p =
            allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo\"]\n");
        let d = decide_match(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "credential_scope_unknown");
    }

    #[test]
    fn unrecognized_scope_is_credential_scope_unknown() {
        let p =
            allow_acme_with_cred("upstream_credential:\n  alias: \"gh\"\n  scopes: [\"banana\"]\n");
        let d = decide_match(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "credential_scope_unknown");
    }

    #[test]
    fn repo_write_does_not_cover_deploy_key_per_p59_contract() {
        // repo:write is NOT a covering scope in credential-scope.md — it is unrecognized -> unknown,
        // NOT a silent pass. (The c2 gate must never be broader than the documented contract.)
        let p = allow_acme_with_cred(
            "upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:write\"]\n",
        );
        let d = decide_match(&p, "github.add_deploy_key", &acme_call());
        assert_eq!(d.reason, "credential_scope_unknown");
    }

    #[test]
    fn repo_admin_covers_then_matching_digest_allows() {
        // repo:admin covers the credential gate (admin breadth); with a matching baseline + observed
        // digest the drift gate passes too -> allow.
        let p = allow_acme_with_cred(
            "upstream_credential:\n  alias: \"gh\"\n  scopes: [\"repo:admin\"]\n",
        );
        let d = decide_match(&p, "github.add_deploy_key", &acme_call());
        assert!(d.allow);
        assert_eq!(d.reason, "allow");
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
        let d = decide_match(
            &p,
            "github.add_deploy_key",
            &json!({"owner": "other", "repo": "x"}),
        );
        assert_eq!(d.reason, "no_declared_allowance");
    }

    #[test]
    fn scope_covers_matches_the_p59_contract() {
        // The enforcement lattice equals credential-scope.md exactly: covered by
        // {repo:deploy_key:write, repo:admin}; NOT covered by {repo:read, repo:metadata,
        // repo:contents:read}; everything else (incl. repo:write, repo) is unrecognized -> unknown.
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
        assert!(cov(&["repo:deploy_key:write"]), "exact covers");
        assert!(cov(&["repo:admin"]), "admin breadth covers");
        // repo:write is NOT in the documented contract -> unrecognized -> unknown (not covered).
        assert!(matches!(
            scope_covers("github_deploy_key", req, &["repo:write".to_string()]),
            ScopeCoverage::Unknown
        ));
        for ns in ["repo:read", "repo:metadata", "repo:contents:read"] {
            assert!(
                matches!(
                    scope_covers("github_deploy_key", req, &[ns.to_string()]),
                    ScopeCoverage::Insufficient
                ),
                "{ns} is recognized-non-covering -> insufficient"
            );
        }
        assert!(matches!(
            scope_covers("github_deploy_key", req, &["repo".to_string()]),
            ScopeCoverage::Unknown
        ));
        // An unrecognized scope alongside a recognized non-covering one -> unknown takes precedence.
        assert!(
            matches!(
                scope_covers(
                    "github_deploy_key",
                    req,
                    &["banana".to_string(), "repo:read".to_string()]
                ),
                ScopeCoverage::Unknown
            ),
            "unknown takes precedence over insufficient"
        );
        // No documented lattice for another class -> Unknown (fail-closed).
        assert!(matches!(
            scope_covers("slack_add_member", req, &["repo:admin".to_string()]),
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

    // --- c3 drift gate (runs only after classification + allowance + credential-scope pass) ---------

    /// `decide` for the acme call with VALID, a given baseline, and a given observed digest state.
    fn decide_drift(baseline: &DeclaredManifest, observed: &ObservedToolDigest) -> Decision {
        let p = policy_from(VALID).unwrap();
        decide(
            &p,
            baseline,
            observed,
            "github.add_deploy_key",
            &acme_call(),
        )
    }

    #[test]
    fn baseline_missing_when_tool_absent_from_baseline() {
        // A baseline that has SOME tool but not the invoked one -> this tool has no approved baseline.
        let baseline = baseline_with("other.tool", APPROVED);
        let d = decide_drift(
            &baseline,
            &ObservedToolDigest::Present(APPROVED.to_string()),
        );
        assert!(!d.allow);
        assert_eq!(d.reason, "manifest_baseline_missing");
    }

    #[test]
    fn current_observation_incomplete_when_no_complete_manifest() {
        let baseline = baseline_with(TOOL, APPROVED);
        let d = decide_drift(&baseline, &ObservedToolDigest::NoCompleteManifest);
        assert!(!d.allow);
        assert_eq!(d.reason, "manifest_current_observation_incomplete");
    }

    #[test]
    fn current_observation_incomplete_when_tool_absent_from_observed() {
        // A complete manifest that does not contain the invoked tool: no current digest to compare,
        // so drift cannot be ruled out -> fail closed (not an allow).
        let baseline = baseline_with(TOOL, APPROVED);
        let d = decide_drift(&baseline, &ObservedToolDigest::CompleteButToolAbsent);
        assert!(!d.allow);
        assert_eq!(d.reason, "manifest_current_observation_incomplete");
    }

    #[test]
    fn ambiguous_observation_denies_observation_ambiguous() {
        // A duplicate-name (ambiguous) observed manifest is inconclusive -> deny, never pick a digest.
        let baseline = baseline_with(TOOL, APPROVED);
        let d = decide_drift(&baseline, &ObservedToolDigest::Ambiguous);
        assert!(!d.allow);
        assert_eq!(d.reason, "manifest_observation_ambiguous");
    }

    #[test]
    fn drifted_when_observed_digest_differs_from_baseline() {
        let baseline = baseline_with(TOOL, APPROVED);
        let d = decide_drift(
            &baseline,
            &ObservedToolDigest::Present("sha256:something-else".to_string()),
        );
        assert!(!d.allow);
        assert_eq!(d.reason, "manifest_drifted_since_approval");
    }

    #[test]
    fn allows_only_when_baseline_and_observed_digests_match_exactly() {
        let baseline = baseline_with(TOOL, APPROVED);
        let d = decide_drift(
            &baseline,
            &ObservedToolDigest::Present(APPROVED.to_string()),
        );
        assert!(
            d.allow,
            "exact digest match clears the drift gate -> forward"
        );
        assert_eq!(d.reason, "allow");
    }

    // --- declared-manifest baseline loader (strict, startup-validated) ------------------------------

    fn manifest_from(json: &str) -> Result<DeclaredManifest> {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(json.as_bytes()).unwrap();
        load_declared_manifest(f.path())
    }

    #[test]
    fn valid_baseline_loads() {
        let m = manifest_from(
            r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"github.add_deploy_key","tool_digest":"sha256:abc"}]}"#,
        )
        .unwrap();
        assert_eq!(
            m.tool_digest_for("github.add_deploy_key"),
            Some("sha256:abc")
        );
        assert_eq!(m.tool_digest_for("nope"), None);
    }

    #[test]
    fn wrong_schema_baseline_fails() {
        assert!(manifest_from(
            r#"{"schema":"assay.mcp_manifest_observed.v0","tools":[{"name":"t","tool_digest":"sha256:abc"}]}"#
        )
        .is_err());
    }

    #[test]
    fn empty_tools_baseline_fails() {
        assert!(
            manifest_from(r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[]}"#).is_err()
        );
    }

    #[test]
    fn non_sha256_digest_fails() {
        assert!(manifest_from(
            r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"t","tool_digest":"deadbeef"}]}"#
        )
        .is_err());
    }

    #[test]
    fn tool_without_digest_fails() {
        // tool_digest is required (not Option) -> a tool missing it fails to parse.
        assert!(manifest_from(
            r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"t"}]}"#
        )
        .is_err());
    }

    #[test]
    fn duplicate_baseline_tool_names_fail_load() {
        // An approval baseline must be unambiguous: duplicate names fail startup (no first-match-wins).
        assert!(manifest_from(
            r#"{"schema":"assay.declared_mcp_manifest.v0","tools":[{"name":"t","tool_digest":"sha256:a"},{"name":"t","tool_digest":"sha256:b"}]}"#
        )
        .is_err());
    }

    // --- P61e-d: enforcement_decision.v0 record ----------------------------------------------------

    #[test]
    fn decision_record_for_a_deny_is_shaped_and_leak_free() {
        let p = policy_from(VALID).unwrap();
        let d = decide_match(&p, "misc.do_thing", &json!({})); // unclassified -> deny
        let rec = decision_record(&p, &d, "misc.do_thing", &json!({}));
        assert_eq!(rec["schema"], "assay.enforcement_decision.v0");
        assert_eq!(rec["decision"], "deny");
        assert_eq!(rec["reason"], "unclassified_tool_call");
        assert_eq!(rec["fail_closed"], true);
        assert_eq!(rec["drift_state"], "not_evaluated");
        assert_eq!(rec["caller"]["id"], "ci-agent");
        assert_eq!(rec["credential_alias"], "gh-deploy");
        assert!(rec["non_claims"].is_array());
        // The record carries no transport-outcome field — it must not claim delivery.
        assert!(
            rec.get("forwarded").is_none(),
            "no transport claim in the decision record"
        );
        // The declared scopes are never serialized into the record (alias only).
        let s = serde_json::to_string(&rec).unwrap();
        assert!(
            !s.contains("repo:deploy_key:write"),
            "declared credential scopes must not leak into the decision record"
        );
    }

    #[test]
    fn decision_record_for_an_allow_is_policy_decision_not_a_delivery_claim() {
        let p = policy_from(VALID).unwrap();
        let d = decide_match(&p, "github.add_deploy_key", &acme_call()); // matching -> allow
        assert!(d.allow);
        let rec = decision_record(&p, &d, "github.add_deploy_key", &acme_call());
        assert_eq!(rec["decision"], "allow");
        assert_eq!(rec["reason"], "allow");
        assert_eq!(rec["fail_closed"], false);
        assert_eq!(rec["drift_state"], "satisfied");
        assert_eq!(rec["tool"]["action_class"], "github_deploy_key");
        assert_eq!(rec["action"]["target"]["owner"], "acme");
        // The decision (allow) is the durable fact; the record never asserts the call was delivered.
        assert!(
            rec.get("forwarded").is_none(),
            "an allow decision must not be a transport/delivery claim"
        );
    }

    #[test]
    fn decision_record_drift_state_reflects_the_drift_gate() {
        let p = policy_from(VALID).unwrap();
        let baseline = baseline_with(TOOL, APPROVED);
        let d = decide(
            &p,
            &baseline,
            &ObservedToolDigest::Present("sha256:something-else".to_string()),
            "github.add_deploy_key",
            &acme_call(),
        );
        assert_eq!(d.reason, "manifest_drifted_since_approval");
        let rec = decision_record(&p, &d, "github.add_deploy_key", &acme_call());
        assert_eq!(rec["decision"], "deny");
        assert_eq!(rec["drift_state"], "drifted");
    }
}
