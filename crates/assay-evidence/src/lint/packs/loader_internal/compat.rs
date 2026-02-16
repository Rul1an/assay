//! Version compatibility boundary for pack loading.
//!
//! Intended ownership (Commit B):
//! - compatibility checks and semver satisfaction helpers

use crate::lint::packs::loader::PackError;
use crate::lint::packs::schema::PackDefinition;

/// Check if current Assay version satisfies pack requirements.
pub(crate) fn check_version_compatibility_impl(
    definition: &PackDefinition,
) -> Result<(), PackError> {
    let current_version = env!("CARGO_PKG_VERSION");
    let required = &definition.requires.assay_min_version;

    // Parse the required version constraint
    // Format: ">=X.Y.Z" or just "X.Y.Z"
    let required_version = required.trim_start_matches(">=").trim_start_matches('=');

    // Simple semver comparison (major.minor.patch)
    if !version_satisfies_impl(current_version, required_version) {
        return Err(PackError::IncompatibleVersion {
            pack: definition.name.clone(),
            required: required.clone(),
            current: current_version.to_string(),
        });
    }

    Ok(())
}

/// Simple semver comparison (checks if current >= required).
pub(crate) fn version_satisfies_impl(current: &str, required: &str) -> bool {
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
        _ => false, // If we can't parse, treat as incompatible (fail closed)
    }
}
