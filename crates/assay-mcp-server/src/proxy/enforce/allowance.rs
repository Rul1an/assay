use super::policy::Allowance;
use assay_core::mcp::jcs;
use assay_mcp_server::cache::sha256_hex;
use serde_json::Value;

/// Domain-separated, canonical digest of a projected target for the DIAGNOSTIC decision log only.
/// Stable for correlation, never a raw target, never an evidence artifact.
pub fn target_digest(target: &Value) -> String {
    let mut preimage = b"assay.mcp.target.v0\0".to_vec();
    preimage.extend_from_slice(&jcs::to_vec(target).unwrap_or_default());
    format!("sha256:{}", sha256_hex(&preimage))
}

/// c1 only knows the `github_deploy_key` target shape ({owner, repo}, projected plain by the P57c
/// classifier — owner/repo are sanitized for control chars only, never hashed, so a plain string
/// compare against the declared allowance is correct). Any other action_class has no verifiable
/// matcher in c1, so it is fail-closed (no match) and its allowance arrives with that class's own slice.
pub(crate) fn allowance_matches(a: &Allowance, action_class: &str, target: &Value) -> bool {
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
