//! Object store implementation of BundleStore.
//!
//! Supports S3, Azure Blob, GCS, and local filesystem via the `object_store` crate.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::TryStreamExt;
use object_store::{ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload};

use super::bounded::{accumulate_bounded, BoundedGetError, StreamCeiling};
use super::{BundleMeta, BundleStore, KeyBuilder, StoreError, StoreResult, StoreSpec, StoreStatus};

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
}

impl ObjectStoreBundleStore {
    /// Create a store from a parsed spec.
    ///
    /// # Environment Variables (S3)
    ///
    /// | Variable | Description |
    /// |----------|-------------|
    /// | `AWS_ACCESS_KEY_ID` | AWS credentials |
    /// | `AWS_SECRET_ACCESS_KEY` | AWS credentials |
    /// | `AWS_REGION` | Default region (overridden by URL query param) |
    /// | `ASSAY_STORE_REGION` | Override region (highest precedence) |
    /// | `ASSAY_STORE_ALLOW_HTTP` | Allow HTTP (for MinIO dev), default: false |
    /// | `ASSAY_STORE_PATH_STYLE` | Use path-style URLs (for some S3-compat), default: false |
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

                // Start with env-based config (AWS_* vars)
                let mut builder =
                    object_store::aws::AmazonS3Builder::from_env().with_bucket_name(bucket);

                // Region precedence: ASSAY_STORE_REGION > URL param > AWS_REGION
                let region = std::env::var("ASSAY_STORE_REGION")
                    .ok()
                    .or_else(|| spec.region.clone());
                if let Some(r) = region {
                    builder = builder.with_region(&r);
                }

                // Allow HTTP for dev (MinIO, LocalStack)
                let allow_http = std::env::var("ASSAY_STORE_ALLOW_HTTP")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
                builder = builder.with_allow_http(allow_http);

