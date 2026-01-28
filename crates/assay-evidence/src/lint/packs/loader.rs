//! Pack loader with YAML parsing, validation, and digest computation.
//!
//! Implements strict YAML parsing per SPEC-Pack-Engine-v1:
//! - Rejects duplicate keys
//! - Rejects unknown fields (via serde deny_unknown_fields)
//! - Computes deterministic digest: sha256(JCS(JSON(yaml)))

use super::schema::{PackDefinition, PackValidationError};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;

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
    let path = Path::new(reference);

    // 1. If path exists on filesystem → load as file
    if path.exists() {
        return load_pack_from_file(path);
    }

    // 2. Check built-in packs by name
    if let Some((builtin_name, content)) = get_builtin_pack_with_name(reference) {
        return load_pack_from_string(content, PackSource::BuiltIn(builtin_name));
    }

    // 3. Not found
    Err(PackError::NotFound {
        reference: reference.to_string(),
        suggestion: suggest_similar_pack(reference),
    })
}

/// Look up a built-in pack by name, returning both name and content.
fn get_builtin_pack_with_name(name: &str) -> Option<(&'static str, &'static str)> {
    super::BUILTIN_PACKS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(n, c)| (*n, *c))
}

/// Load multiple packs from references.
pub fn load_packs(references: &[String]) -> Result<Vec<LoadedPack>, PackError> {
    let mut packs = Vec::with_capacity(references.len());
    for reference in references {
        packs.push(load_pack(reference)?);
    }
    Ok(packs)
}

/// Load a pack from a file path.
pub fn load_pack_from_file(path: &Path) -> Result<LoadedPack, PackError> {
    let content = std::fs::read_to_string(path).map_err(|e| PackError::ReadError {
        path: path.to_path_buf(),
        source: e,
    })?;

    load_pack_from_string(&content, PackSource::File(path.to_path_buf()))
}

/// Load a pack from YAML string content.
fn load_pack_from_string(content: &str, source: PackSource) -> Result<LoadedPack, PackError> {
    // Parse YAML with strict settings
    let definition: PackDefinition =
        serde_yaml::from_str(content).map_err(|e| PackError::YamlParseError {
            message: format_yaml_error(e),
        })?;

    // Validate the pack
    definition.validate()?;

    // Check version compatibility
    check_version_compatibility(&definition)?;

    // Compute digest
    let digest = compute_pack_digest(&definition)?;

    Ok(LoadedPack {
        definition,
        digest,
        source,
    })
}

/// Compute pack digest: sha256(JCS(JSON(pack)))
fn compute_pack_digest(definition: &PackDefinition) -> Result<String, PackError> {
    // Serialize to JSON
    let json = serde_json::to_value(definition).map_err(|e| PackError::YamlParseError {
        message: format!("Failed to serialize pack to JSON: {}", e),
    })?;

    // Apply JCS canonicalization (RFC 8785)
    // For now, use serde_json's compact serialization which is close to JCS
    // TODO: Use proper JCS library when available
    let canonical = serde_json::to_string(&json).map_err(|e| PackError::YamlParseError {
        message: format!("Failed to canonicalize JSON: {}", e),
    })?;

    // Compute SHA-256
    let hash = Sha256::digest(canonical.as_bytes());
    Ok(format!("sha256:{}", hex::encode(hash)))
}

/// Check if current Assay version satisfies pack requirements.
fn check_version_compatibility(definition: &PackDefinition) -> Result<(), PackError> {
    let current_version = env!("CARGO_PKG_VERSION");
    let required = &definition.requires.assay_min_version;

    // Parse the required version constraint
    // Format: ">=X.Y.Z" or just "X.Y.Z"
    let required_version = required.trim_start_matches(">=").trim_start_matches("=");

    // Simple semver comparison (major.minor.patch)
    if !version_satisfies(current_version, required_version) {
        return Err(PackError::IncompatibleVersion {
            pack: definition.name.clone(),
            required: required.clone(),
            current: current_version.to_string(),
        });
    }

    Ok(())
}

/// Simple semver comparison (checks if current >= required).
fn version_satisfies(current: &str, required: &str) -> bool {
    let parse_version = |v: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() >= 3 {
            Some((
                parts[0].parse().ok()?,
                parts[1].parse().ok()?,
                parts[2].split('-').next()?.parse().ok()?,
            ))
        } else if parts.len() == 2 {
            Some((parts[0].parse().ok()?, parts[1].parse().ok()?, 0))
        } else {
            None
        }
    };

    match (parse_version(current), parse_version(required)) {
        (Some((c_major, c_minor, c_patch)), Some((r_major, r_minor, r_patch))) => {
            (c_major, c_minor, c_patch) >= (r_major, r_minor, r_patch)
        }
        _ => true, // If we can't parse, assume compatible
    }
}

/// Format YAML parsing error for user-friendly display.
fn format_yaml_error(e: serde_yaml::Error) -> String {
    let msg = e.to_string();

    // Check for duplicate key errors
    if msg.contains("duplicate key") {
        return format!(
            "Duplicate key detected (duplicate keys not allowed for security): {}",
            msg
        );
    }

    // Check for unknown field errors
    if msg.contains("unknown field") {
        return format!(
            "Unknown field detected (prevents digest bypass attacks): {}",
            msg
        );
    }

    msg
}

/// Suggest similar pack names.
fn suggest_similar_pack(reference: &str) -> String {
    use super::BUILTIN_PACKS;

    // Simple prefix matching for suggestions
    let suggestions: Vec<&str> = BUILTIN_PACKS
        .iter()
        .filter(|(name, _)| {
            name.starts_with(reference)
                || reference.starts_with(*name)
                || levenshtein_distance(name, reference) <= 3
        })
        .map(|(name, _)| *name)
        .collect();

    if suggestions.is_empty() {
        format!(
            "Available built-in packs: {}. Or specify a file path: --pack ./my-pack.yaml",
            BUILTIN_PACKS
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!("Did you mean '{}'?", suggestions.join("' or '"))
    }
}

/// Simple Levenshtein distance for fuzzy matching.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[m][n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_satisfies() {
        assert!(version_satisfies("2.9.0", "2.9.0"));
        assert!(version_satisfies("2.10.0", "2.9.0"));
        assert!(version_satisfies("3.0.0", "2.9.0"));
        assert!(!version_satisfies("2.8.0", "2.9.0"));
        assert!(!version_satisfies("2.9.0", "2.10.0"));
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("eu-ai-act", "eu-ai-act"), 0);
        assert_eq!(levenshtein_distance("eu-ai-act", "eu-ai-act-baseline"), 9);
        assert_eq!(levenshtein_distance("euaiact", "eu-ai-act"), 2);
    }
}
