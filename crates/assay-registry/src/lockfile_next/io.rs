//! Lockfile IO boundary for Step2 split.

use std::path::Path;

use tokio::{fs, io::AsyncReadExt};
use tracing::info;

use crate::error::{RegistryError, RegistryResult};

use super::super::Lockfile;

pub(crate) async fn load_impl(path: impl AsRef<Path>) -> RegistryResult<Lockfile> {
    let path = path.as_ref();

    if !path.exists() {
        return Err(RegistryError::Lockfile {
            message: format!("lockfile not found: {}", path.display()),
        });
    }

    let read_error = |e: std::io::Error| RegistryError::Lockfile {
        message: format!("failed to read lockfile: {e}"),
    };
    let file = fs::File::open(path).await.map_err(read_error)?;
    let mut bytes = Vec::new();
    file.take(super::MAX_LOCKFILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(read_error)?;
    super::check_size(bytes.len())?;
    let content = std::str::from_utf8(&bytes).map_err(|e| RegistryError::Lockfile {
        message: format!("failed to read lockfile: {e}"),
    })?;
    Lockfile::parse(content)
}

pub(crate) async fn save_impl(lockfile: &Lockfile, path: impl AsRef<Path>) -> RegistryResult<()> {
    let path = path.as_ref();
    let content = lockfile.to_yaml()?;

    fs::write(path, content)
        .await
        .map_err(|e| RegistryError::Lockfile {
            message: format!("failed to write lockfile: {}", e),
        })?;

    info!(path = %path.display(), "saved lockfile");
    Ok(())
}
