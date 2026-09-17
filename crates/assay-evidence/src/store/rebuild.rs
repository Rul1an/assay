//! Evidence run-to-bundle index rebuilding from canonical bundles.
//!
//! Reconstructs the non-canonical run index under `runs/{run_id}/bundles/{bundle_id}.ref`
//! by inspecting the canonical bundles stored under `bundles/`.

use futures::TryStreamExt;
use serde::{Deserialize, Serialize};

use super::bounded::StreamCeiling;
use super::error::{StoreError, StoreResult};
use super::object_store_backend::ObjectStoreBundleStore;
use super::BundleStore;

pub const SCHEMA_INDEX_REBUILD_V0: &str = "assay.evidence.index_rebuild.v0";

/// A stale run reference pointing to a bundle that does not exist in canonical storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleRef {
    pub run_id: String,
    pub bundle_id: String,
}

/// A bundle in canonical storage that failed verification or retrieval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedBundle {
    pub bundle_id: String,
    pub reason: String,
}

/// Summary report of an evidence index rebuild operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IndexRebuildReport {
    pub schema: String,
    pub discovered_bundles: usize,
    pub verified_bundles: usize,
    pub failed_bundles: Vec<FailedBundle>,
    pub refs_linked: usize,
    pub refs_already_indexed: usize,
    pub stale_refs: Vec<StaleRef>,
}

