//! Cache read-path boundary for Step2.x facade thinning.

use chrono::Utc;
use tokio::fs;
use tracing::{debug, warn};

use crate::error::{RegistryError, RegistryResult};
use crate::verify::compute_digest;

use super::super::{CacheEntry, CacheMeta, PackCache};

pub(crate) async fn get_impl(
    cache: &PackCache,
    name: &str,
    version: &str,
) -> RegistryResult<Option<CacheEntry>> {
    let pack_dir = cache.pack_dir(name, version);

    let pack_path = pack_dir.join("pack.yaml");
    let meta_path = pack_dir.join("metadata.json");

    if !pack_path.exists() || !meta_path.exists() {
        debug!(name, version, "pack not in cache");
        return Ok(None);
    }

    let meta_content = fs::read_to_string(&meta_path)
        .await
        .map_err(|e| RegistryError::Cache {
            message: format!("failed to read cache metadata: {}", e),
        })?;
    let metadata: CacheMeta =
        serde_json::from_str(&meta_content).map_err(|e| RegistryError::Cache {
            message: format!("failed to parse cache metadata: {}", e),
        })?;

    if metadata.expires_at < Utc::now() {
        debug!(
            name,
            version,
            expires_at = %metadata.expires_at,
            "cache entry expired"
        );
        return Ok(None);
    }

    let content = fs::read_to_string(&pack_path)
        .await
        .map_err(|e| RegistryError::Cache {
            message: format!("failed to read cached pack: {}", e),
        })?;

    let computed_digest = compute_digest(&content);
    if computed_digest != metadata.digest {
        warn!(
            name,
            version,
            expected = %metadata.digest,
            actual = %computed_digest,
            "cache integrity check failed"
        );
        return Err(RegistryError::DigestMismatch {
            name: name.to_string(),
            version: version.to_string(),
            expected: metadata.digest,
            actual: computed_digest,
        });
    }

    let sig_path = pack_dir.join("signature.json");
    let signature = if sig_path.exists() {
        let sig_content = fs::read_to_string(&sig_path).await.ok();
        sig_content.and_then(|s| serde_json::from_str(&s).ok())
    } else {
        None
    };

    debug!(name, version, "cache hit");
    Ok(Some(CacheEntry {
        content,
        metadata,
        signature,
    }))
}

pub(crate) async fn get_metadata_impl(
    cache: &PackCache,
    name: &str,
    version: &str,
) -> Option<CacheMeta> {
    let meta_path = cache.pack_dir(name, version).join("metadata.json");

    let content = fs::read_to_string(&meta_path).await.ok()?;
    serde_json::from_str(&content).ok()
}

pub(crate) async fn list_impl(
    cache: &PackCache,
) -> RegistryResult<Vec<(String, String, CacheMeta)>> {
    let mut result = Vec::new();

    if !cache.cache_dir.exists() {
        return Ok(result);
    }

    let mut names = fs::read_dir(&cache.cache_dir)
        .await
        .map_err(|e| RegistryError::Cache {
            message: format!("failed to read cache directory: {}", e),
        })?;

    while let Some(name_entry) = names.next_entry().await.map_err(|e| RegistryError::Cache {
        message: format!("failed to read directory entry: {}", e),
    })? {
        let name_path = name_entry.path();
        if !name_path.is_dir() {
            continue;
        }

        let name = name_entry.file_name().to_string_lossy().to_string();

        let mut versions = fs::read_dir(&name_path)
            .await
            .map_err(|e| RegistryError::Cache {
                message: format!("failed to read version directory: {}", e),
            })?;

        while let Some(version_entry) =
            versions
                .next_entry()
                .await
                .map_err(|e| RegistryError::Cache {
                    message: format!("failed to read directory entry: {}", e),
                })?
        {
            let version_path = version_entry.path();
            if !version_path.is_dir() {
                continue;
            }

            let version = version_entry.file_name().to_string_lossy().to_string();

            if let Some(meta) = get_metadata_impl(cache, &name, &version).await {
                result.push((name.clone(), version, meta));
            }
        }
    }

    Ok(result)
}