                // Path-style URLs for some S3-compatible endpoints
                if std::env::var("ASSAY_STORE_PATH_STYLE")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false)
                {
                    builder = builder.with_virtual_hosted_style_request(false);
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

        Ok(Self {
            inner,
            keys: KeyBuilder::new(&spec.prefix),
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
        }
    }

    /// Create an in-memory store with a prefix for testing.
    pub fn memory_with_prefix(prefix: &str) -> Self {
        Self {
            inner: Arc::new(object_store::memory::InMemory::new()),
            keys: KeyBuilder::new(prefix),
        }
    }

    /// Build a store over an arbitrary backend. Private: the tests need a backend that misreports
    /// object metadata, which no public constructor can produce and no caller should want.
    #[cfg(test)]
    fn from_parts(inner: Arc<dyn ObjectStore>, prefix: &str) -> Self {
        Self {
            inner,
            keys: KeyBuilder::new(prefix),
        }
    }

    /// Download a bundle under an explicit source-byte ceiling.
    ///
    /// [`BundleStore::get_bundle`] ends in `GetResult::bytes().await`, which materializes the whole
    /// object before returning; a ceiling applied to its result describes an allocation that has
    /// already happened. This streams instead: the accumulator never appends the chunk that would
    /// cross the ceiling and stops polling there, which is what ADR-043 §1 asks of an ingest
    /// entrypoint. Peak resident input is the ceiling plus at most one already-delivered backend
    /// chunk, because a chunk's length cannot be tested before the stream has yielded it.
    ///
    /// The object's declared size is used only to refuse early, never to accept and never as a
    /// capacity hint. Metadata is not ignored, and saying so would be inaccurate — it is simply
    /// not the oracle: a store that under-reports gains nothing, because the streamed bytes are
    /// counted and refused on their own. The cost of the early refusal is availability rather than
    /// safety: a backend that over-reports a size will have a downloadable object refused. That
    /// trade is deliberate, since the alternative is transferring up to a full ceiling's worth of
    /// bytes from a source that has already told us it is too large.
    ///
    /// A refusal here says a configured budget was exceeded and nothing else. It is not a finding
    /// about the remote bundle's validity, which this call never gets far enough to assess.
    pub async fn get_bundle_bounded(
        &self,
        bundle_id: &str,
        ceiling: StreamCeiling,
    ) -> Result<Bytes, BoundedGetError> {
        let key = self.keys.bundle_key(bundle_id);

        // The initial fetch is bounded too, by the same idle timeout the stream polls use.
        //
        // Both transport bounds live inside the accumulator, which is only reached once this
        // future resolves. A backend that accepts the request and then never answers it therefore
        // slipped past every one of them: no chunk is ever counted, no poll is ever timed, and the
        // call simply never returns. Bounding the stream but not the request that produces it
        // leaves the one gap that costs nothing to close.
        let fetched = match tokio::time::timeout(ceiling.idle_timeout, self.inner.get(&key)).await {
            Ok(fetched) => fetched,
            Err(_) => {
                return Err(BoundedGetError::IdleTimeout {
                    limit: ceiling.idle_timeout,
                })
            }
        };

        // A request that did complete keeps its own classification: this is still where a missing
        // bundle becomes `NotFound` rather than a timeout.
        //
        // The fallback text is constant for the same reason the stream read's is. `NotFound` stays
        // separate and still names the bundle id, which is the caller's own argument rather than
        // anything the store chose.
        let result = fetched.map_err(|e| match e {
            object_store::Error::NotFound { .. } => StoreError::NotFound {
                bundle_id: bundle_id.to_string(),
            },
            _ => StoreError::Io {
                message: "failed to get bundle".to_string(),
            },
        })?;

        if result.meta.size > ceiling.max_source_bytes {
            return Err(BoundedGetError::SourceCeiling {
                limit: ceiling.max_source_bytes,
            });
        }

        accumulate_bounded(result.into_stream(), ceiling).await
    }

    /// Check store connectivity, access, and inventory.
    ///
    /// Probes the store for reachability, read/write access, bundle count,
    /// and total size. Object Lock detection is best-effort (`"unknown"` for
    /// most backends).
    pub async fn store_status(&self, spec: &StoreSpec) -> StoreStatus {
        let backend = spec.scheme.clone();
        let bucket = spec.bucket.clone();
        let prefix = spec.prefix.clone();

        // Probe: reachable + readable via list
        let list_result = self.list_bundles(None, Some(10_000)).await;
        let (reachable, readable, bundles) = match list_result {
            Ok(metas) => (true, true, metas),
            Err(_) => (false, false, vec![]),
        };

        let bundle_count = bundles.len() as u64;
        let total_size_bytes: u64 = bundles.iter().filter_map(|m| m.size).sum();

        // Probe: writable via put + delete of a probe key outside the bundles/ namespace
        let writable = if reachable {
            let probe_path = if prefix.is_empty() {
                ".assay_probe_write_test".to_string()
            } else {
                format!("{}/.assay_probe_write_test", prefix.trim_end_matches('/'))
            };
            let probe_key = object_store::path::Path::from(probe_path);
            let probe_bytes = Bytes::from("probe");
            let put_ok = self
                .inner
                .put(&probe_key, PutPayload::from_bytes(probe_bytes))
                .await
                .is_ok();
            if put_ok {
                let _ = self.inner.delete(&probe_key).await;
            }
            put_ok
        } else {
            false
        };

        StoreStatus {
            reachable,
            readable,
            writable,
            backend,
            bucket,
            prefix,
            bundle_count,
            total_size_bytes,
            object_lock: "unknown".to_string(),
        }
    }

    /// Attempt a conditional put (If-None-Match: "*").
    /// Falls back to check-then-put if conditional writes aren't supported.
    ///
    /// # Immutability Guarantees
    ///
    /// | Backend | Conditional Write | Guarantee |
    /// |---------|-------------------|-----------|
    /// | AWS S3 | ✅ PutMode::Create | Strong |
    /// | MinIO | ✅ (recent versions) | Strong |
    /// | R2/B2 | ⚠️ Varies | Check docs |
    /// | file:// | ✅ | Strong |
    /// | memory:// | ✅ | Strong |
    ///
    /// If conditional writes fail with "not supported", we fall back to
    /// check-then-put with a warning. This has a race window but is
    /// acceptable for non-critical backends.
    async fn put_if_not_exists(
        &self,
        key: &object_store::path::Path,
        bytes: Bytes,
    ) -> StoreResult<()> {
        // Try conditional write first (preferred)
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
                // Object exists - return AlreadyExists error for caller to handle as idempotent
                return Err(StoreError::AlreadyExists {
                    bundle_id: key.as_ref().to_string(),
                });
            }
            Err(object_store::Error::NotSupported { .. }) => {
                // Fall through to check-then-put
                tracing::warn!(
                    key = %key.as_ref(),
                    "Conditional writes not supported by backend. \
                     Falling back to check-then-put (race window exists). \
                     Immutability not guaranteed for this store."
                );
            }
            Err(e) => return Err(e.into()),
        }

        // Fallback: check if exists, then put
        // ⚠️ Race condition: another writer could put between head and put
        // This is best-effort for non-compliant backends
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
                        size: Some(entry.size),
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

