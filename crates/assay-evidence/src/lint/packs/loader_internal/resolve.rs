//! Source resolution boundary for pack loading.
//!
//! Intended ownership (Commit B):
//! - reference resolution order (path > builtin > local config dir)
//! - pack name validation and local path discovery

use crate::lint::packs::loader::{LoadedPack, PackError};
use crate::lint::packs::schema::PackValidationError;
use std::path::PathBuf;

/// Look up a built-in pack by name, returning both name and content.
pub(crate) fn get_builtin_pack_with_name_impl(name: &str) -> Option<(&'static str, &'static str)> {
    super::super::super::BUILTIN_PACKS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(n, c)| (*n, *c))
}

/// Try to load a pack from the local config directory.
pub(crate) fn try_load_from_config_dir_impl(name: &str) -> Result<Option<LoadedPack>, PackError> {
    let config_dir = match get_config_pack_dir_impl() {
        Some(d) => d,
        None => return Ok(None),
    };

    if !config_dir.exists() {
        return Ok(None);
    }

    // Harden canonicalization: if dir exists, must canonicalize or error
    let canonical_config =
        std::fs::canonicalize(&config_dir).map_err(|e| PackError::ReadError {
            path: config_dir.clone(),
            source: e,
        })?;

    // Candidates:
    // 1. {config_dir}/{name}.yaml
    // 2. {config_dir}/{name}/pack.yaml
    let candidates = vec![
        config_dir.join(format!("{}.yaml", name)),
        config_dir.join(name).join("pack.yaml"),
    ];

    for path in candidates {
        if path.exists() {
            // SECURITY: Path containment check
            // Use canonicalize() to resolve symlinks and '..'
            let canonical_path =
                std::fs::canonicalize(&path).map_err(|e| PackError::ReadError {
                    path: path.clone(),
                    source: e,
                })?;

            if !canonical_path.starts_with(&canonical_config) {
                // Determine if this is a symlink escape attempt
                return Err(PackValidationError::Safety(format!(
                    "Pack path '{}' resolves outside config directory '{}'",
                    path.display(),
                    config_dir.display()
                ))
                .into());
            }

            // Mitigate TOCTOU: load from the canonical path we just verified
            return Ok(Some(super::run::load_pack_from_file_impl(&canonical_path)?));
        }
    }

    Ok(None)
}

/// Determine the config pack directory per ADR-021.
pub(crate) fn get_config_pack_dir_impl() -> Option<PathBuf> {
    // Unix: $XDG_CONFIG_HOME/assay/packs or ~/.config/assay/packs
    // Windows: %APPDATA%\assay\packs

    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg).join("assay").join("packs"));
            }
        }

        // Fallback: ~/.config/assay/packs
        if let Ok(home) = std::env::var("HOME") {
            return Some(
                PathBuf::from(home)
                    .join(".config")
                    .join("assay")
                    .join("packs"),
            );
        }
    }

    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(PathBuf::from(appdata).join("assay").join("packs"));
        }
    }

    None
}

/// Validate pack name grammar per ADR-021.
///
/// Chars: a-z, 0-9, -
/// Must not start or end with hyphen.
pub(crate) fn is_valid_pack_name_impl(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Suggest similar pack names.
pub(crate) fn suggest_similar_pack_impl(reference: &str) -> String {
    // Simple prefix matching for suggestions
    let suggestions: Vec<&str> = super::super::super::BUILTIN_PACKS
        .iter()
        .filter(|(name, _)| {
            name.starts_with(reference)
                || reference.starts_with(*name)
                || levenshtein_distance_impl(name, reference) <= 3
        })
        .map(|(name, _)| *name)
        .collect();

    if suggestions.is_empty() {
        format!(
            "Available built-in packs: {}. Or specify a file path: --pack ./my-pack.yaml",
            super::super::super::BUILTIN_PACKS
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
#[allow(clippy::needless_range_loop)]
pub(crate) fn levenshtein_distance_impl(a: &str, b: &str) -> usize {
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
