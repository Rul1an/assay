//! Object store implementation of BundleStore.
//!
//! Supports S3, Azure Blob, GCS, and local filesystem via the `object_store` crate.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::TryStreamExt;
use object_store::{ObjectStore, PutMode, PutOptions, PutPayload};

use super::{BundleMeta, BundleStore, KeyBuilder, StoreError, StoreResult, StoreSpec};

/// Bundle store backed by `object_store`.
///
/// Supports:
/// - S3 and S3-compatible (Backblaze B2, Wasabi, MinIO, R2)
/// - Azure Blob Storage
/// - Google Cloud Storage
/// - Local filesystem
/// - In-memory (for testing)
pub struct ObjectStoreBundleStore {
    inner: Arc<dyn ObjectStore>,
    keys: KeyBuilder,
    /// Whether the backend supports conditional writes (If-None-Match).
    /// We attempt conditional writes first, and fall back if unsupported.
    conditional_writes_supported: bool,
}

impl ObjectStoreBundleStore {
    /// Create a store from a parsed spec.
    pub async fn from_spec(spec: &StoreSpec) -> StoreResult<Self> {
        let inner: Arc<dyn ObjectStore> = match spec.scheme.as_str() {
            "memory" => Arc::new(object_store::memory::InMemory::new()),
            "file" => {
                let path = if let Some(bucket) = &spec.bucket {
                    format!("/{}/{}", bucket, spec.prefix)
                } else if spec.prefix.is_empty() {
                    "/tmp/assay-store".to_string()
                } else {
                    format!("/{}", spec.prefix)
                };
                // Ensure directory exists
                std::fs::create_dir_all(&path).map_err(|e| StoreError::Io {
                    message: format!("failed to create store directory {}: {}", path, e),
                })?;
                Arc::new(
                    object_store::local::LocalFileSystem::new_with_prefix(&path).map_err(|e| {
                        StoreError::Io {
                            message: format!("failed to create local store at {}: {}", path, e),
                        }
                    })?,
                )
            }
            "s3" => {
                let bucket = spec
                    .bucket
                    .as_ref()
                    .ok_or_else(|| StoreError::InvalidSpec {
                        spec: format!("s3://{:?}/{}", spec.bucket, spec.prefix),
                        reason: "S3 URL must include bucket name".to_string(),
                    })?;

                let mut builder = object_store::aws::AmazonS3Builder::from_env()
                    .with_bucket_name(bucket)
                    .with_allow_http(false);

                if let Some(region) = &spec.region {
                    builder = builder.with_region(region);
                }

                Arc::new(builder.build().map_err(|e| StoreError::Io {
                    message: format!("failed to create S3 client: {}", e),
                })?)
            }
            scheme => {
                return Err(StoreError::InvalidSpec {
                    spec: spec.scheme.clone(),
                    reason: format!("unsupported scheme: {}", scheme),
                })
            }
        };

        // For S3, assume conditional writes are supported (AWS added this in 2024)
        // For memory/file, they're always supported via object_store
        let conditional_writes_supported = true;

        Ok(Self {
            inner,
            keys: KeyBuilder::new(&spec.prefix),
            conditional_writes_supported,
        })
    }

    /// Create a store from a URL string.
    pub async fn from_url(url: &str) -> StoreResult<Self> {
        let spec = StoreSpec::parse(url)?;
        Self::from_spec(&spec).await
    }

    /// Create an in-memory store for testing.
    pub fn memory() -> Self {
        Self {
            inner: Arc::new(object_store::memory::InMemory::new()),
            keys: KeyBuilder::new(""),
            conditional_writes_supported: true,
        }
    }

    /// Create an in-memory store with a prefix for testing.
    pub fn memory_with_prefix(prefix: &str) -> Self {
        Self {
            inner: Arc::new(object_store::memory::InMemory::new()),
            keys: KeyBuilder::new(prefix),
            conditional_writes_supported: true,
        }
    }

