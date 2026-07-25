//! ADR-043 §1 invariant for `evidence push`.
//!
//! Whatever refuses an ingest must refuse *before* the bundle leaves the box. The push path used
//! to `read_to_end` the file with no ceiling, then clone the buffer into `Bytes`, then verify.
//! An oversized archive was already in memory twice by then, and `--no-verify` skipped even the
//! ceiling-consulting step.
//!
//! The current shape wraps the file in a `LimitReader` before the first read, verifies (or opens
//! unverified with the same limits) on a `Cursor` over the bounded buffer, and moves the buffer
//! into `Bytes` only after that. The invariant these tests pin is downstream: if the CLI decides
//! the bundle is not acceptable, the store directory must stay empty. That holds in both modes,
//! and would break under a regression that puts the upload before the check.

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

fn store_layout() -> (tempfile::TempDir, std::path::PathBuf, String) {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let store_url = format!("file://{}", store_dir.display());
    (dir, store_dir, store_url)
}

fn write_garbage(path: &std::path::Path) {
    // Not a tar, not gzip, not anything. The reader rejects this the moment it tries to decode,
    // which is what makes it a clean lever for the "refuse before upload" invariant.
    fs::write(path, b"this is not a bundle, not even close").unwrap();
}

fn store_is_empty(dir: &std::path::Path) -> bool {
    match fs::read_dir(dir) {
        Ok(mut it) => it.next().is_none(),
        Err(_) => true,
    }
}

#[test]
fn push_refuses_a_malformed_bundle_before_writing_to_the_store() {
    let (dir, store_dir, store_url) = store_layout();
    let bundle = dir.path().join("garbage.tar.gz");
    write_garbage(&bundle);

    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "push"])
        .arg(&bundle)
        .args(["--store", &store_url])
        .assert()
        .failure();

    assert!(
        store_is_empty(&store_dir),
        "verify-mode push must refuse before uploading; store dir was populated"
    );
}

#[test]
fn push_no_verify_still_refuses_before_writing_to_the_store() {
    let (dir, store_dir, store_url) = store_layout();
    let bundle = dir.path().join("garbage.tar.gz");
    write_garbage(&bundle);

    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "push", "--no-verify"])
        .arg(&bundle)
        .args(["--store", &store_url])
        .assert()
        .failure();

    assert!(
        store_is_empty(&store_dir),
        "--no-verify must still reject a garbage bundle before uploading; store dir was populated"
    );
}
