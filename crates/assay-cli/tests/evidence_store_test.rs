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
fn test_no_store_configured_fails() {
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "store-status", "--format", "json"])
        .env_remove("ASSAY_STORE_URL")
        .assert()
        .failure();
}
