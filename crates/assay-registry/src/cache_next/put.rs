//! Cache put-path boundary for Step2 split.

use chrono::Utc;
use tokio::fs;
use tracing::debug;

use crate::error::{RegistryError, RegistryResult};
use crate::types::FetchResult;

use super::super::{CacheMeta, PackCache, DEFAULT_TTL_SECS};
use super::{integrity, io, policy};

pub(crate) async fn put_impl(
    cache: &PackCache,
    name: &str,
    version: &str,
    result: &FetchResult,
    registry_url: Option<&str>,
) -> RegistryResult<()> {
    let pack_dir = cache.pack_dir(name, version);

    fs::create_dir_all(&pack_dir)
        .await
        .map_err(|e| RegistryError::Cache {
            message: format!("failed to create cache directory: {}", e),
        })?;

    let expires_at = policy::parse_cache_control_expiry_impl(&result.headers, DEFAULT_TTL_SECS);

    let metadata = CacheMeta {
        fetched_at: Utc::now(),
        digest: result.computed_digest.clone(),
        etag: result.headers.etag.clone(),
        expires_at,
        key_id: result.headers.key_id.clone(),
        registry_url: registry_url.map(String::from),
    };

    let pack_path = pack_dir.join("pack.yaml");
    let meta_path = pack_dir.join("metadata.json");

    io::write_atomic_impl(&pack_path, &result.content).await?;

    let meta_json = serde_json::to_string_pretty(&metadata).map_err(|e| RegistryError::Cache {
        message: format!("failed to serialize metadata: {}", e),
    })?;
    io::write_atomic_impl(&meta_path, &meta_json).await?;

    if let Some(sig_b64) = &result.headers.signature {
        if let Ok(envelope) = integrity::parse_signature_impl(sig_b64) {
            let sig_path = pack_dir.join("signature.json");
            let sig_json =
                serde_json::to_string_pretty(&envelope).map_err(|e| RegistryError::Cache {
                    message: format!("failed to serialize signature: {}", e),
                })?;
            io::write_atomic_impl(&sig_path, &sig_json).await?;
        }
    }

    debug!(name, version, "cached pack");
    Ok(())
}
