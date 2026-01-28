//! Bundle storage abstraction for BYOS (Bring Your Own Storage).
//!
//! This module provides a clean adapter layer between evidence bundles and
//! S3-compatible object storage. No platform, no DB, no server—just object
//! storage + naming conventions.
//!
//! # Design Principles
//!
//! 1. **Pure adapter**: Upload/download/list of `.tar.gz` bundles only
//! 2. **Immutability-safe**: Conditional writes prevent silent overwrites
//! 3. **Content-addressed**: `bundle_id` (SHA-256) is the source of truth, not ETags
//! 4. **Testable**: Works with in-memory backend for unit tests
//!
//! # Key Schema
//!
//! ```text
//! bundles/{bundle_id}/bundle.tar.gz     # The bundle itself
//! runs/{run_id}/bundles/{bundle_id}.ref # Run-to-bundle index (for list --run-id)
//! ```

pub mod error;
pub mod naming;
pub mod object_store_backend;

use async_trait::async_trait;
use bytes::Bytes;

pub use error::{StoreError, StoreResult};
pub use naming::KeyBuilder;
pub use object_store_backend::ObjectStoreBundleStore;

/// Parsed store specification from CLI/config.
///
/// # Examples
///
/// ```text
/// s3://my-bucket/assay/evidence
/// file:///tmp/assay-store
/// memory://  (for testing)
/// ```
#[derive(Debug, Clone)]
pub struct StoreSpec {
    /// The scheme (s3, file, memory, az, gcs)
    pub scheme: String,
    /// Bucket or container name (empty for file://)
    pub bucket: Option<String>,
    /// Base prefix/path within the bucket
    pub prefix: String,
    /// Optional region (for S3)
    pub region: Option<String>,
}

impl StoreSpec {
    /// Parse a store URL like `s3://bucket/prefix` or `file:///path`.
    pub fn parse(url: &str) -> StoreResult<Self> {
        let url = url::Url::parse(url).map_err(|e| StoreError::InvalidSpec {
            spec: url.to_string(),
            reason: e.to_string(),
        })?;

        let scheme = url.scheme().to_string();
        let bucket = url.host_str().map(|s| s.to_string());
        let prefix = url.path().trim_start_matches('/').to_string();

        // Extract region from query params if present
        let region = url
            .query_pairs()
            .find(|(k, _)| k == "region")
            .map(|(_, v)| v.to_string());

        Ok(Self {
            scheme,
            bucket,
            prefix,
            region,
        })
    }

    /// Check if this is a memory store (for testing).
    pub fn is_memory(&self) -> bool {
        self.scheme == "memory"
    }

    /// Check if this is a local file store.
    pub fn is_file(&self) -> bool {
        self.scheme == "file"
    }
}

/// Metadata about a stored bundle.
#[derive(Debug, Clone)]
pub struct BundleMeta {
    /// The content-addressed bundle ID (sha256:...)
    pub bundle_id: String,
    /// Size in bytes (if known)
    pub size: Option<u64>,
    /// Last modified timestamp (if known)
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
}

/// Reference linking a run to a bundle.
#[derive(Debug, Clone)]
pub struct RunBundleRef {
    pub run_id: String,
    pub bundle_id: String,
}

/// The core bundle storage trait.
///
/// Implementations handle the actual I/O to S3-compatible stores.
/// All operations are async for compatibility with object stores.
///
/// # Immutability
///
/// `put_bundle` uses conditional writes (If-None-Match) to prevent
/// overwriting existing bundles. This is critical for audit trails.
#[async_trait]
pub trait BundleStore: Send + Sync {
    /// Upload a bundle. Uses conditional write to prevent overwrites.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if uploaded successfully
    /// - `Err(StoreError::AlreadyExists)` if bundle already exists (idempotent)
    /// - `Err(StoreError::...)` for other errors
    async fn put_bundle(&self, bundle_id: &str, bytes: Bytes) -> StoreResult<()>;

    /// Download a bundle by ID.
    ///
    /// # Returns
    ///
    /// - `Ok(Bytes)` with the bundle contents
    /// - `Err(StoreError::NotFound)` if bundle doesn't exist
    async fn get_bundle(&self, bundle_id: &str) -> StoreResult<Bytes>;

    /// Check if a bundle exists.
    async fn bundle_exists(&self, bundle_id: &str) -> StoreResult<bool>;

    /// Link a bundle to a run ID (for `list --run-id`).
    ///
    /// Creates a small reference object under `runs/{run_id}/bundles/`.
    /// Idempotent: linking the same bundle twice is a no-op.
    async fn link_run_bundle(&self, run_id: &str, bundle_id: &str) -> StoreResult<()>;

    /// List all bundle IDs linked to a run.
    ///
    /// # Returns
    ///
    /// Vector of bundle IDs in no guaranteed order.
    async fn list_bundles_for_run(&self, run_id: &str) -> StoreResult<Vec<String>>;

    /// List all bundle IDs (optionally filtered by prefix).
    ///
    /// # Arguments
    ///
    /// - `prefix`: Optional prefix filter (e.g., "sha256:abc")
    /// - `limit`: Maximum number of results (default: 1000)
    ///
    /// # Note
    ///
    /// This is a convenience operation. For authoritative listings,
    /// use `list_bundles_for_run` with explicit run IDs.
    async fn list_bundles(
        &self,
        prefix: Option<&str>,
        limit: Option<usize>,
    ) -> StoreResult<Vec<BundleMeta>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_s3_spec() {
        let spec = StoreSpec::parse("s3://my-bucket/assay/evidence").unwrap();
        assert_eq!(spec.scheme, "s3");
        assert_eq!(spec.bucket, Some("my-bucket".to_string()));
        assert_eq!(spec.prefix, "assay/evidence");
    }

    #[test]
    fn test_parse_s3_with_region() {
        let spec = StoreSpec::parse("s3://my-bucket/prefix?region=us-west-2").unwrap();
        assert_eq!(spec.region, Some("us-west-2".to_string()));
    }

    #[test]
    fn test_parse_file_spec() {
        let spec = StoreSpec::parse("file:///tmp/assay-store").unwrap();
        assert_eq!(spec.scheme, "file");
        assert!(spec.bucket.is_none());
        assert_eq!(spec.prefix, "tmp/assay-store");
        assert!(spec.is_file());
    }

    #[test]
    fn test_parse_memory_spec() {
        let spec = StoreSpec::parse("memory://test").unwrap();
        assert!(spec.is_memory());
    }
}
