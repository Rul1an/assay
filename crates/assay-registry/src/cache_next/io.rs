//! IO boundary scaffold for cache split.
//!
//! Planned ownership (Step2+):
//! - atomic write/rename helpers
//! - filesystem interactions for cache content

use std::path::{Path, PathBuf};

use tokio::fs;

use crate::error::{RegistryError, RegistryResult};

pub(crate) fn default_cache_dir_impl() -> RegistryResult<PathBuf> {
    let base = dirs::cache_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| RegistryError::Cache {
            message: "could not determine cache directory".to_string(),
        })?;

    Ok(base.join("assay").join("cache").join("packs"))
}

pub(crate) async fn write_atomic_impl(path: &Path, content: &str) -> RegistryResult<()> {
    let temp_path = path.with_extension("tmp");

    fs::write(&temp_path, content)
        .await
        .map_err(|e| RegistryError::Cache {
            message: format!("failed to write temp file: {}", e),
        })?;

    fs::rename(&temp_path, path)
        .await
        .map_err(|e| RegistryError::Cache {
            message: format!("failed to rename temp file: {}", e),
        })?;

    Ok(())
}