impl ObjectStoreBundleStore {
    /// Rebuild the run index from canonical bundles in the store.
    ///
    /// 1. Lists canonical bundles under `bundles/`.
    /// 2. Lists existing run references under `runs/`.
    /// 3. Identifies stale references (pointing to missing bundles) and reports them.
    ///    Stale references are never trusted and never deleted.
    /// 4. Downloads and verifies each canonical bundle with full cryptographic integrity
    ///    and manifest bundle_id binding. Corrupt bundles are skipped and reported.
    /// 5. Recreates missing run links under `runs/{run_id}/bundles/{bundle_id}.ref`.
    ///    Idempotent: existing references are not re-linked and zero new refs are created
    ///    on repeated runs.
    pub async fn rebuild_index(&self, ceiling: StreamCeiling) -> StoreResult<IndexRebuildReport> {
        // 1. Discover canonical bundles from bundles/
        let base_prefix = self.keys.bundles_prefix();
        let list = self.inner.list(Some(&base_prefix));
        let entries: Vec<_> = list.try_collect().await.map_err(|e| StoreError::Io {
            message: format!("failed to list canonical bundles: {}", e),
        })?;

        let mut canonical_bundle_ids: Vec<String> = Vec::new();
        for entry in &entries {
            if let Some(id) = self.keys.parse_bundle_key(&entry.location) {
                canonical_bundle_ids.push(id);
            }
        }
        canonical_bundle_ids.sort();
        canonical_bundle_ids.dedup();

        let canonical_set: std::collections::HashSet<String> =
            canonical_bundle_ids.iter().cloned().collect();

        // 2. Discover existing run refs under runs/
        let runs_prefix = self.keys.runs_prefix();
        let ref_list = self.inner.list(Some(&runs_prefix));
        let ref_entries: Vec<_> = ref_list.try_collect().await.map_err(|e| StoreError::Io {
            message: format!("failed to list run references: {}", e),
        })?;

        let mut existing_refs: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut stale_refs: Vec<StaleRef> = Vec::new();

        for entry in &ref_entries {
            if let Some((run_id, bundle_id)) = self.keys.parse_run_ref_parts(&entry.location) {
                if !canonical_set.contains(&bundle_id) {
                    stale_refs.push(StaleRef {
                        run_id: run_id.clone(),
                        bundle_id: bundle_id.clone(),
                    });
                }
                existing_refs.insert((run_id, bundle_id));
            }
        }

        // Sort stale_refs for deterministic output
        stale_refs.sort_by(|a, b| (&a.run_id, &a.bundle_id).cmp(&(&b.run_id, &b.bundle_id)));

        // 3. Verify each canonical bundle and (re)create run refs
        let mut verified_bundles = 0;
        let mut failed_bundles = Vec::new();
        let mut refs_linked = 0;
        let mut refs_already_indexed = 0;

        for bundle_id in &canonical_bundle_ids {
            let bytes = match self.get_bundle_bounded(bundle_id.as_str(), ceiling).await {
                Ok(b) => b,
                Err(e) => {
                    failed_bundles.push(FailedBundle {
                        bundle_id: bundle_id.clone(),
                        reason: e.to_string(),
                    });
                    continue;
                }
            };

            let verified = match crate::verify_bundle(std::io::Cursor::new(bytes.as_ref())) {
                Ok(v) => v,
                Err(e) => {
                    failed_bundles.push(FailedBundle {
                        bundle_id: bundle_id.clone(),
                        reason: format!("verification failed: {}", e),
                    });
                    continue;
                }
            };

            if verified.manifest.bundle_id != *bundle_id {
                failed_bundles.push(FailedBundle {
                    bundle_id: bundle_id.clone(),
                    reason: format!(
                        "contract mismatch: key {} does not match manifest id {}",
                        bundle_id, verified.manifest.bundle_id
                    ),
                });
                continue;
            }

            verified_bundles += 1;
            let run_id = verified.manifest.run_id;

            if existing_refs.contains(&(run_id.clone(), bundle_id.clone())) {
                refs_already_indexed += 1;
            } else {
                self.link_run_bundle(&run_id, bundle_id.as_str()).await?;
                existing_refs.insert((run_id, bundle_id.clone()));
                refs_linked += 1;
            }
        }

        failed_bundles.sort_by(|a, b| a.bundle_id.cmp(&b.bundle_id));

        Ok(IndexRebuildReport {
            schema: SCHEMA_INDEX_REBUILD_V0.to_string(),
            discovered_bundles: canonical_bundle_ids.len(),
            verified_bundles,
            failed_bundles,
            refs_linked,
            refs_already_indexed,
            stale_refs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::BundleWriter;
    use crate::types::EvidenceEvent;
    use crate::VerifyLimits;
    use bytes::Bytes;

    fn make_test_bundle(run_id: &str, seq: u64) -> (Bytes, String) {
        let mut buffer = Vec::new();
        {
            let mut w = BundleWriter::new(&mut buffer);
            w.add_event(EvidenceEvent::new(
                "assay.test",
                "urn:assay:test",
                run_id,
                seq,
                serde_json::json!({ "run": run_id, "seq": seq }),
            ));
            w.finish().expect("write bundle");
        }
        let manifest = crate::verify_bundle(std::io::Cursor::new(&buffer))
            .expect("the fixture verifies")
            .manifest;
        (Bytes::from(buffer), manifest.bundle_id)
    }

    fn generous_ceiling() -> StreamCeiling {
        StreamCeiling::new(VerifyLimits::default().max_bundle_bytes)
    }

    #[tokio::test]
    async fn rebuild_recreates_missing_refs_from_manifests() {
        let store = ObjectStoreBundleStore::memory();
        let (bytes1, id1) = make_test_bundle("run_alpha", 0);
        let (bytes2, id2) = make_test_bundle("run_beta", 0);

        store.put_bundle(&id1, bytes1).await.expect("put bundle 1");
        store.put_bundle(&id2, bytes2).await.expect("put bundle 2");

        // Initial state: no refs in the store
        assert!(store
            .list_bundles_for_run("run_alpha")
            .await
            .unwrap()
            .is_empty());
        assert!(store
            .list_bundles_for_run("run_beta")
            .await
            .unwrap()
            .is_empty());

        // First run: links both missing refs
        let report1 = store
            .rebuild_index(generous_ceiling())
            .await
            .expect("first rebuild");

        assert_eq!(report1.discovered_bundles, 2);
        assert_eq!(report1.verified_bundles, 2);
        assert_eq!(report1.refs_linked, 2);
        assert_eq!(report1.refs_already_indexed, 0);
        assert!(report1.failed_bundles.is_empty());
        assert!(report1.stale_refs.is_empty());

        let alpha_bundles = store.list_bundles_for_run("run_alpha").await.unwrap();
        assert_eq!(alpha_bundles, vec![id1.clone()]);
        let beta_bundles = store.list_bundles_for_run("run_beta").await.unwrap();
        assert_eq!(beta_bundles, vec![id2.clone()]);

        // Second run: idempotent, links zero new refs
        let report2 = store
            .rebuild_index(generous_ceiling())
            .await
            .expect("second rebuild");

        assert_eq!(report2.discovered_bundles, 2);
        assert_eq!(report2.verified_bundles, 2);
        assert_eq!(report2.refs_linked, 0);
        assert_eq!(report2.refs_already_indexed, 2);
        assert!(report2.failed_bundles.is_empty());
        assert!(report2.stale_refs.is_empty());
    }

    #[tokio::test]
    async fn rebuild_never_reads_existing_refs() {
        let store = ObjectStoreBundleStore::memory();
        let (bytes, valid_id) = make_test_bundle("valid_run", 0);
        store.put_bundle(&valid_id, bytes).await.expect("put valid");

        // Plant a stale ref to a nonexistent bundle
        let ghost_id = "sha256:ghost_nonexistent_bundle_1234567890";
        store
            .link_run_bundle("stale_run", ghost_id)
            .await
            .expect("plant stale ref");

        let report = store
            .rebuild_index(generous_ceiling())
            .await
            .expect("rebuild");

        // The stale ref is reported, not trusted, and not deleted
        assert_eq!(report.discovered_bundles, 1);
        assert_eq!(report.verified_bundles, 1);
        assert_eq!(report.refs_linked, 1);
        assert_eq!(report.stale_refs.len(), 1);
        assert_eq!(report.stale_refs[0].bundle_id, ghost_id);
        assert_eq!(report.stale_refs[0].run_id, "stale_run");

        // Ghost is NOT trusted: does not exist in store as a bundle
        assert!(!store.bundle_exists(ghost_id).await.unwrap());

        // Stale ref is NOT deleted: still exists in the store
        let stale_run_bundles = store.list_bundles_for_run("stale_run").await.unwrap();
        assert_eq!(stale_run_bundles, vec![ghost_id.to_string()]);
    }

    #[tokio::test]
    async fn rebuild_skips_a_bundle_that_fails_verification() {
        let store = ObjectStoreBundleStore::memory();
        let corrupt_id = "sha256:corrupted_bundle_bytes";
        store
            .put_bundle(corrupt_id, Bytes::from_static(b"corrupted not tar gz"))
            .await
            .expect("put corrupt");

        let report = store
            .rebuild_index(generous_ceiling())
            .await
            .expect("rebuild");

        assert_eq!(report.discovered_bundles, 1);
        assert_eq!(report.verified_bundles, 0);
        assert_eq!(report.refs_linked, 0);
        assert_eq!(report.failed_bundles.len(), 1);
        assert_eq!(report.failed_bundles[0].bundle_id, corrupt_id);

        // Corrupted bundle gets no ref linked
        let refs = store.list_bundles_for_run("any_run").await.unwrap();
        assert!(refs.is_empty());
    }
}
