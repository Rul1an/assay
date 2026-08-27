use anyhow::{Context, Result};
use std::path::Path;

// #2654 Slice A: the declared-v0 model and its strict validation live in the library
// (`crate::declared_manifest`) so that startup and the fixture guard share one implementation
// instead of two descriptions of the same rule. This module keeps the startup wiring only.
pub use assay_mcp_server::declared_manifest::{BaselineTool, DeclaredManifest};

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
///
/// The bounded read and validation live in `crate::declared_manifest::load_declared_manifest`;
/// this function adds only the caller-facing flag name.
pub fn load_declared_manifest(path: &Path) -> Result<DeclaredManifest> {
    assay_mcp_server::declared_manifest::load_declared_manifest(path)
        .with_context(|| format!("--declared-mcp-manifest {}", path.display()))
}
