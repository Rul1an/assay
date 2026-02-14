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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use tokio::fs;

#[cfg(test)]
use crate::error::RegistryError;
use crate::error::RegistryResult;
#[cfg(test)]
use crate::types::PackHeaders;
use crate::types::{DsseEnvelope, FetchResult};
#[cfg(test)]
use crate::verify::compute_digest;

#[path = "cache_next/mod.rs"]
mod cache_next;

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
        let cache_dir = cache_next::io::default_cache_dir_impl()?;
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
        cache_next::keys::pack_dir_impl(&self.cache_dir, name, version)
    }

    /// Get a cached pack, verifying integrity on read.
    ///
    /// Returns `None` if not cached or expired.
    /// Returns `Err` if integrity verification fails (caller should evict and re-fetch).
    pub async fn get(&self, name: &str, version: &str) -> RegistryResult<Option<CacheEntry>> {
        cache_next::read::get_impl(self, name, version).await
    }

    /// Store a pack in the cache.
    pub async fn put(
        &self,
        name: &str,
        version: &str,
        result: &FetchResult,
        registry_url: Option<&str>,
    ) -> RegistryResult<()> {
        cache_next::put::put_impl(self, name, version, result, registry_url).await
    }

    /// Get cached metadata without loading content.
    pub async fn get_metadata(&self, name: &str, version: &str) -> Option<CacheMeta> {
        cache_next::read::get_metadata_impl(self, name, version).await
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
        cache_next::evict::evict_impl(self, name, version).await
    }

    /// Clear all cached packs.
    pub async fn clear(&self) -> RegistryResult<()> {
        cache_next::evict::clear_impl(self).await
    }

    /// List all cached packs.
    pub async fn list(&self) -> RegistryResult<Vec<(String, String, CacheMeta)>> {
        cache_next::read::list_impl(self).await
    }
}

impl Default for PackCache {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self::with_dir("/tmp/assay-cache/packs"))
    }
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

        let expires =
            cache_next::policy::parse_cache_control_expiry_impl(&headers, DEFAULT_TTL_SECS);
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

        let expires =
            cache_next::policy::parse_cache_control_expiry_impl(&headers, DEFAULT_TTL_SECS);
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

    // ==================== Cache Robustness Tests (SPEC §7.2) ====================

    #[tokio::test]
    async fn test_pack_yaml_corrupt_evict_refetch() {
        // SPEC §7.2: Corrupted cache entry should be detected and evictable
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: \"1.0.0\"";
        let result = create_fetch_result(content);

        // Put valid content
        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();

        // Verify it works
        let entry = cache.get("test-pack", "1.0.0").await.unwrap();
        assert!(entry.is_some());

        // Corrupt the cached file
        let pack_path = cache.pack_dir("test-pack", "1.0.0").join("pack.yaml");
        fs::write(&pack_path, "corrupted: content\nmalicious: true")
            .await
            .unwrap();

        // Get should fail with DigestMismatch
        let err = cache.get("test-pack", "1.0.0").await.unwrap_err();
        assert!(
            matches!(err, RegistryError::DigestMismatch { .. }),
            "Should detect corruption: {:?}",
            err
        );

        // Evict the corrupted entry
        cache.evict("test-pack", "1.0.0").await.unwrap();

        // Now cache should be empty
        let entry = cache.get("test-pack", "1.0.0").await.unwrap();
        assert!(entry.is_none(), "Cache should be empty after evict");
    }

    #[tokio::test]
    async fn test_signature_json_corrupt_handling() {
        // SPEC §7.2: Corrupted signature.json should not crash, signature becomes None
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: \"1.0.0\"";

        // Create with valid signature
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

        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();

        // Verify signature exists
        let entry = cache.get("test-pack", "1.0.0").await.unwrap().unwrap();
        assert!(entry.signature.is_some());

        // Corrupt the signature file
        let sig_path = cache.pack_dir("test-pack", "1.0.0").join("signature.json");
        fs::write(&sig_path, "this is not valid json{{{")
            .await
            .unwrap();

        // Get should still work, but signature is None (graceful degradation)
        let entry = cache.get("test-pack", "1.0.0").await.unwrap().unwrap();
        assert!(
            entry.signature.is_none(),
            "Corrupt signature should be None, not error"
        );
        // Content should still be valid
        assert_eq!(entry.content, content);
    }

    #[tokio::test]
    async fn test_metadata_json_corrupt_handling() {
        // SPEC §7.2: Corrupted metadata.json should return cache miss
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: \"1.0.0\"";
        let result = create_fetch_result(content);

        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();

        // Corrupt the metadata file
        let meta_path = cache.pack_dir("test-pack", "1.0.0").join("metadata.json");
        fs::write(&meta_path, "invalid json content").await.unwrap();

        // Get should fail with cache error (not crash)
        let result = cache.get("test-pack", "1.0.0").await;
        assert!(
            matches!(result, Err(RegistryError::Cache { .. })),
            "Should return cache error for corrupt metadata: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_atomic_write_prevents_partial_cache() {
        // SPEC §7.2: Atomic writes prevent partial/corrupt cache entries
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: \"1.0.0\"";
        let result = create_fetch_result(content);

        // After put, no .tmp files should exist
        cache
            .put("test-pack", "1.0.0", &result, None)
            .await
            .unwrap();

        let pack_dir = cache.pack_dir("test-pack", "1.0.0");

        // Check no temp files remain
        let mut entries = fs::read_dir(&pack_dir).await.unwrap();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            assert!(
                !name_str.ends_with(".tmp"),
                "Temp file should not remain: {}",
                name_str
            );
        }
    }

    #[tokio::test]
    async fn test_cache_registry_url_tracking() {
        // SPEC §7.1: Cache should track which registry pack came from
        let (cache, _temp_dir) = create_test_cache();
        let content = "name: test\nversion: \"1.0.0\"";
        let result = create_fetch_result(content);

        cache
            .put(
                "test-pack",
                "1.0.0",
                &result,
                Some("https://registry.example.com/v1"),
            )
            .await
            .unwrap();

        let meta = cache.get_metadata("test-pack", "1.0.0").await.unwrap();
        assert_eq!(
            meta.registry_url,
            Some("https://registry.example.com/v1".to_string())
        );
    }
}
