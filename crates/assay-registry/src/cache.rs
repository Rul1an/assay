//! Local cache layer for packs.
//!
//! Provides caching with integrity verification on read (TOCTOU protection).
//!
//! # Cache Structure
//!
//! ```text
//! ~/.assay/cache/packs/{name}/{version}/
//!   pack.yaml        # Pack content
//!   metadata.json    # Cache metadata
//!   signature.json   # DSSE envelope (optional)
//! ```

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, warn};

use crate::error::{RegistryError, RegistryResult};
use crate::types::{DsseEnvelope, FetchResult, PackHeaders};
use crate::verify::compute_digest;

/// Default cache TTL (24 hours).
const DEFAULT_TTL_SECS: i64 = 24 * 60 * 60;

/// Cache metadata stored alongside pack content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMeta {
    /// When the pack was fetched.
    pub fetched_at: DateTime<Utc>,

    /// Content digest (sha256:...).
    pub digest: String,

    /// ETag for conditional requests.
    #[serde(default)]
    pub etag: Option<String>,

    /// When the cache entry expires.
    pub expires_at: DateTime<Utc>,

    /// Key ID used to sign (if signed).
    #[serde(default)]
    pub key_id: Option<String>,

    /// Registry URL this was fetched from.
    #[serde(default)]
    pub registry_url: Option<String>,
}

/// Pack cache for storing and retrieving packs locally.
#[derive(Debug, Clone)]
pub struct PackCache {
    /// Base cache directory.
    cache_dir: PathBuf,
}

/// Cached pack entry.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Pack content.
    pub content: String,

    /// Cache metadata.
    pub metadata: CacheMeta,

    /// DSSE envelope (if signed).
    pub signature: Option<DsseEnvelope>,
}

impl PackCache {
    /// Create a new cache with default location.
    ///
    /// Default: `~/.assay/cache/packs`
    pub fn new() -> RegistryResult<Self> {
        let cache_dir = default_cache_dir()?;
        Ok(Self { cache_dir })
    }

    /// Create a cache with a custom directory.
    pub fn with_dir(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    /// Get the cache directory.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Get the path for a pack's cache directory.
    fn pack_dir(&self, name: &str, version: &str) -> PathBuf {
        self.cache_dir.join(name).join(version)
    }

    /// Get a cached pack, verifying integrity on read.
    ///
    /// Returns `None` if not cached or expired.
    /// Returns `Err` if integrity verification fails (caller should evict and re-fetch).
    pub async fn get(&self, name: &str, version: &str) -> RegistryResult<Option<CacheEntry>> {
        let pack_dir = self.pack_dir(name, version);

        // Check if pack exists
        let pack_path = pack_dir.join("pack.yaml");
        let meta_path = pack_dir.join("metadata.json");

        if !pack_path.exists() || !meta_path.exists() {
            debug!(name, version, "pack not in cache");
            return Ok(None);
        }

        // Read metadata first
        let meta_content =
            fs::read_to_string(&meta_path)
                .await
                .map_err(|e| RegistryError::Cache {
                    message: format!("failed to read cache metadata: {}", e),
                })?;
        let metadata: CacheMeta =
            serde_json::from_str(&meta_content).map_err(|e| RegistryError::Cache {
                message: format!("failed to parse cache metadata: {}", e),
            })?;

        // Check expiry
        if metadata.expires_at < Utc::now() {
            debug!(
                name,
                version,
                expires_at = %metadata.expires_at,
                "cache entry expired"
            );
            return Ok(None);
        }

        // Read content
        let content = fs::read_to_string(&pack_path)
            .await
            .map_err(|e| RegistryError::Cache {
                message: format!("failed to read cached pack: {}", e),
            })?;

        // CRITICAL: Verify digest BEFORE returning (TOCTOU protection)
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

        // Read signature if present
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

    /// Store a pack in the cache.
    pub async fn put(
        &self,
        name: &str,
        version: &str,
        result: &FetchResult,
        registry_url: Option<&str>,
    ) -> RegistryResult<()> {
        let pack_dir = self.pack_dir(name, version);

        // Create directory
        fs::create_dir_all(&pack_dir)
            .await
            .map_err(|e| RegistryError::Cache {
                message: format!("failed to create cache directory: {}", e),
            })?;

        // Calculate expiry from Cache-Control header
        let expires_at = parse_cache_control_expiry(&result.headers);

        // Build metadata
        let metadata = CacheMeta {
            fetched_at: Utc::now(),
            digest: result.computed_digest.clone(),
            etag: result.headers.etag.clone(),
            expires_at,
            key_id: result.headers.key_id.clone(),
            registry_url: registry_url.map(String::from),
        };

        // Write files atomically (write to temp, then rename)
        let pack_path = pack_dir.join("pack.yaml");
        let meta_path = pack_dir.join("metadata.json");

        // Write content
        write_atomic(&pack_path, &result.content).await?;

        // Write metadata
        let meta_json =
            serde_json::to_string_pretty(&metadata).map_err(|e| RegistryError::Cache {
                message: format!("failed to serialize metadata: {}", e),
            })?;
        write_atomic(&meta_path, &meta_json).await?;

        // Write signature if present
        if let Some(sig_b64) = &result.headers.signature {
            // Try to parse as DSSE envelope
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

    /// Get cached metadata without loading content.
    pub async fn get_metadata(&self, name: &str, version: &str) -> Option<CacheMeta> {
        let meta_path = self.pack_dir(name, version).join("metadata.json");

        let content = fs::read_to_string(&meta_path).await.ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Get the ETag for conditional requests.
    pub async fn get_etag(&self, name: &str, version: &str) -> Option<String> {
        self.get_metadata(name, version).await.and_then(|m| m.etag)
    }

    /// Check if a pack is cached and not expired.
    pub async fn is_cached(&self, name: &str, version: &str) -> bool {
        match self.get_metadata(name, version).await {
            Some(meta) => meta.expires_at >= Utc::now(),
            None => false,
        }
    }

    /// Evict a pack from the cache.
    pub async fn evict(&self, name: &str, version: &str) -> RegistryResult<()> {
        let pack_dir = self.pack_dir(name, version);

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

    /// Clear all cached packs.
    pub async fn clear(&self) -> RegistryResult<()> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir)
                .await
                .map_err(|e| RegistryError::Cache {
                    message: format!("failed to clear cache: {}", e),
                })?;
            debug!("cleared pack cache");
        }
        Ok(())
    }

    /// List all cached packs.
    pub async fn list(&self) -> RegistryResult<Vec<(String, String, CacheMeta)>> {
        let mut result = Vec::new();

        if !self.cache_dir.exists() {
            return Ok(result);
        }

        let mut names = fs::read_dir(&self.cache_dir)
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

            let mut versions =
                fs::read_dir(&name_path)
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

                if let Some(meta) = self.get_metadata(&name, &version).await {
                    result.push((name.clone(), version, meta));
                }
            }
        }

        Ok(result)
    }
}

impl Default for PackCache {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self::with_dir("/tmp/assay-cache/packs"))
    }
}

