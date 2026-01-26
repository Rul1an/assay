#![allow(deprecated)]
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_evidence_export_verify_show_flow() {
    let dir = tempdir().unwrap();
    let profile_path = dir.path().join("profile.yaml");
    let bundle_path = dir.path().join("bundle.tar.gz");

    // 1. Setup rich profile
    let profile_content = r#"
version: "1.0"
name: test-flow
created_at: "2026-01-26T23:00:00Z"
updated_at: "2026-01-26T23:00:00Z"
total_runs: 10
run_ids: ["test_run_123"]
entries:
  files:
    "/Users/roelschuurkes/secret.txt":
      first_seen: 100
      last_seen: 200
      runs_seen: 1
      hits_total: 10
  network:
    "api.stripe.com":
      first_seen: 100
      last_seen: 200
      runs_seen: 1
      hits_total: 5
"#;
    fs::write(&profile_path, profile_content).unwrap();

    // 2. Export
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence")
        .arg("export")
        .arg("--profile")
        .arg(&profile_path)
        .arg("--out")
        .arg(&bundle_path)
        .arg("--detail")
        .arg("observed")
        .assert()
        .success();

    assert!(bundle_path.exists());

    // 3. Verify
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence")
        .arg("verify")
        .arg(&bundle_path)
        .assert()
        .success()
        .stderr(predicate::str::contains("Bundle verified").and(predicate::str::contains("OK")));

    // 4. Show (Verify table content and REDACTION)
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence")
        .arg("show")
        .arg(&bundle_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Verified:    ✅ OK"))
        .stdout(predicate::str::contains("Run ID:      test_run_123"))
        // Check for path generalization (~/**/secret.txt instead of /Users/...)
        .stdout(predicate::str::contains("~/**/secret.txt"))
        .stdout(predicate::str::contains("assay.fs.access"))
        .stdout(predicate::str::contains("api.stripe.com"));
}

#[test]
fn test_evidence_export_deterministic() {
    let dir = tempdir().unwrap();
    let profile_path = dir.path().join("profile.yaml");
    let bundle1 = dir.path().join("bundle1.tar.gz");
    let bundle2 = dir.path().join("bundle2.tar.gz");

    fs::write(&profile_path, "version: \"1.0\"\nname: det-test\ntotal_runs: 1\ncreated_at: \"2026-01-26T23:00:00Z\"\nupdated_at: \"2026-01-26T23:00:00Z\"\nentries: {}").unwrap();

    // Export twice
    for b in &[&bundle1, &bundle2] {
        Command::cargo_bin("assay")
            .unwrap()
            .arg("evidence")
            .arg("export")
            .arg("--profile")
            .arg(&profile_path)
            .arg("--out")
            .arg(b)
            .assert()
            .success();
    }

    // Verify manifest and run_root identity (Absolute determinism)
    // We can't easily check byte-for-byte tar due to gzip headers,
    // but we can check that they have identical Bundle IDs.
    let get_bundle_id = |path: &std::path::Path| {
        let mut cmd = Command::cargo_bin("assay").unwrap();
        let output = cmd
            .arg("evidence")
            .arg("show")
            .arg(path)
            .arg("--format")
            .arg("json")
            .output()
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        json["manifest"]["bundle_id"].as_str().unwrap().to_string()
    };

    let id1 = get_bundle_id(&bundle1);
    let id2 = get_bundle_id(&bundle2);
    assert_eq!(
        id1, id2,
        "Bundles should have identical IDs when anchored to same profile"
    );
    assert!(!id1.is_empty());
}

#[test]
fn test_evidence_verify_fail_corrupt_manifest() {
    let dir = tempdir().unwrap();
    let bundle_path = dir.path().join("corrupt.tar.gz");

    // 1. Create valid bundle
    let profile_path = dir.path().join("profile.yaml");
    fs::write(&profile_path, "version: \"1.0\"\nname: corrupt-test\ntotal_runs: 1\ncreated_at: \"2026-01-26T23:00:00Z\"\nupdated_at: \"2026-01-26T23:00:00Z\"\nentries: {}").unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence")
        .arg("export")
        .arg("--profile")
        .arg(&profile_path)
        .arg("--out")
        .arg(&bundle_path)
        .assert()
        .success();

    // 2. Corrupt it (flip a byte in the middle of the gzip)
    let mut bytes = fs::read(&bundle_path).unwrap();
    if bytes.len() > 50 {
        bytes[40] ^= 0xFF;
    }
    fs::write(&bundle_path, bytes).unwrap();

    // 3. Verify should fail
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence")
        .arg("verify")
        .arg(&bundle_path)
        .assert()
        .failure()
        .stderr(predicate::str::is_match("(?i)(failed|corrupt|invalid)").unwrap());
}

#[test]
fn test_evidence_verify_fail_on_extra_file() {
    let dir = tempdir().unwrap();
    let bundle_path = dir.path().join("extra.tar.gz");
    let bundle_unpacked = dir.path().join("unpacked");
    fs::create_dir(&bundle_unpacked).unwrap();

    // 1. Create valid bundle
    let profile_path = dir.path().join("profile.yaml");
    fs::write(&profile_path, "version: \"1.0\"\nname: extra-test\ntotal_runs: 1\ncreated_at: \"2026-01-26T23:00:00Z\"\nupdated_at: \"2026-01-26T23:00:00Z\"\nentries: {}").unwrap();
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence")
        .arg("export")
        .arg("--profile")
        .arg(&profile_path)
        .arg("--out")
        .arg(&bundle_path)
        .assert()
        .success();

    // 2. Use tar to add extra file
    // Note: This relies on 'tar' Being available on the system (standard on Mac/Linux)
    let _ = std::process::Command::new("gunzip")
        .arg(&bundle_path)
        .status();
    let bundle_tar = dir.path().join("extra.tar");
    fs::write(dir.path().join("malicious.txt"), "hello").unwrap();
    let _ = std::process::Command::new("tar")
        .arg("-rf")
        .arg(&bundle_tar)
        .arg("-C")
        .arg(dir.path())
        .arg("malicious.txt")
        .status();
    let _ = std::process::Command::new("gzip").arg(&bundle_tar).status();
    fs::rename(dir.path().join("extra.tar.gz"), &bundle_path).unwrap();

    // 3. Verify should fail
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("evidence")
        .arg("verify")
        .arg(&bundle_path)
        .assert()
        .failure()
        .stderr(predicate::str::is_match("(?i)(extra|disallowed|unexpected)").unwrap());
}