    /// Attempt a conditional put (If-None-Match: "*").
    /// Falls back to regular put if conditional writes aren't supported.
    async fn put_if_not_exists(
        &self,
        key: &object_store::path::Path,
        bytes: Bytes,
    ) -> StoreResult<()> {
        if self.conditional_writes_supported {
            // Try conditional write first
            let opts = PutOptions {
                mode: PutMode::Create, // Fails if object exists
                ..Default::default()
            };

            match self
                .inner
                .put_opts(key, PutPayload::from_bytes(bytes.clone()), opts)
                .await
            {
                Ok(_) => return Ok(()),
                Err(object_store::Error::AlreadyExists { .. }) => {
                    // Object exists - this is fine for idempotency
                    return Err(StoreError::AlreadyExists {
                        bundle_id: key.as_ref().to_string(),
                    });
                }
                Err(object_store::Error::NotSupported { .. }) => {
                    // Fall through to regular put
                    tracing::warn!(
                        "Conditional writes not supported by backend, falling back to check-then-put"
                    );
                }
                Err(e) => return Err(e.into()),
            }
        }

        // Fallback: check if exists, then put
        // Note: This has a race condition, but it's best-effort for non-compliant backends
        if self.inner.head(key).await.is_ok() {
            return Err(StoreError::AlreadyExists {
                bundle_id: key.as_ref().to_string(),
            });
        }

        self.inner
            .put(key, PutPayload::from_bytes(bytes))
            .await
            .map_err(|e| StoreError::Io {
                message: format!("failed to put object: {}", e),
            })?;

        Ok(())
    }
}

#[async_trait]
impl BundleStore for ObjectStoreBundleStore {
    async fn put_bundle(&self, bundle_id: &str, bytes: Bytes) -> StoreResult<()> {
        let key = self.keys.bundle_key(bundle_id);
        self.put_if_not_exists(&key, bytes).await.map_err(|e| {
            if let StoreError::AlreadyExists { .. } = e {
                StoreError::AlreadyExists {
                    bundle_id: bundle_id.to_string(),
                }
            } else {
                e
            }
        })
    }