/// Get the default cache directory.
fn default_cache_dir() -> RegistryResult<PathBuf> {
    let base = dirs::cache_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| RegistryError::Cache {
            message: "could not determine cache directory".to_string(),
        })?;

    Ok(base.join("assay").join("cache").join("packs"))
}

/// Parse Cache-Control header to determine expiry.
fn parse_cache_control_expiry(headers: &PackHeaders) -> DateTime<Utc> {
    let now = Utc::now();
    let default_ttl = Duration::seconds(DEFAULT_TTL_SECS);

    let ttl = headers
        .cache_control
        .as_ref()
        .and_then(|cc| {
            // Parse max-age=N
            cc.split(',')
                .find(|part| part.trim().starts_with("max-age="))
                .and_then(|part| {
                    part.trim()
                        .strip_prefix("max-age=")
                        .and_then(|v| v.parse::<i64>().ok())
                })
        })
        .map(Duration::seconds)
        .unwrap_or(default_ttl);

    now + ttl
}

/// Parse signature from Base64.
fn parse_signature(b64: &str) -> RegistryResult<DsseEnvelope> {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    let bytes = BASE64.decode(b64).map_err(|e| RegistryError::Cache {
        message: format!("invalid base64 signature: {}", e),
    })?;

    serde_json::from_slice(&bytes).map_err(|e| RegistryError::Cache {
        message: format!("invalid DSSE envelope: {}", e),
    })
}

