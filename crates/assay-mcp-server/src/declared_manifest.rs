//! #2654 Slice A: the strict `assay.declared_mcp_manifest.v0` model.
//!
//! This is the ONE validator for an operator-declared approval baseline. It is deliberately in the
//! library, not behind the binary's `proxy` module, so that production startup
//! (`--declared-mcp-manifest`) and the fixture guard validate through the same code rather than
//! through two descriptions of the same rule.
//!
//! Load-bearing rules, each of which used to be silently unenforced:
//!
//! - **Closed members.** `deny_unknown_fields` everywhere. A baseline carrying a member this model
//!   does not know is refused, because an approval that contains something nobody validated is not
//!   an approval. The known-optional members below are the ones the committed baselines already
//!   carry; they are declared so they stay checkable, not so they stay ignorable.
//! - **Exact canonicalization id.** A baseline must name
//!   [`crate::manifest_observed::CANONICALIZATION`]. A digest is only comparable to another digest computed
//!   the same way, so a baseline that will not say how it was computed cannot be compared.
//! - **Full digest syntax.** `sha256:` plus exactly 64 lowercase hex. A prefix check accepts
//!   `sha256:` alone, and the drift gate compares digests as exact bytes.
//! - **Recomputed manifest digest.** `manifest_digest` must recompute from the declared
//!   `(name, tool_digest)` pairs through [`crate::manifest_observed::manifest_digest`] — the same function
//!   the observed producer uses. This is what makes a per-tool digest un-editable in isolation.
//! - **Order independence.** That shared function sorts its entries, so a baseline's validity does
//!   not depend on the order tools appear in the file. The committed baselines are not name-sorted,
//!   and requiring them to be would break bytes an operator already approved.
//!
//! Non-claims: this validates the baseline's internal truth, never that the baseline is the *right*
//! approval. It performs no candidate generation, no promotion, and no first-observation trust.

use crate::manifest_observed::{manifest_digest, CANONICALIZATION};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const DECLARED_MANIFEST_SCHEMA: &str = "assay.declared_mcp_manifest.v0";

/// The declaring server's identity. Carried for operator provenance; not part of any digest.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeclaredServer {
    pub id: String,
}

/// One approved tool. `name` and `tool_digest` are the approval; the rest is attribution the
/// producer also emits, declared here so it is validated rather than ignored.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BaselineTool {
    pub name: String,
    pub tool_digest: String,
    #[serde(default)]
    pub privileged: Option<bool>,
    #[serde(default)]
    pub privilege_classification: Option<String>,
    #[serde(default)]
    pub action_class: Option<String>,
    /// P60d-v2 attribution. Optional, and deliberately OUTSIDE the manifest-digest preimage, so its
    /// presence moves no digest. Values are still syntax-checked: an unreadable digest is not
    /// attribution.
    #[serde(default)]
    pub field_digests: Option<BTreeMap<String, String>>,
}

