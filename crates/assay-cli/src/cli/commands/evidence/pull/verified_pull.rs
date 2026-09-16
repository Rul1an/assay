//! `--verify` binds the served bytes to the requested id before anything reaches disk.
//!
//! The store is keyed by `bundle_id`, and a key is a name anyone with write access can put any
//! object under. Verifier check 14 proves `bundle_id == run_root` *inside* the archive; it cannot
//! know which key the archive was fetched under. So a valid bundle B served under key A must be
//! refused as a contract violation between key and content, in the verifier's own vocabulary, and
//! nothing may be written under A's name. (#2492 slice 1)

use super::*;
use assay_evidence::bundle::writer::BundleWriter;
use assay_evidence::store::BundleStore;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::Bytes;

/// A small, internally valid bundle and the id its manifest carries.
fn valid_bundle() -> (Bytes, String) {
    let mut buffer = Vec::new();
    {
        let mut w = BundleWriter::new(&mut buffer);
        w.add_event(EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_verified_pull",
            0,
            serde_json::json!({ "k": "v" }),
        ));
        w.finish().expect("write bundle");
    }
    let id = assay_evidence::verify_bundle(std::io::Cursor::new(&buffer))
        .expect("the fixture verifies")
        .manifest
        .bundle_id;
    (Bytes::from(buffer), id)
}

fn entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect()
}

fn generous() -> StreamCeiling {
    StreamCeiling::new(VerifyLimits::default().max_bundle_bytes)
}

/// The refusal is typed. An untyped `anyhow` message also exits non-zero, but nothing downstream
/// can tell it from a network failure; the verifier's `Contract` class with the bundle-id code is
/// what `evidence show` and the reason-code registry already classify.
#[tokio::test]
async fn a_bundle_served_under_a_foreign_key_is_a_contract_error_naming_both_ids() {
    let (bytes, real_id) = valid_bundle();
    let requested = "sha256:not-its-id";
    let store = ObjectStoreBundleStore::memory();
    store.put_bundle(requested, bytes).await.expect("seed");
    let out = tempfile::tempdir().expect("tmp");

    let err = pull_single(&store, requested, out.path(), true, generous())
        .await
        .expect_err("a foreign key must not be accepted");

    let verifier = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<VerifyError>())
        .unwrap_or_else(|| panic!("no typed VerifyError in the chain: {err:?}"));
    assert_eq!(verifier.class, ErrorClass::Contract);
    assert_eq!(verifier.code, ErrorCode::ContractBundleIdMismatch);
    let text = format!("{err:#}");
    assert!(
        text.contains(requested),
        "must name the requested id: {text}"
    );
    assert!(text.contains(&real_id), "must name the served id: {text}");
    assert!(
        entries(out.path()).is_empty(),
        "nothing may be written under the requested name, found {:?}",
        entries(out.path())
    );
}

/// Verification failing for any other reason leaves no file either. The ceiling arm already
/// refuses before an output path exists; this pins the same discipline on the verify arm.
#[tokio::test]
async fn a_bundle_that_fails_verification_leaves_no_file() {
    let id = "sha256:corrupt";
    let store = ObjectStoreBundleStore::memory();
    store
        .put_bundle(id, Bytes::from_static(b"this is not a bundle"))
        .await
        .expect("seed");
    let out = tempfile::tempdir().expect("tmp");

    pull_single(&store, id, out.path(), true, generous())
        .await
        .expect_err("garbage must not verify");

    assert!(
        entries(out.path()).is_empty(),
        "a failed --verify must leave the output directory empty, found {:?}",
        entries(out.path())
    );
}

/// The acceptance twin: the same bundle under its own id verifies and lands intact.
#[tokio::test]
async fn a_bundle_served_under_its_own_id_verifies_and_is_written() {
    let (bytes, id) = valid_bundle();
    let store = ObjectStoreBundleStore::memory();
    store.put_bundle(&id, bytes.clone()).await.expect("seed");
    let out = tempfile::tempdir().expect("tmp");

    let code = pull_single(&store, &id, out.path(), true, generous())
        .await
        .expect("a matching id verifies");

    assert_eq!(code, 0);
    let written = entries(out.path());
    assert_eq!(written.len(), 1, "exactly one file, found {written:?}");
    let body = std::fs::read(out.path().join(&written[0])).expect("read back");
    assert_eq!(body, bytes.as_ref(), "bytes must round-trip");
}
