// crates/assay-core/src/agentic/mod.rs
// A reusable "Agentic Contract" builder that turns Diagnostics into:
// - suggested_actions (commands to run)
// - suggested_patches (JSON Patch ops, machine-applicable)
//
// This is intentionally conservative + deterministic.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;

use crate::errors::diagnostic::Diagnostic;

mod builder;
mod policy_helpers;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedAction {
    pub id: String,
    pub title: String,
    pub risk: RiskLevel,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedPatch {
    pub id: String,
    pub title: String,
    pub risk: RiskLevel,
    pub file: String, // path relative to cwd (or absolute)
    pub ops: Vec<JsonPatchOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum JsonPatchOp {
    Add { path: String, value: JsonValue },
    Remove { path: String },
    Replace { path: String, value: JsonValue },
    Move { from: String, path: String },
}

/// Context for Agentic suggestions.
///
/// This provides the "world view" needed to generate relevant fixes,
/// such as where the policy file is located or what the assay config path is.
pub struct AgenticCtx {
    /// Optional: path to the *policy* file (policy.yaml).
    /// If not set, we fall back to diagnostics.context.policy_file or "policy.yaml".
    pub policy_path: Option<PathBuf>,

    /// Optional: path to the *assay config* file (assay.yaml).
    /// If not set, we fall back to diagnostics.context.config_file or "assay.yaml".
    pub config_path: Option<PathBuf>,
}

/// Main entrypoint: build suggestions for any diagnostics list.
///
/// Analyzes a list of `Diagnostic` items and generates:
/// 1. `SuggestedAction`: High-level commands (e.g., `assay fix`, `mkdir`).
/// 2. `SuggestedPatch`: Concrete JSON Patch operations to apply to files.
///
/// The generation is deterministic and stateless (except for reading files referenced in context).
pub fn build_suggestions(
    diags: &[Diagnostic],
    ctx: &AgenticCtx,
) -> (Vec<SuggestedAction>, Vec<SuggestedPatch>) {
    builder::build_suggestions_impl(diags, ctx)
}

#[cfg(test)]
mod tests;