/// The operator-pinned approval-time baseline. The ONLY source of the approval baseline — never the
/// first observed session manifest (spec §16-B).
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeclaredManifest {
    pub schema: String,
    pub canonicalization: String,
    pub manifest_digest: String,
    pub tools: Vec<BaselineTool>,
    /// Free-text operator note. Declared so it is permitted and bounded, never interpreted.
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub server: Option<DeclaredServer>,
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

/// `sha256:` + exactly 64 lowercase hex. Exact bytes, because the drift gate compares exact bytes.
pub(crate) fn is_canonical_sha256(s: &str) -> bool {
    match s.strip_prefix("sha256:") {
        Some(hex) => {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }
        None => false,
    }
}

/// Parse and strictly validate a declared baseline from JSON text.
///
/// Every failure is a hard error for the caller to turn into a startup failure: in enforcing mode a
/// proxy that would forward privileged calls without a valid baseline must not start.
pub fn parse_declared_manifest(text: &str) -> Result<DeclaredManifest> {
    let value = assay_core::mcp::parse_unique_json(text)?;
    let manifest: DeclaredManifest = serde_json::from_value(value)?;
    validate_declared_manifest(manifest)
}

/// Read and validate a declared baseline through the shared manifest byte ceiling.
pub fn load_declared_manifest(path: &Path) -> Result<DeclaredManifest> {
    let bytes = crate::manifest_io::read_bounded_bytes(path)?;
    let text = std::str::from_utf8(&bytes)
        .with_context(|| format!("declared manifest {} is not UTF-8", path.display()))?;
    parse_declared_manifest(text)
}

pub fn construct_declared_manifest(
    canonicalization: String,
    manifest_digest: String,
    tools: Vec<BaselineTool>,
    server: Option<DeclaredServer>,
) -> Result<DeclaredManifest> {
    validate_declared_manifest(DeclaredManifest {
        schema: DECLARED_MANIFEST_SCHEMA.to_string(),
        canonicalization,
        manifest_digest,
        tools,
        note: None,
        server,
    })
}

fn validate_declared_manifest(manifest: DeclaredManifest) -> Result<DeclaredManifest> {
    if manifest.schema != DECLARED_MANIFEST_SCHEMA {
        bail!(
            "schema must be {DECLARED_MANIFEST_SCHEMA}, got {:?}",
            manifest.schema
        );
    }
    if manifest.canonicalization != CANONICALIZATION {
        bail!(
            "canonicalization must be {CANONICALIZATION}, got {:?}; a digest is only comparable to one computed the same way",
            manifest.canonicalization
        );
    }
    if !is_canonical_sha256(&manifest.manifest_digest) {
        bail!(
            "manifest_digest must be sha256: plus 64 lowercase hex, got {:?}",
            manifest.manifest_digest
        );
    }
    if manifest.tools.is_empty() {
        bail!("tools must be a non-empty array");
    }

    let mut seen = std::collections::HashSet::new();
    for t in &manifest.tools {
        if t.name.trim().is_empty() {
            bail!("every tool must have a non-empty name");
        }
        if !is_canonical_sha256(&t.tool_digest) {
            bail!(
                "tool {:?} tool_digest must be sha256: plus 64 lowercase hex, got {:?}",
                t.name,
                t.tool_digest
            );
        }
        if let Some(fields) = &t.field_digests {
            for (field, digest) in fields {
                if !crate::manifest_observed::FIELD_NAMES.contains(&field.as_str()) {
                    bail!("tool {:?} has unknown field_digest key {:?}", t.name, field);
                }
                if !is_canonical_sha256(digest) {
                    bail!(
                        "tool {:?} field_digest {:?} must be sha256: plus 64 lowercase hex, got {:?}",
                        t.name,
                        field,
                        digest
                    );
                }
            }
        }
        // Duplicate declared names are `declared_mcp_manifest_ambiguous` (manifest-drift contract): a
        // first-match-wins lookup over an ambiguous approval baseline is unsafe, so fail startup.
        if !seen.insert(t.name.as_str()) {
            bail!(
                "duplicate tool name {:?} (an approval baseline must be unambiguous)",
                t.name
            );
        }
    }

    // The declared manifest_digest must recompute from the declared pairs, through the SAME rule the
    // observed producer uses. This is what stops one tool_digest from being edited in isolation.
    let pairs: Vec<(String, String)> = manifest
        .tools
        .iter()
        .map(|t| (t.name.clone(), t.tool_digest.clone()))
        .collect();
    let recomputed = manifest_digest(&pairs);
    if recomputed != manifest.manifest_digest {
        bail!(
            "manifest_digest does not recompute from the declared tools: file says {:?}, tools yield {:?}",
            manifest.manifest_digest,
            recomputed
        );
    }

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::is_canonical_sha256;

    #[test]
    fn canonical_sha256_syntax_is_closed_and_shared() {
        assert!(is_canonical_sha256(&format!("sha256:{}", "a".repeat(64))));
        assert!(!is_canonical_sha256(&format!("sha256:{}", "a".repeat(63))));
        assert!(!is_canonical_sha256(&format!("sha256:{}", "A".repeat(64))));
        assert!(!is_canonical_sha256(&format!("sha512:{}", "a".repeat(64))));
    }
}
