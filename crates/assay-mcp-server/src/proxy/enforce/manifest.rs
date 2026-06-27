use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;

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
