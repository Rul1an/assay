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

mod bounded;
pub mod config;
pub mod error;
pub mod naming;
pub mod object_store_backend;

use async_trait::async_trait;
use bytes::Bytes;
use serde::Serialize;

pub use bounded::{BoundedGetError, StreamCeiling};
pub use error::{StoreError, StoreResult};
pub use naming::KeyBuilder;
pub use object_store_backend::ObjectStoreBundleStore;

/// Diagnostic status of a connected evidence store.
///
/// `bundle_count` and `total_size_bytes` are capped at 10,000 entries for
/// responsiveness. Stores with more bundles will show approximate values.
#[derive(Debug, Clone, Serialize)]
pub struct StoreStatus {
    pub reachable: bool,
    pub readable: bool,
    pub writable: bool,
    pub backend: String,
    pub bucket: Option<String>,
    pub prefix: String,
    pub bundle_count: u64,
    pub total_size_bytes: u64,
    /// Best-effort Object Lock detection: `"unknown"`, `"enabled"`, or `"disabled"`.
    pub object_lock: String,
}

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

const FILE_STORE_ROOT_REFUSAL_REASON: &str = "file store URL must include an explicit non-root local filesystem path (for example file:///tmp/assay-store); filesystem root '/' is not allowed";

fn is_filesystem_root_path(path: &std::path::Path) -> bool {
    use std::path::Component;

    let mut saw_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => saw_root = true,
            Component::CurDir => {}
            Component::Normal(_) | Component::ParentDir => return false,
        }
    }
    saw_root
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
        let prefix = if scheme == "file" {
            let path = url.to_file_path().map_err(|_| StoreError::InvalidSpec {
                spec: url.to_string(),
                reason: "file URL is not a valid local filesystem path".to_string(),
            })?;
            if is_filesystem_root_path(&path) {
                return Err(StoreError::InvalidSpec {
                    spec: url.to_string(),
                    reason: FILE_STORE_ROOT_REFUSAL_REASON.to_string(),
                });
            }
            path.display().to_string()
        } else {
            url.path().trim_start_matches('/').to_string()
        };

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
    /// The `bundles/` prefix this lists is the canonical set. `list_bundles_for_run` reads the
    /// `runs/` index, which is derived from the bundles' manifests and is not authoritative:
    /// a `.ref` can be missing or stale without any canonical object changing.
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
    fn test_parse_core_specs_regression() {
        let file_url = "file:///tmp/assay-store";
        let expected_file_prefix = url::Url::parse(file_url)
            .unwrap()
            .to_file_path()
            .unwrap()
            .display()
            .to_string();

        let cases = [
            (
                "s3://my-bucket/assay/evidence",
                StoreSpec {
                    scheme: "s3".to_string(),
                    bucket: Some("my-bucket".to_string()),
                    prefix: "assay/evidence".to_string(),
                    region: None,
                },
            ),
            (
                "s3://my-bucket/prefix?region=us-west-2",
                StoreSpec {
                    scheme: "s3".to_string(),
                    bucket: Some("my-bucket".to_string()),
                    prefix: "prefix".to_string(),
                    region: Some("us-west-2".to_string()),
                },
            ),
            (
                file_url,
                StoreSpec {
                    scheme: "file".to_string(),
                    bucket: None,
                    prefix: expected_file_prefix,
                    region: None,
                },
            ),
            (
                "memory://test",
                StoreSpec {
                    scheme: "memory".to_string(),
                    bucket: Some("test".to_string()),
                    prefix: String::new(),
                    region: None,
                },
            ),
        ];

        for (url, expected) in cases {
            let actual = StoreSpec::parse(url).unwrap();
            assert_eq!(actual.scheme, expected.scheme, "scheme mismatch for {url}");
            assert_eq!(actual.bucket, expected.bucket, "bucket mismatch for {url}");
            assert_eq!(actual.prefix, expected.prefix, "prefix mismatch for {url}");
            assert_eq!(actual.region, expected.region, "region mismatch for {url}");
        }
    }

    fn assert_file_root_url_is_refused(url: &str) {
        let expected_spec = url::Url::parse(url)
            .expect("test input should parse as URL")
            .to_string();
        let err = StoreSpec::parse(url).expect_err("rooted file URL must be refused");
        match err {
            StoreError::InvalidSpec { spec, reason } => {
                assert_eq!(spec, expected_spec);
                assert!(
                    reason.contains("filesystem root"),
                    "reason should explain root refusal: {reason}"
                );
                assert!(
                    reason.contains("file:///tmp/assay-store"),
                    "reason should include an explicit path example: {reason}"
                );
            }
            other => panic!("expected InvalidSpec, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_rejects_file_url_without_explicit_path_file_double_slash() {
        assert_file_root_url_is_refused("file://");
    }

    #[test]
    fn test_parse_rejects_file_url_without_explicit_path_file_scheme_only() {
        assert_file_root_url_is_refused("file:");
    }

    #[test]
    fn test_parse_rejects_file_url_without_explicit_path_file_triple_slash() {
        assert_file_root_url_is_refused("file:///");
    }
}
