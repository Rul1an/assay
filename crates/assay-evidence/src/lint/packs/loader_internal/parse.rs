//! Parsing boundary for pack loading.
//!
//! Intended ownership (Commit B):
//! - YAML decode and strict parse error shaping
//! - schema validation handoff

use super::compat;
use super::digest;
use crate::lint::packs::loader::{LoadedPack, PackError, PackSource};
use crate::lint::packs::schema::PackDefinition;

/// Load pack from string content.
///
/// Implements strict YAML parsing for the Pack Engine v1 spec:
/// - Rejects duplicate keys (partially via serde map handling)
/// - Rejects unknown fields (`deny_unknown_fields`)
///
/// Note: While the spec discourages anchors/aliases, the current implementation
/// accepts them if they resolve to valid JSON. Future versions may enforce failing on anchors/aliases.
pub(crate) fn load_pack_from_string_impl(
    content: &str,
    source: PackSource,
) -> Result<LoadedPack, PackError> {
    // Parse YAML with strict settings (deny_unknown_fields on schema types)
    let definition: PackDefinition =
        assay_common::doc::load_strict(content).map_err(|e| PackError::YamlParseError {
            message: format_yaml_error_impl(e),
        })?;

    if definition.requires.evidence_schema_version.is_none() {
        tracing::warn!(
            pack = %definition.name,
            "pack missing evidence_schema_version; assuming Assay Evidence Spec v1.0 for backward compatibility"
        );
    }

    // Validate the pack
    definition.validate()?;

    // Check version compatibility
    compat::check_version_compatibility_impl(&definition)?;

    // Compute digest
    let digest = digest::compute_pack_digest_impl(&definition)?;

    Ok(LoadedPack {
        definition,
        digest,
        source,
    })
}

/// Format YAML parsing error for user-friendly display.
fn format_yaml_error_impl(e: assay_common::doc::Refusal) -> String {
    use assay_common::doc::Refusal;
    let msg = match e {
        Refusal::Syntax { message, .. } | Refusal::Shape { message, .. } => message,
        Refusal::UnknownKeys(keys) => format!("unknown pack fields: {}", keys.join(", ")),
        Refusal::NotAMapping { path, expected } => format!("{path}: expected a {expected} mapping"),
        other => format!("invalid pack document: {other:?}"),
    };

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
