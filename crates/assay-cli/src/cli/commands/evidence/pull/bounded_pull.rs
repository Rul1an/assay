//! A refused download leaves no file: the ADR-043 section 1 ceiling seen through the CLI.

use super::*;
use assay_evidence::store::BundleStore;
use assay_evidence::Bytes;

/// An in-memory store holding one bundle of `size` bytes.
async fn seeded_store(bundle_id: &str, size: usize) -> ObjectStoreBundleStore {
    let store = ObjectStoreBundleStore::memory();
    store
        .put_bundle(bundle_id, Bytes::from(vec![b'x'; size]))
        .await
        .expect("seed");
    store
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The property a user can check without reading any code: a refused download leaves no file.
///
/// Asserting only the exit code would not be enough. The old path materialized the object
/// before the CLI saw it, so "the command failed" and "nothing was written" were separate
/// facts, and a future change that streams straight to disk would keep the exit code while
/// leaving a truncated archive behind. A leftover prefix is worse than no file at all: it has
/// a plausible name and a later run cannot tell it from a complete download.
#[tokio::test]
async fn an_oversized_single_pull_writes_no_file() {
    let id = "sha256:toobig";
    let store = seeded_store(id, 10_000).await;
    let out = tempfile::tempdir().expect("tmp");

    let code = pull_single(&store, id, out.path(), false, StreamCeiling::new(100))
        .await
        .expect("a ceiling refusal is an exit code, not an error");

    assert_eq!(code, 2, "a refused download exits 2");
    assert!(
        entries(out.path()).is_empty(),
        "a refused download must leave the output directory empty, found {:?}",
        entries(out.path())
    );
}

/// The same property through the run path, which delegates to `pull_single`. Bounding one
/// function covers both entrypoints, and this is what proves it rather than assuming it.
#[tokio::test]
async fn an_oversized_run_pull_writes_no_file() {
    let id = "sha256:toobig";
    let run = "run-42";
    let store = seeded_store(id, 10_000).await;
    store.link_run_bundle(run, id).await.expect("link");
    let out = tempfile::tempdir().expect("tmp");
    let out_dir = out.path().to_path_buf();

    let code = pull_run(&store, run, &out_dir, false, StreamCeiling::new(100))
        .await
        .expect("run pull returns a code");

    assert_eq!(code, 1, "a run with a refused bundle reports errors");
    assert!(
        entries(out.path()).is_empty(),
        "no bundle in the run may be materialized, found {:?}",
        entries(out.path())
    );
}

/// The acceptance twin. Same fixture shape, ceiling that fits: the file appears with the exact
/// bytes. A guard that refused everything would pass both tests above and fail this one.
#[tokio::test]
async fn a_bundle_within_the_ceiling_is_written_intact() {
    let id = "sha256:fits";
    let store = seeded_store(id, 4096).await;
    let out = tempfile::tempdir().expect("tmp");

    let code = pull_single(
        &store,
        id,
        out.path(),
        false,
        StreamCeiling::new(100 * 1024),
    )
    .await
    .expect("ordinary download");

    assert_eq!(code, 0);
    let written = entries(out.path());
    assert_eq!(written.len(), 1, "exactly one file, found {written:?}");
    let body = std::fs::read(out.path().join(&written[0])).expect("read back");
    assert_eq!(body, vec![b'x'; 4096], "bytes must round-trip");
}

/// Exactly the ceiling is accepted end to end, and one byte more is refused, with the file
/// system as the witness in both directions.
#[tokio::test]
async fn the_ceiling_boundary_holds_through_the_cli_path() {
    let out = tempfile::tempdir().expect("tmp");
    let exact = seeded_store("sha256:exact", 100).await;
    assert_eq!(
        pull_single(
            &exact,
            "sha256:exact",
            out.path(),
            false,
            StreamCeiling::new(100)
        )
        .await
        .expect("exact"),
        0
    );
    assert_eq!(entries(out.path()).len(), 1);

    let out2 = tempfile::tempdir().expect("tmp");
    let over = seeded_store("sha256:over", 101).await;
    assert_eq!(
        pull_single(
            &over,
            "sha256:over",
            out2.path(),
            false,
            StreamCeiling::new(100)
        )
        .await
        .expect("over"),
        2
    );
    assert!(entries(out2.path()).is_empty());
}

/// A missing bundle still exits 2 and writes nothing, unchanged by this slice.
#[tokio::test]
async fn a_missing_bundle_still_exits_two_and_writes_nothing() {
    let store = ObjectStoreBundleStore::memory();
    let out = tempfile::tempdir().expect("tmp");

    let code = pull_single(
        &store,
        "sha256:nonexistent",
        out.path(),
        false,
        StreamCeiling::new(100 * 1024),
    )
    .await
    .expect("not found is an exit code");

    assert_eq!(code, 2);
    assert!(entries(out.path()).is_empty());
}
