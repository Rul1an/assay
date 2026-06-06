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

use crate::error::RegistryResult;
use crate::types::{DsseEnvelope, FetchResult};

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
