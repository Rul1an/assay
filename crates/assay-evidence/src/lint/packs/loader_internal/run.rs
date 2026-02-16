//! Orchestration boundary for pack loading.

use super::parse;
use super::resolve;
use crate::lint::packs::loader::{LoadedPack, PackError, PackSource};
use std::path::Path;

/// Load a pack from a reference (file path or built-in name).
pub(crate) fn load_pack_impl(reference: &str) -> Result<LoadedPack, PackError> {
    let path = Path::new(reference);

    // 1. If path exists on filesystem -> load as file or dir
    if path.exists() {
        if path.is_dir() {
            let pack_yaml = path.join("pack.yaml");
            if pack_yaml.exists() {
                return load_pack_from_file_impl(&pack_yaml);
            }
            return Err(PackError::ReadError {
                path: pack_yaml,
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Directory provided but 'pack.yaml' not found",
                ),
            });
        }
        return load_pack_from_file_impl(path);
    }

    // 2. Check built-in packs by name
    if let Some((builtin_name, content)) = resolve::get_builtin_pack_with_name_impl(reference) {
        return parse::load_pack_from_string_impl(content, PackSource::BuiltIn(builtin_name));
    }

    // 3. Local pack directory (valid name only; containment enforced in load)
    if resolve::is_valid_pack_name_impl(reference) {
        if let Some(loaded) = resolve::try_load_from_config_dir_impl(reference)? {
            return Ok(loaded);
        }
    }

    // 4. Registry / BYOS (future)

    // 5. Not found
    Err(PackError::NotFound {
        reference: reference.to_string(),
        suggestion: resolve::suggest_similar_pack_impl(reference),
    })
}

/// Load multiple packs from references.
pub(crate) fn load_packs_impl(references: &[String]) -> Result<Vec<LoadedPack>, PackError> {
    let mut packs = Vec::with_capacity(references.len());
    for reference in references {
        packs.push(load_pack_impl(reference)?);
    }
    Ok(packs)
}

/// Load a pack from a file path.
pub(crate) fn load_pack_from_file_impl(path: &Path) -> Result<LoadedPack, PackError> {
    let content = std::fs::read_to_string(path).map_err(|e| PackError::ReadError {
        path: path.to_path_buf(),
        source: e,
    })?;

    parse::load_pack_from_string_impl(&content, PackSource::File(path.to_path_buf()))
}
