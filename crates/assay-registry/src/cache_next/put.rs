//! Cache put-path boundary for Step2 split.

use chrono::Utc;
use tokio::fs;
use tracing::debug;

use crate::error::{RegistryError, RegistryResult};
use crate::types::FetchResult;

use super::super::{
    parse_cache_control_expiry, parse_signature, write_atomic, CacheMeta, PackCache,
};

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

    let expires_at = parse_cache_control_expiry(&result.headers);

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

    write_atomic(&pack_path, &result.content).await?;

    let meta_json = serde_json::to_string_pretty(&metadata).map_err(|e| RegistryError::Cache {
        message: format!("failed to serialize metadata: {}", e),
    })?;
    write_atomic(&meta_path, &meta_json).await?;

    if let Some(sig_b64) = &result.headers.signature {
        if let Ok(envelope) = parse_signature(sig_b64) {
            let sig_path = pack_dir.join("signature.json");
            let sig_json =
                serde_json::to_string_pretty(&envelope).map_err(|e| RegistryError::Cache {
                    message: format!("failed to serialize signature: {}", e),
                })?;
            write_atomic(&sig_path, &sig_json).await?;
        }
    }

    debug!(name, version, "cached pack");
    Ok(())
}
