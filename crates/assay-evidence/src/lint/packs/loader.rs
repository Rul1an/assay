//! Pack loader with YAML parsing, validation, and digest computation.
//!
//! # YAML Parsing
//! - Rejects unknown fields (`deny_unknown_fields`).
//! - Duplicate keys: rejected when detected by the YAML parser (not guaranteed at all nesting levels).
//! - Anchors/aliases: currently accepted; future versions may reject.
//! - Computes deterministic digest: sha256(JCS(JSON(yaml)))

use super::schema::{PackDefinition, PackValidationError};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[path = "loader_internal/mod.rs"]
mod loader_internal;

/// Source of a loaded pack.
#[derive(Debug, Clone)]
pub enum PackSource {
    /// Built-in pack (embedded at compile time).
    BuiltIn(&'static str),
    /// Pack loaded from file.
    File(PathBuf),
}

impl std::fmt::Display for PackSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackSource::BuiltIn(name) => write!(f, "builtin:{}", name),
            PackSource::File(path) => write!(f, "file:{}", path.display()),
        }
    }
}

/// A loaded and validated pack.
#[derive(Debug, Clone)]
pub struct LoadedPack {
    /// Pack definition.
    pub definition: PackDefinition,
    /// Pack digest (sha256 of JCS-canonical JSON).
    pub digest: String,
    /// Source of the pack.
    pub source: PackSource,
}

impl LoadedPack {
    /// Get the canonical ID for a rule.
    pub fn canonical_rule_id(&self, rule_id: &str) -> String {
        format!(
            "{}@{}:{}",
            self.definition.name, self.definition.version, rule_id
        )
    }
}

/// Pack loading error.
#[derive(Debug, Error)]
pub enum PackError {
    #[error("Pack '{reference}' not found. {suggestion}")]
    NotFound {
        reference: String,
        suggestion: String,
    },

    #[error("Failed to read pack file '{path}': {source}")]
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse pack YAML: {message}")]
    YamlParseError { message: String },

    #[error("Pack validation failed: {0}")]
    ValidationError(#[from] PackValidationError),

    #[error("Pack '{pack}' requires Assay >={required}, but current version is {current}")]
    IncompatibleVersion {
        pack: String,
        required: String,
        current: String,
    },

    #[error(
        "Rule collision in compliance packs: {rule_id} defined in both '{pack_a}' and '{pack_b}'"
    )]
    ComplianceCollision {
        rule_id: String,
        pack_a: String,
        pack_b: String,
    },
}

/// Load a pack from a reference (file path or built-in name).
pub fn load_pack(reference: &str) -> Result<LoadedPack, PackError> {
    loader_internal::run::load_pack_impl(reference)
}

/// Load multiple packs from references.
pub fn load_packs(references: &[String]) -> Result<Vec<LoadedPack>, PackError> {
    loader_internal::run::load_packs_impl(references)
}

/// Load a pack from a file path.
pub fn load_pack_from_file(path: &Path) -> Result<LoadedPack, PackError> {
    loader_internal::run::load_pack_from_file_impl(path)
}
