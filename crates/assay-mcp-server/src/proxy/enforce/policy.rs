use super::manifest::DeclaredManifest;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The enforce-mode inputs to the proxy, grouped so `run` stays within a sane arity. All fields are
/// absent in observe mode; `policy` and `baseline` are always present in enforce mode (loaded at
/// startup), `decision_out` is the optional P61e-d evidence path, `establish_out` is the optional
/// `assay.manifest_establish.v0` carrier path (Increment 2c), and `establish_budget` is the one total
/// deadline for a pre-call establish run.
#[derive(Default)]
pub struct EnforceInputs {
    pub policy: Option<EnforcePolicy>,
    pub baseline: Option<DeclaredManifest>,
    pub decision_out: Option<PathBuf>,
    /// Optional NDJSON path for `assay.denied_call_observation.v0`: the caller-visible proxy-deny
    /// surface, bound to the call's tool and target digest when classification provides one. This is
    /// an observation carrier, not the policy verdict.
    pub denied_call_observation_out: Option<PathBuf>,
    pub establish_out: Option<PathBuf>,
    /// Optional NDJSON path for the per-call `assay.tool_annotation_conformance.v0` carrier
    /// (Increment 5b): the server's declared annotation hints vs Assay's observed call
    /// classification. Orthogonal to the verdict; on an allowed call a write failure fails closed,
    /// the same evidence rule as the other carriers.
    pub tool_conformance_out: Option<PathBuf>,
    pub establish_budget: std::time::Duration,
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