/// Write content to file atomically.
async fn write_atomic(path: &Path, content: &str) -> RegistryResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use tempfile::TempDir;

    fn create_test_cache() -> (PackCache, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let cache = PackCache::with_dir(temp_dir.path().join("cache"));
        (cache, temp_dir)
    }

    fn create_fetch_result(content: &str) -> FetchResult {
        FetchResult {
            content: content.to_string(),
            headers: PackHeaders {
                digest: Some(compute_digest(content)),
                signature: None,
                key_id: None,
                etag: Some("\"abc123\"".to_string()),
                cache_control: Some("max-age=3600".to_string()),
                content_length: Some(content.len() as u64),
            },
            computed_digest: compute_digest(content),
        }
    }

    #[tokio::test]
    async fn test_cache_roundtrip() {
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: 1.0.0";
        let result = create_fetch_result(content);

        // Put
        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();

        // Get
        let entry = cache.get("test-pack", "1.0.0").await.unwrap().unwrap();
        assert_eq!(entry.content, content);
        assert_eq!(entry.metadata.digest, compute_digest(content));
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let (cache, _temp_dir) = create_test_cache();

        let result = cache.get("nonexistent", "1.0.0").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_integrity_failure() {
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: 1.0.0";
        let result = create_fetch_result(content);

        // Put
        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();

        // Corrupt the cached file
        let pack_path = cache.pack_dir("test-pack", "1.0.0").join("pack.yaml");
        fs::write(&pack_path, "corrupted content").await.unwrap();

        // Get should fail integrity check
        let err = cache.get("test-pack", "1.0.0").await.unwrap_err();
        assert!(matches!(err, RegistryError::DigestMismatch { .. }));
    }

    #[tokio::test]
    async fn test_cache_expiry() {
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: 1.0.0";
        let result = FetchResult {
            content: content.to_string(),
            headers: PackHeaders {
                digest: Some(compute_digest(content)),
                signature: None,
                key_id: None,
                etag: None,
                cache_control: Some("max-age=0".to_string()), // Expire immediately
                content_length: None,
            },
            computed_digest: compute_digest(content),
        };

        // Put
        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();

        // Wait a moment
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Get should return None (expired)
        let entry = cache.get("test-pack", "1.0.0").await.unwrap();
        assert!(entry.is_none());
    }

    #[tokio::test]
    async fn test_cache_evict() {
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: 1.0.0";
        let result = create_fetch_result(content);

        // Put
        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();
        assert!(cache.is_cached("test-pack", "1.0.0").await);

        // Evict
        cache.evict("test-pack", "1.0.0").await.unwrap();
        assert!(!cache.is_cached("test-pack", "1.0.0").await);
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: 1.0.0";
        let result = create_fetch_result(content);

        // Put multiple packs
        cache.put("pack1", "1.0.0", &result, None).await.unwrap();
        cache.put("pack2", "1.0.0", &result, None).await.unwrap();

        // Clear
        cache.clear().await.unwrap();

        // Both should be gone
        assert!(!cache.is_cached("pack1", "1.0.0").await);
        assert!(!cache.is_cached("pack2", "1.0.0").await);
    }

    #[tokio::test]
    async fn test_cache_list() {
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: 1.0.0";
        let result = create_fetch_result(content);

        // Put multiple packs
        cache.put("pack1", "1.0.0", &result, None).await.unwrap();
        cache.put("pack1", "2.0.0", &result, None).await.unwrap();
        cache.put("pack2", "1.0.0", &result, None).await.unwrap();

        // List
        let entries = cache.list().await.unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[tokio::test]
    async fn test_get_etag() {
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: 1.0.0";
        let result = create_fetch_result(content);

        // Put
        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();

        // Get ETag
        let etag = cache.get_etag("test-pack", "1.0.0").await;
        assert_eq!(etag, Some("\"abc123\"".to_string()));
    }

    #[tokio::test]
    async fn test_parse_cache_control() {
        let headers = PackHeaders {
            digest: None,
            signature: None,
            key_id: None,
            etag: None,
            cache_control: Some("max-age=7200, public".to_string()),
            content_length: None,
        };

        let expires = parse_cache_control_expiry(&headers);
        let now = Utc::now();

        // Should be approximately 2 hours in the future
        let diff = expires - now;
        assert!(diff.num_seconds() >= 7190 && diff.num_seconds() <= 7210);
    }

    #[tokio::test]
    async fn test_default_ttl() {
        let headers = PackHeaders {
            digest: None,
            signature: None,
            key_id: None,
            etag: None,
            cache_control: None, // No Cache-Control
            content_length: None,
        };

        let expires = parse_cache_control_expiry(&headers);
        let now = Utc::now();

        // Should be approximately 24 hours in the future
        let diff = expires - now;
        assert!(diff.num_hours() >= 23 && diff.num_hours() <= 25);
    }

    #[tokio::test]
    async fn test_cache_with_signature() {
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: 1.0.0";

        // Create a mock DSSE envelope
        let envelope = DsseEnvelope {
            payload_type: "application/vnd.assay.pack+yaml;v=1".to_string(),
            payload: base64::engine::general_purpose::STANDARD.encode(content),
            signatures: vec![],
        };
        let envelope_json = serde_json::to_vec(&envelope).unwrap();
        let envelope_b64 = base64::engine::general_purpose::STANDARD.encode(&envelope_json);

        let result = FetchResult {
            content: content.to_string(),
            headers: PackHeaders {
                digest: Some(compute_digest(content)),
                signature: Some(envelope_b64),
                key_id: Some("sha256:test-key".to_string()),
                etag: None,
                cache_control: Some("max-age=3600".to_string()),
                content_length: None,
            },
            computed_digest: compute_digest(content),
        };

        // Put
        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();

        // Get
        let entry = cache.get("test-pack", "1.0.0").await.unwrap().unwrap();
        assert!(entry.signature.is_some());
        assert_eq!(
            entry.signature.unwrap().payload_type,
            "application/vnd.assay.pack+yaml;v=1"
        );
    }
}
