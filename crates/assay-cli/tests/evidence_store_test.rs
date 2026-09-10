//! Integration tests for BYOS evidence store commands.
//!
//! Uses `file://` backend with temp directories for fully offline testing.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn create_test_bundle(dir: &std::path::Path) -> std::path::PathBuf {
    let profile_path = dir.join("profile.yaml");
    let bundle_path = dir.join("bundle.tar.gz");

    let profile = r#"
version: "1.0"
name: store-test
created_at: "2026-03-15T12:00:00Z"
updated_at: "2026-03-15T12:00:00Z"
total_runs: 1
run_ids: ["store_test_run_001"]
entries:
  files:
    "/tmp/test.txt":
      first_seen: 100
      last_seen: 200
      runs_seen: 1
      hits_total: 1
"#;
    fs::write(&profile_path, profile).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "export", "--profile"])
        .arg(&profile_path)
        .arg("--out")
        .arg(&bundle_path)
        .assert()
        .success();

    bundle_path
}

#[test]
fn test_push_pull_roundtrip() {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let store_url = format!("file://{}", store_dir.display());

    let bundle_path = create_test_bundle(dir.path());

    // Push
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "push"])
        .arg(&bundle_path)
        .args(["--store", &store_url])
        .assert()
        .success()
        .stderr(predicate::str::contains("Uploaded"));

    // List
    let list_output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence", "list", "--store", &store_url, "--format", "json",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(list_output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["count"], 1);

    let bundle_id = json["bundles"][0]["bundle_id"].as_str().unwrap();

    // Pull
    let pull_dir = dir.path().join("pulled");
    fs::create_dir_all(&pull_dir).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "pull",
            "--bundle-id",
            bundle_id,
            "--store",
            &store_url,
            "-o",
        ])
        .arg(&pull_dir)
        .assert()
        .success()
        .stderr(predicate::str::contains("Downloaded"));
}

#[test]
fn test_push_idempotent() {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let store_url = format!("file://{}", store_dir.display());

    let bundle_path = create_test_bundle(dir.path());

    // First push
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "push"])
        .arg(&bundle_path)
        .args(["--store", &store_url])
        .assert()
        .success();

    // Second push (idempotent with --allow-exists)
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "push"])
        .arg(&bundle_path)
        .args(["--store", &store_url, "--allow-exists"])
        .assert()
        .success()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn test_push_with_run_id_and_list() {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let store_url = format!("file://{}", store_dir.display());

    let bundle_path = create_test_bundle(dir.path());

    // Push with run ID
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "push"])
        .arg(&bundle_path)
        .args(["--store", &store_url, "--run-id", "test-run-42"])
        .assert()
        .success();

    // List by run ID
    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "list",
            "--store",
            &store_url,
            "--run-id",
            "test-run-42",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("sha256:"));
}

#[test]
fn test_store_status_file_backend() {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let store_url = format!("file://{}", store_dir.display());

    // Status on empty store
    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "store-status",
            "--store",
            &store_url,
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"reachable\": true"))
        .stdout(predicate::str::contains("\"bundle_count\": 0"));

    // Push a bundle, then check status again
    let bundle_path = create_test_bundle(dir.path());
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "push"])
        .arg(&bundle_path)
        .args(["--store", &store_url])
        .assert()
        .success();

    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "store-status",
            "--store",
            &store_url,
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"bundle_count\": 1"));
}

#[test]
fn test_store_status_with_config_file() {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();

    let config_path = dir.path().join("store.yaml");
    let store_url = format!("file://{}", store_dir.display());
    fs::write(&config_path, format!("url: {}\n", store_url)).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "store-status", "--store-config"])
        .arg(&config_path)
        .args(["--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"reachable\": true"));
}

#[test]
fn test_no_store_configured_exits_2() {
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "store-status", "--format", "json"])
        .env_remove("ASSAY_STORE_URL")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Config error"));
}

// ── Identity across push and pull ─────────────────────────────────────────────────────────────
//
// The store is keyed by `bundle_id`, which is the bundle's `run_root`: a digest over event
// semantics, not over the archive bytes. Two findings followed from that. `pull --verify` checked
// that the bytes were *a* valid bundle, never that they were the bundle that was asked for; and
// `push` treated any existing object under the key as an idempotent re-upload without looking at
// it, then linked the run to whatever was there.

/// Export a bundle whose single observed file is `observed`, so each call can differ in content.
fn create_bundle_observing(
    dir: &std::path::Path,
    name: &str,
    observed: &str,
) -> std::path::PathBuf {
    let profile_path = dir.join(format!("{name}.yaml"));
    let bundle_path = dir.join(format!("{name}.tar.gz"));
    let profile = format!(
        r#"
version: "1.0"
name: {name}
created_at: "2026-03-15T12:00:00Z"
updated_at: "2026-03-15T12:00:00Z"
total_runs: 1
run_ids: ["{name}_run"]
entries:
  files:
    "{observed}":
      first_seen: 100
      last_seen: 200
      runs_seen: 1
      hits_total: 1
"#
    );
    fs::write(&profile_path, profile).unwrap();
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "export", "--profile"])
        .arg(&profile_path)
        .arg("--out")
        .arg(&bundle_path)
        .assert()
        .success();
    bundle_path
}

/// Push and return the bundle id the command reports.
fn push_and_read_id(bundle: &std::path::Path, store_url: &str) -> String {
    let out = Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "push"])
        .arg(bundle)
        .args(["--store", store_url])
        .assert()
        .success();
    let stderr = String::from_utf8(out.get_output().stderr.clone()).unwrap();
    stderr
        .lines()
        .find_map(|line| line.split("Uploaded: ").nth(1))
        .map(|id| id.trim().to_string())
        .expect("push reports the uploaded bundle id")
}

