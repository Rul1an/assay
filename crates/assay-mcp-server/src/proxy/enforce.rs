//! P61e-c1: the enforcing-proxy policy decision point — caller-allowance gate, deny-only.
//! Spec: docs/reference/mcp-upstream-proxy-enforcement.md.
//!
//! c1 scope: parse the `--enforce-policy` file, classify an observed `tools/call`, and decide a DENY
//! with a precedence-pinned reason. There is no allow path and no forwarding in c1 (that arrives in
//! c3, after the credential-scope gate in c2). A `tools/call` that passes the c1 gates is still denied
//! with `pdp_gate_unavailable`, a temporary rollout reason removed when c3 lands.
//!
//! Caller identity is the static `caller.id` from the policy only — no transport/env/request
//! inference — so a single stdio session is bound to one configured caller and `unknown_caller`
//! cannot occur at runtime (a policy without `caller.id` fails startup).

use anyhow::{bail, Context, Result};
use assay_core::mcp::jcs;
use assay_mcp_server::cache::sha256_hex;
use assay_mcp_server::tool_decision::classify;
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct EnforcePolicy {
    pub caller: Caller,
    /// Parsed but UNUSED in c1; the credential-scope gate (c2) consumes it. Accepted now so a c2
    /// policy validates and loads unchanged when the gate lands.
    #[serde(default)]
    #[allow(dead_code)]
    pub upstream_credential: Option<UpstreamCredential>,
    #[serde(default)]
    pub allowances: Vec<Allowance>,
}

#[derive(Debug, Deserialize)]
pub struct Caller {
    pub id: String,
}

/// Parsed but UNUSED in c1; the credential-scope gate (c2) reads `scopes`.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct UpstreamCredential {
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

/// The c1 PDP. Precedence (first failing gate wins), all deny in c1:
/// 1. classification (before allowance, so a missing-target call reads as classification_incomplete,
///    never as a target mismatch);
/// 2. caller-allowance match (github_deploy_key {owner, repo} only in c1);
/// 3. all c1 gates passed -> pdp_gate_unavailable (the later gates are not enabled yet).
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

    // 3. c1 stops here: the credential-scope (c2) and drift (c3) gates are not enabled, and there is
    // deliberately no allow/forward path before c3.
    Decision {
        reason: "pdp_gate_unavailable",
        action_class: Some(action_class.to_string()),
        target_digest: Some(tdig),
    }
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
allowances:
  - action_class: "github_deploy_key"
    targets:
      - { owner: "acme", repo: "prod-app" }
"#;

    #[test]
    fn loads_a_valid_policy() {
        let p = policy_from(VALID).unwrap();
        assert_eq!(p.caller.id, "ci-agent");
        assert_eq!(p.allowances.len(), 1);
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
    fn matching_allowance_reaches_pdp_gate_unavailable() {
        let p = policy_from(VALID).unwrap();
        let d = decide(
            &p,
            "github.add_deploy_key",
            &json!({"owner": "acme", "repo": "prod-app"}),
        );
        assert_eq!(
            d.reason, "pdp_gate_unavailable",
            "c1 has no allow path; passed gates still deny"
        );
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