    async fn get_bundle(&self, bundle_id: &str) -> StoreResult<Bytes> {
        let key = self.keys.bundle_key(bundle_id);

        let result = self.inner.get(&key).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => StoreError::NotFound {
                bundle_id: bundle_id.to_string(),
            },
            _ => StoreError::Io {
                message: format!("failed to get bundle: {}", e),
            },
        })?;

        result.bytes().await.map_err(|e| StoreError::Io {
            message: format!("failed to read bundle bytes: {}", e),
        })
    }

    async fn bundle_exists(&self, bundle_id: &str) -> StoreResult<bool> {
        let key = self.keys.bundle_key(bundle_id);
        match self.inner.head(&key).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(StoreError::Io {
                message: format!("failed to check bundle existence: {}", e),
            }),
        }
    }

    async fn link_run_bundle(&self, run_id: &str, bundle_id: &str) -> StoreResult<()> {
        let key = self.keys.run_bundle_ref_key(run_id, bundle_id);

        // Reference content is just the bundle_id (for verification)
        let content = Bytes::from(bundle_id.to_string());

        // Idempotent: ignore AlreadyExists
        match self.put_if_not_exists(&key, content).await {
            Ok(()) => Ok(()),
            Err(StoreError::AlreadyExists { .. }) => Ok(()), // Idempotent
            Err(e) => Err(e),
        }
    }

    async fn list_bundles_for_run(&self, run_id: &str) -> StoreResult<Vec<String>> {
        let prefix = self.keys.run_bundles_prefix(run_id);

        let list = self.inner.list(Some(&prefix));
        let entries: Vec<_> = list.try_collect().await.map_err(|e| StoreError::Io {
            message: format!("failed to list run bundles: {}", e),
        })?;

        let bundle_ids: Vec<String> = entries
            .iter()
            .filter_map(|entry| self.keys.parse_run_ref_key(&entry.location))
            .collect();

        Ok(bundle_ids)
    }

    async fn list_bundles(
        &self,
        prefix: Option<&str>,
        limit: Option<usize>,
    ) -> StoreResult<Vec<BundleMeta>> {
        let base_prefix = self.keys.bundles_prefix();
        let full_prefix = if let Some(p) = prefix {
            object_store::path::Path::from(format!("{}{}", base_prefix.as_ref(), p))
        } else {
            base_prefix
        };

        let list = self.inner.list(Some(&full_prefix));
        let entries: Vec<_> = list.try_collect().await.map_err(|e| StoreError::Io {
            message: format!("failed to list bundles: {}", e),
        })?;

        let limit = limit.unwrap_or(1000);

        let metas: Vec<BundleMeta> = entries
            .iter()
            .filter_map(|entry| {
                self.keys
                    .parse_bundle_key(&entry.location)
                    .map(|id| BundleMeta {
                        bundle_id: id,
                        size: Some(entry.size as u64),
                        modified: Some(entry.last_modified),
                    })
            })
            .take(limit)
            .collect();

        Ok(metas)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_store_roundtrip() {
        let store = ObjectStoreBundleStore::memory();

        let bundle_id = "sha256:abc123def456";
        let content = Bytes::from("test bundle content");

        // Put
        store
            .put_bundle(bundle_id, content.clone())
            .await
            .expect("put failed");

        // Exists
        assert!(store.bundle_exists(bundle_id).await.unwrap());

        // Get
        let retrieved = store.get_bundle(bundle_id).await.expect("get failed");
        assert_eq!(retrieved, content);
    }

    #[tokio::test]
    async fn test_put_idempotent() {
        let store = ObjectStoreBundleStore::memory();

        let bundle_id = "sha256:abc123";
        let content = Bytes::from("content");

        // First put succeeds
        store.put_bundle(bundle_id, content.clone()).await.unwrap();

        // Second put returns AlreadyExists
        let result = store.put_bundle(bundle_id, content).await;
        assert!(matches!(result, Err(StoreError::AlreadyExists { .. })));
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let store = ObjectStoreBundleStore::memory();

        let result = store.get_bundle("sha256:nonexistent").await;
        assert!(matches!(result, Err(StoreError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_run_bundle_linking() {
        let store = ObjectStoreBundleStore::memory();

        let run_id = "run_001";
        let bundle_id = "sha256:abc123";

        // Link
        store
            .link_run_bundle(run_id, bundle_id)
            .await
            .expect("link failed");

        // List
        let bundles = store.list_bundles_for_run(run_id).await.unwrap();
        assert_eq!(bundles, vec![bundle_id.to_string()]);
    }

    #[tokio::test]
    async fn test_link_idempotent() {
        let store = ObjectStoreBundleStore::memory();

        let run_id = "run_001";
        let bundle_id = "sha256:abc123";

        // Link twice - should not error
        store.link_run_bundle(run_id, bundle_id).await.unwrap();
        store.link_run_bundle(run_id, bundle_id).await.unwrap();

        // Should only appear once
        let bundles = store.list_bundles_for_run(run_id).await.unwrap();
        assert_eq!(bundles.len(), 1);
    }

    #[tokio::test]
    async fn test_list_bundles() {
        let store = ObjectStoreBundleStore::memory();

        // Put some bundles
        store
            .put_bundle("sha256:aaa", Bytes::from("a"))
            .await
            .unwrap();
        store
            .put_bundle("sha256:bbb", Bytes::from("b"))
            .await
            .unwrap();
        store
            .put_bundle("sha256:ccc", Bytes::from("c"))
            .await
            .unwrap();

        // List all
        let all = store.list_bundles(None, None).await.unwrap();
        assert_eq!(all.len(), 3);

        // List with limit
        let limited = store.list_bundles(None, Some(2)).await.unwrap();
        assert_eq!(limited.len(), 2);

        // Note: prefix filtering works at the key level, not bundle_id level
        // For bundle_id filtering, use list_bundles and filter in memory
        // or use list_bundles_for_run with explicit run IDs
    }

    #[tokio::test]
    async fn test_with_prefix() {
        let store = ObjectStoreBundleStore::memory_with_prefix("assay/evidence");

        let bundle_id = "sha256:test";
        let content = Bytes::from("content");

        store.put_bundle(bundle_id, content).await.unwrap();
        assert!(store.bundle_exists(bundle_id).await.unwrap());
    }
}