/// The store object whose bytes equal `bytes`, wherever the file backend nested it.
fn stored_object_with(store_dir: &std::path::Path, bytes: &[u8]) -> std::path::PathBuf {
    fn walk(dir: &std::path::Path, bytes: &[u8]) -> Option<std::path::PathBuf> {
        for entry in fs::read_dir(dir).ok()? {
            let path = entry.ok()?.path();
            if path.is_dir() {
                if let Some(found) = walk(&path, bytes) {
                    return Some(found);
                }
            } else if fs::read(&path).ok()? == bytes {
                return Some(path);
            }
        }
        None
    }
    walk(store_dir, bytes).expect("the pushed archive is stored byte for byte")
}

#[test]
fn pull_verify_refuses_a_valid_bundle_stored_under_another_id() {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let store_url = format!("file://{}", store_dir.display());

    let requested = create_bundle_observing(dir.path(), "requested", "/tmp/requested.txt");
    let other = create_bundle_observing(dir.path(), "other", "/tmp/other.txt");
    let requested_id = push_and_read_id(&requested, &store_url);
    let other_id = push_and_read_id(&other, &store_url);
    assert_ne!(
        requested_id, other_id,
        "the fixture needs two distinct bundles"
    );

    // Anyone who can write to the store puts a different, internally valid bundle under the
    // requested key.
    let requested_object = stored_object_with(&store_dir, &fs::read(&requested).unwrap());
    fs::copy(&other, &requested_object).unwrap();

    let pull_dir = dir.path().join("pulled");
    fs::create_dir_all(&pull_dir).unwrap();
    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "pull",
            "--bundle-id",
            &requested_id,
            "--store",
            &store_url,
        ])
        .arg("--verify")
        .arg("-o")
        .arg(&pull_dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Verified: OK").not())
        .stderr(predicate::str::contains(&other_id));

    assert_eq!(
        fs::read_dir(&pull_dir).unwrap().count(),
        0,
        "a bundle that is not the one requested must not be written under its name"
    );
}

#[test]
fn pull_verify_still_accepts_the_bundle_that_was_requested() {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let store_url = format!("file://{}", store_dir.display());

    let bundle = create_bundle_observing(dir.path(), "requested", "/tmp/requested.txt");
    let id = push_and_read_id(&bundle, &store_url);

    let pull_dir = dir.path().join("pulled");
    fs::create_dir_all(&pull_dir).unwrap();
    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "evidence",
            "pull",
            "--bundle-id",
            &id,
            "--store",
            &store_url,
            "--verify",
            "-o",
        ])
        .arg(&pull_dir)
        .assert()
        .success()
        .stderr(predicate::str::contains("Verified: OK"));
    let written: Vec<_> = fs::read_dir(&pull_dir).unwrap().collect();
    assert_eq!(written.len(), 1);
    assert_eq!(
        fs::read(written[0].as_ref().unwrap().path()).unwrap(),
        fs::read(&bundle).unwrap()
    );
}

/// The same archive content, gzip-compressed again with a different header: a different byte
/// sequence that verifies and carries the same `bundle_id`.
fn recompressed(bundle: &std::path::Path, out: &std::path::Path) {
    use std::io::{Read, Write};
    let mut tar = Vec::new();
    flate2::read::GzDecoder::new(fs::File::open(bundle).unwrap())
        .read_to_end(&mut tar)
        .unwrap();
    let mut encoder = flate2::GzBuilder::new()
        .mtime(1_234_567)
        .write(fs::File::create(out).unwrap(), flate2::Compression::best());
    encoder.write_all(&tar).unwrap();
    encoder.finish().unwrap();
}

#[test]
fn push_refuses_a_different_archive_under_an_existing_id_and_links_nothing() {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let store_url = format!("file://{}", store_dir.display());

    let first = create_bundle_observing(dir.path(), "first", "/tmp/first.txt");
    let id = push_and_read_id(&first, &store_url);
    let second = dir.path().join("second.tar.gz");
    recompressed(&first, &second);
    assert_ne!(fs::read(&first).unwrap(), fs::read(&second).unwrap());

    for extra in [&[][..], &["--allow-exists"][..]] {
        Command::cargo_bin("assay")
            .unwrap()
            .args(["evidence", "push"])
            .arg(&second)
            .args(["--store", &store_url, "--run-id", "collision-run"])
            .args(extra)
            .assert()
            .failure()
            .stderr(predicate::str::contains(&id));
    }

    // Nothing was linked to the run, and the stored object is still the first archive.
    let listed = Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "list", "--store", &store_url])
        .args(["--run-id", "collision-run", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&listed.stdout).contains(&id),
        "a refused push must not link the run to the stored archive"
    );
    stored_object_with(&store_dir, &fs::read(&first).unwrap());
}

#[test]
fn push_of_identical_bytes_stays_idempotent_and_links_the_run() {
    let dir = tempdir().unwrap();
    let store_dir = dir.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let store_url = format!("file://{}", store_dir.display());

    let bundle = create_bundle_observing(dir.path(), "same", "/tmp/same.txt");
    let id = push_and_read_id(&bundle, &store_url);
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "push"])
        .arg(&bundle)
        .args(["--store", &store_url, "--run-id", "same-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("already exists"));
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "list", "--store", &store_url])
        .args(["--run-id", "same-run", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&id));
}
