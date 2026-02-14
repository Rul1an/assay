//! Cache eviction/cleanup boundary for Step2.x facade thinning.

use tokio::fs;
use tracing::debug;

use crate::error::{RegistryError, RegistryResult};

use super::super::PackCache;

pub(crate) async fn evict_impl(cache: &PackCache, name: &str, version: &str) -> RegistryResult<()> {
    let pack_dir = cache.pack_dir(name, version);

    if pack_dir.exists() {
        fs::remove_dir_all(&pack_dir)
            .await
            .map_err(|e| RegistryError::Cache {
                message: format!("failed to evict cache entry: {}", e),
            })?;
        debug!(name, version, "evicted from cache");
    }

    Ok(())
}

pub(crate) async fn clear_impl(cache: &PackCache) -> RegistryResult<()> {
    if cache.cache_dir.exists() {
        fs::remove_dir_all(&cache.cache_dir)
            .await
            .map_err(|e| RegistryError::Cache {
                message: format!("failed to clear cache: {}", e),
            })?;
        debug!("cleared pack cache");
    }
    Ok(())
}
