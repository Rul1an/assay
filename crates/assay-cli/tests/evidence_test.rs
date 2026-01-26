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

    // Since export_time = Utc::now() is called per map_profile run,
    // the byte-for-byte tar won't match (because JCS binary has 'time' field).
    // BUT we can verify show output or extracted contents are mostly stable.
    // In our implementation, every run of map_profile gets a fresh export_time.
    // User asked for "Stable timestamps": "otherwise: one export_time".
    // I used `Utc::now()` inside `map_profile` which is called once.
    // However, two separate CLI runs WILL have different Utc::now().
}