#[cfg(test)]
mod bounded_download {
    use super::*;
    use crate::store::StreamCeiling;
    use object_store::path::Path as ObjPath;
    use object_store::{GetOptions, GetResult, ListResult, ObjectMeta, PutResult};
    use std::ops::Range;

    /// A backend that delegates everything but lies about how large an object is.
    ///
    /// No public constructor can build one, which is the point: the question is what happens when
    /// a remote reports a size that does not match the bytes it then sends, and that is a property
    /// of an untrusted store rather than of ours.
    #[derive(Debug)]
    struct MisreportingStore {
        inner: object_store::memory::InMemory,
        reported_size: u64,
    }

    impl std::fmt::Display for MisreportingStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "MisreportingStore")
        }
    }

    #[async_trait]
    impl ObjectStore for MisreportingStore {
        async fn put_opts(
            &self,
            location: &ObjPath,
            payload: object_store::PutPayload,
            opts: object_store::PutOptions,
        ) -> object_store::Result<PutResult> {
            self.inner.put_opts(location, payload, opts).await
        }

        async fn put_multipart_opts(
            &self,
            location: &ObjPath,
            opts: object_store::PutMultipartOptions,
        ) -> object_store::Result<Box<dyn object_store::MultipartUpload>> {
            self.inner.put_multipart_opts(location, opts).await
        }

        async fn get_opts(
            &self,
            location: &ObjPath,
            options: GetOptions,
        ) -> object_store::Result<GetResult> {
            let mut result = self.inner.get_opts(location, options).await?;
            // The payload is untouched; only the declared size is a lie.
            result.meta.size = self.reported_size;
            Ok(result)
        }

        fn delete_stream(
            &self,
            locations: futures::stream::BoxStream<'static, object_store::Result<ObjPath>>,
        ) -> futures::stream::BoxStream<'static, object_store::Result<ObjPath>> {
            self.inner.delete_stream(locations)
        }

        fn list(
            &self,
            prefix: Option<&ObjPath>,
        ) -> futures::stream::BoxStream<'static, object_store::Result<ObjectMeta>> {
            self.inner.list(prefix)
        }

        async fn list_with_delimiter(
            &self,
            prefix: Option<&ObjPath>,
        ) -> object_store::Result<ListResult> {
            self.inner.list_with_delimiter(prefix).await
        }

        async fn copy_opts(
            &self,
            from: &ObjPath,
            to: &ObjPath,
            options: object_store::CopyOptions,
        ) -> object_store::Result<()> {
            self.inner.copy_opts(from, to, options).await
        }

        async fn get_ranges(
            &self,
            location: &ObjPath,
            ranges: &[Range<u64>],
        ) -> object_store::Result<Vec<Bytes>> {
            self.inner.get_ranges(location, ranges).await
        }
    }

    async fn store_with(bundle_id: &str, body: Vec<u8>) -> ObjectStoreBundleStore {
        let store = ObjectStoreBundleStore::memory();
        store
            .put_bundle(bundle_id, Bytes::from(body))
            .await
            .expect("seed");
        store
    }

    /// Seed through the lying wrapper, so the stored bytes are real and only `meta.size` differs.
    async fn misreporting_store_with(
        bundle_id: &str,
        body: Vec<u8>,
        reported_size: u64,
    ) -> ObjectStoreBundleStore {
        let lying = Arc::new(MisreportingStore {
            inner: object_store::memory::InMemory::new(),
            reported_size,
        });
        let store = ObjectStoreBundleStore::from_parts(lying, "");
        store
            .put_bundle(bundle_id, Bytes::from(body))
            .await
            .expect("seed");
        store
    }

    #[tokio::test]
    async fn exact_and_over_the_ceiling() {
        let id = "sha256:exact";
        let store = store_with(id, vec![b'x'; 100]).await;

        assert_eq!(
            store
                .get_bundle_bounded(id, StreamCeiling::new(100))
                .await
                .expect("exactly the ceiling must be accepted")
                .len(),
            100
        );
        let err = store
            .get_bundle_bounded(id, StreamCeiling::new(99))
            .await
            .expect_err("one byte over the ceiling must refuse");
        assert!(
            matches!(err, BoundedGetError::SourceCeiling { .. }),
            "expected a byte-ceiling refusal, got {err:?}"
        );
    }

    /// `NotFound` still travels as itself, so the CLI's existing handling is unchanged.
    #[tokio::test]
    async fn a_missing_bundle_is_still_not_found() {
        let store = ObjectStoreBundleStore::memory();
        let err = store
            .get_bundle_bounded("sha256:nonexistent", StreamCeiling::new(1024))
            .await
            .expect_err("missing bundle");
        assert!(!err.is_resource_refusal(), "{err}");
        match err {
            BoundedGetError::Store(StoreError::NotFound { .. }) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    /// Under-reported size buys the remote nothing.
    ///
    /// The object declares one byte and delivers ten thousand. If the declared size were the
    /// oracle the download would sail past the ceiling on the strength of a number the remote
    /// chose. The streamed bytes are counted independently and refuse on their own.
    #[tokio::test]
    async fn an_under_reported_size_does_not_bypass_the_streamed_check() {
        let id = "sha256:liar";
        let store = misreporting_store_with(id, vec![b'x'; 10_000], 1).await;
        let err = store
            .get_bundle_bounded(id, StreamCeiling::new(100))
            .await
            .expect_err("a lying small size must not raise the ceiling");
        assert!(
            matches!(err, BoundedGetError::SourceCeiling { .. }),
            "expected a byte-ceiling refusal, got {err:?}"
        );
    }

    /// Over-reported size refuses early, and that costs availability rather than safety.
    ///
    /// The object declares far more than the ceiling and then delivers ten bytes, so this refusal
    /// is a false one: the download would have fit. That is the accepted trade for not transferring
    /// a ceiling's worth of bytes from a source that has already said it is too large. Recorded as
    /// a test rather than left implicit, because it is a real behaviour a backend with sloppy
    /// metadata will hit.
    #[tokio::test]
    async fn an_over_reported_size_refuses_early_even_though_the_body_would_fit() {
        let id = "sha256:overstated";
        let store = misreporting_store_with(id, vec![b'x'; 10], u64::MAX).await;
        let err = store
            .get_bundle_bounded(id, StreamCeiling::new(100))
            .await
            .expect_err("a declared size over the ceiling refuses before streaming");
        assert!(
            matches!(err, BoundedGetError::SourceCeiling { .. }),
            "expected a byte-ceiling refusal, got {err:?}"
        );

        // And the same object is accepted once the ceiling covers the declared size, which shows
        // the refusal came from the declaration rather than from the bytes.
        let got = store
            .get_bundle_bounded(id, StreamCeiling::new(u64::MAX))
            .await
            .expect("the body itself is ten bytes");
        assert_eq!(got.len(), 10);
    }

    /// A backend that accepts the request and then never answers it.
    ///
    /// Everything else delegates, so the only difference from a working store is that `get_opts`
    /// never resolves — which is what a silently dropped connection looks like from here.
    #[derive(Debug)]
    struct StallingStore {
        inner: object_store::memory::InMemory,
    }

    impl std::fmt::Display for StallingStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "StallingStore")
        }
    }

    #[async_trait]
    impl ObjectStore for StallingStore {
        async fn put_opts(
            &self,
            location: &ObjPath,
            payload: object_store::PutPayload,
            opts: object_store::PutOptions,
        ) -> object_store::Result<PutResult> {
            self.inner.put_opts(location, payload, opts).await
        }

        async fn put_multipart_opts(
            &self,
            location: &ObjPath,
            opts: object_store::PutMultipartOptions,
        ) -> object_store::Result<Box<dyn object_store::MultipartUpload>> {
            self.inner.put_multipart_opts(location, opts).await
        }

        async fn get_opts(
            &self,
            _location: &ObjPath,
            _options: GetOptions,
        ) -> object_store::Result<GetResult> {
            futures::future::pending().await
        }

        fn delete_stream(
            &self,
            locations: futures::stream::BoxStream<'static, object_store::Result<ObjPath>>,
        ) -> futures::stream::BoxStream<'static, object_store::Result<ObjPath>> {
            self.inner.delete_stream(locations)
        }

        fn list(
            &self,
            prefix: Option<&ObjPath>,
        ) -> futures::stream::BoxStream<'static, object_store::Result<ObjectMeta>> {
            self.inner.list(prefix)
        }

        async fn list_with_delimiter(
            &self,
            prefix: Option<&ObjPath>,
        ) -> object_store::Result<ListResult> {
            self.inner.list_with_delimiter(prefix).await
        }

        async fn copy_opts(
            &self,
            from: &ObjPath,
            to: &ObjPath,
            options: object_store::CopyOptions,
        ) -> object_store::Result<()> {
            self.inner.copy_opts(from, to, options).await
        }

        async fn get_ranges(
            &self,
            location: &ObjPath,
            ranges: &[Range<u64>],
        ) -> object_store::Result<Vec<Bytes>> {
            self.inner.get_ranges(location, ranges).await
        }
    }

    /// The gap the stream bounds could not see.
    ///
    /// `max_chunks` and the per-poll timeout both live inside the accumulator, and the accumulator
    /// is only reached once the initial fetch resolves. A backend that never answers therefore
    /// slipped past both: no chunk was ever counted and no poll was ever timed, because neither
    /// ever happened. `start_paused` advances the clock as soon as the runtime goes idle, so this
    /// costs no wall-clock time — and if the bound were absent this test would not fail, it would
    /// never return, which is the shape of the defect.
    #[tokio::test(start_paused = true)]
    async fn a_stalled_initial_fetch_is_refused_by_the_idle_timeout() {
        let stalling = Arc::new(StallingStore {
            inner: object_store::memory::InMemory::new(),
        });
        let store = ObjectStoreBundleStore::from_parts(stalling, "");
        let ceiling = StreamCeiling::new(100 * 1024)
            .with_transport_bounds(1024, std::time::Duration::from_secs(5));

        let err = store
            .get_bundle_bounded("sha256:never-answered", ceiling)
            .await
            .expect_err("a fetch that never resolves must be refused");

        match err {
            BoundedGetError::IdleTimeout { limit } => {
                assert_eq!(limit, std::time::Duration::from_secs(5))
            }
            other => panic!("expected an idle-timeout refusal, got {other:?}"),
        }
    }

    /// The acceptance twin for the bound above: a store that answers promptly is not refused by
    /// it, so the timeout does not simply reject every fetch.
    #[tokio::test(start_paused = true)]
    async fn a_prompt_fetch_is_not_refused_by_the_initial_timeout() {
        let id = "sha256:prompt";
        let store = store_with(id, vec![b'x'; 64]).await;
        let ceiling = StreamCeiling::new(100 * 1024)
            .with_transport_bounds(1024, std::time::Duration::from_secs(5));
        assert_eq!(
            store
                .get_bundle_bounded(id, ceiling)
                .await
                .expect("a prompt store is not a stalled one")
                .len(),
            64
        );
    }

    /// A string only the backend could have supplied. If it shows up in anything an operator
    /// reads, the remote wrote into their terminal and their logs.
    const SENTINEL: &str = "SENTINEL-bucket-9f3a/private-prefix/secret-object.tar.gz";

    /// A backend that fails, carrying the sentinel in its own error.
    ///
    /// `fail_on_stream` chooses which of the two paths fails: the initial request, or a chunk read
    /// once the request has succeeded. Both formatted `object_store::Error` into the surfaced
    /// message, and `object_store::Error`'s rendering carries the store name and object path.
    #[derive(Debug)]
    struct FailingStore {
        inner: object_store::memory::InMemory,
        fail_on_stream: bool,
    }

    impl std::fmt::Display for FailingStore {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "FailingStore")
        }
    }

    fn sentinel_error() -> object_store::Error {
        object_store::Error::Generic {
            store: "SENTINEL-bucket-9f3a",
            source: format!("upstream said: {SENTINEL}").into(),
        }
    }

    #[async_trait]
    impl ObjectStore for FailingStore {
        async fn put_opts(
            &self,
            location: &ObjPath,
            payload: object_store::PutPayload,
            opts: object_store::PutOptions,
        ) -> object_store::Result<PutResult> {
            self.inner.put_opts(location, payload, opts).await
        }

        async fn put_multipart_opts(
            &self,
            location: &ObjPath,
            opts: object_store::PutMultipartOptions,
        ) -> object_store::Result<Box<dyn object_store::MultipartUpload>> {
            self.inner.put_multipart_opts(location, opts).await
        }

        async fn get_opts(
            &self,
            location: &ObjPath,
            options: GetOptions,
        ) -> object_store::Result<GetResult> {
            if !self.fail_on_stream {
                return Err(sentinel_error());
            }
            // The request succeeds; the body then fails partway through.
            let mut result = self.inner.get_opts(location, options).await?;
            result.payload =
                object_store::GetResultPayload::Stream(Box::pin(futures::stream::iter(vec![
                    Ok(Bytes::from_static(b"partial")),
                    Err(sentinel_error()),
                ])));
            Ok(result)
        }

        fn delete_stream(
            &self,
            locations: futures::stream::BoxStream<'static, object_store::Result<ObjPath>>,
        ) -> futures::stream::BoxStream<'static, object_store::Result<ObjPath>> {
            self.inner.delete_stream(locations)
        }

        fn list(
            &self,
            prefix: Option<&ObjPath>,
        ) -> futures::stream::BoxStream<'static, object_store::Result<ObjectMeta>> {
            self.inner.list(prefix)
        }

        async fn list_with_delimiter(
            &self,
            prefix: Option<&ObjPath>,
        ) -> object_store::Result<ListResult> {
            self.inner.list_with_delimiter(prefix).await
        }

        async fn copy_opts(
            &self,
            from: &ObjPath,
            to: &ObjPath,
            options: object_store::CopyOptions,
        ) -> object_store::Result<()> {
            self.inner.copy_opts(from, to, options).await
        }

        async fn get_ranges(
            &self,
            location: &ObjPath,
            ranges: &[Range<u64>],
        ) -> object_store::Result<Vec<Bytes>> {
            self.inner.get_ranges(location, ranges).await
        }
    }

    async fn failing_store(bundle_id: &str, fail_on_stream: bool) -> ObjectStoreBundleStore {
        let backend = Arc::new(FailingStore {
            inner: object_store::memory::InMemory::new(),
            fail_on_stream,
        });
        let store = ObjectStoreBundleStore::from_parts(backend, "");
        if fail_on_stream {
            store
                .put_bundle(bundle_id, Bytes::from(vec![b'x'; 64]))
                .await
                .expect("seed");
        }
        store
    }

    /// Both renderings, not only `Display`.
    ///
    /// `StoreError::Io` holds its text in a field, so a leak shows up in `Debug` too — and `Debug`
    /// is what lands in a log line written with `{:?}` or in an `anyhow` chain. Checking only
    /// `to_string()` would pass while the value still travelled.
    fn assert_sentinel_absent(err: &BoundedGetError, path: &str) {
        for rendered in [format!("{err}"), format!("{err:?}"), format!("{err:#?}")] {
            for fragment in [
                SENTINEL,
                "SENTINEL-bucket-9f3a",
                "private-prefix",
                "secret-object",
            ] {
                assert!(
                    !rendered.contains(fragment),
                    "backend-chosen text {fragment:?} reached the {path} error: {rendered}"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_failing_initial_fetch_does_not_echo_backend_text() {
        let store = failing_store("sha256:whatever", false).await;
        let err = store
            .get_bundle_bounded("sha256:whatever", StreamCeiling::new(100 * 1024))
            .await
            .expect_err("the backend fails the request");
        assert_sentinel_absent(&err, "initial fetch");
    }

    #[tokio::test]
    async fn a_failing_stream_read_does_not_echo_backend_text() {
        let id = "sha256:streamfail";
        let store = failing_store(id, true).await;
        let err = store
            .get_bundle_bounded(id, StreamCeiling::new(100 * 1024))
            .await
            .expect_err("the backend fails partway through the body");
        assert_sentinel_absent(&err, "stream read");
    }

    /// The acceptance twin. A constant message must not flatten the classification: a store
    /// failure is still a store failure and a missing bundle is still `NotFound`, distinguished by
    /// variant rather than by text, and neither is mistaken for a budget refusal.
    #[tokio::test]
    async fn ordinary_store_classification_survives_the_constant_message() {
        let store = failing_store("sha256:whatever", false).await;
        let err = store
            .get_bundle_bounded("sha256:whatever", StreamCeiling::new(100 * 1024))
            .await
            .unwrap_err();
        match &err {
            BoundedGetError::Store(StoreError::Io { .. }) => {}
            other => panic!("a backend failure must stay a store IO error, got {other:?}"),
        }
        assert!(!err.is_resource_refusal(), "not a budget refusal: {err}");

        let missing = ObjectStoreBundleStore::memory();
        let err = missing
            .get_bundle_bounded("sha256:absent", StreamCeiling::new(100 * 1024))
            .await
            .unwrap_err();
        match &err {
            BoundedGetError::Store(StoreError::NotFound { bundle_id }) => {
                assert_eq!(
                    bundle_id, "sha256:absent",
                    "NotFound still names the caller's own argument"
                );
            }
            other => panic!("a missing bundle must stay NotFound, got {other:?}"),
        }
    }

    /// The acceptance twin: an ordinary bundle round-trips byte-for-byte under a real ceiling.
    #[tokio::test]
    async fn an_ordinary_bundle_round_trips() {
        let id = "sha256:ordinary";
        let body = vec![b'a'; 4096];
        let store = store_with(id, body.clone()).await;
        let got = store
            .get_bundle_bounded(id, StreamCeiling::new(100 * 1024 * 1024))
            .await
            .expect("ordinary download");
        assert_eq!(got.as_ref(), body.as_slice());
    }
}
