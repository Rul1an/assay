use super::*;
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
    "/home/user/secret.txt":
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
fn test_promptfoo_imported_receipts_feed_trust_basis_generation() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("results.jsonl");
    let bundle = dir.path().join("promptfoo-receipts.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"gradingResult":{"componentResults":[{"pass":true,"score":1,"reason":"Assertion passed","assertion":{"type":"equals","value":"Hello world"}}]}}"#,
            "\n",
            r#"{"gradingResult":{"componentResults":[{"pass":false,"score":0,"reason":"Expected output \"Goodbye world\" to equal \"Hello world\"","assertion":{"type":"equals","value":"Hello world"}}]}}"#,
            "\n"
        ),
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("promptfoo-jsonl")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("results.jsonl")
        .arg("--run-id")
        .arg("promptfoo_trust_basis")
        .arg("--import-time")
        .arg("2026-04-26T12:00:00Z")
        .assert()
        .success();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify")
        .arg(&bundle)
        .assert()
        .success();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("trust-basis")
        .arg("generate")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let claims = json["claims"].as_array().unwrap();
    assert_eq!(
        claims.len(),
        10,
        "P45b keeps all frozen Trust Basis claims present"
    );
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "verified",
        "Promptfoo receipts should now surface the bounded external receipt boundary claim"
    );
    assert_eq!(
        claim(claims, "external_decision_receipt_boundary_visible")["level"],
        "absent",
        "Promptfoo receipts are eval receipts, not decision receipts"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "absent",
        "Promptfoo receipts are eval receipts, not inventory receipts"
    );
}

#[test]
fn test_openfeature_imported_decision_receipts_verify_and_feed_trust_basis_generation() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("openfeature-details.jsonl");
    let bundle = dir.path().join("openfeature-receipts.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"schema":"openfeature.evaluation-details.export.v1","framework":"openfeature","surface":"evaluation_details","target_kind":"feature_flag","flag_key":"checkout.new_flow","result":{"value":true,"variant":"on","reason":"STATIC"}}"#,
            "\n",
            r#"{"schema":"openfeature.evaluation-details.export.v1","framework":"openfeature","surface":"evaluation_details","target_kind":"feature_flag","flag_key":"checkout.missing","result":{"value":false,"reason":"ERROR","error_code":"FLAG_NOT_FOUND"}}"#,
            "\n"
        ),
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("openfeature-details")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("openfeature-details.jsonl")
        .arg("--run-id")
        .arg("openfeature_trust_basis")
        .arg("--import-time")
        .arg("2026-04-27T12:00:00Z")
        .assert()
        .success();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("verify")
        .arg(&bundle)
        .assert()
        .success();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("trust-basis")
        .arg("generate")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let claims = json["claims"].as_array().unwrap();
    assert_eq!(
        claims.len(),
        10,
        "P45b keeps all frozen Trust Basis claims present"
    );
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent",
        "OpenFeature decision receipts are not external eval receipts"
    );
    assert_eq!(
        claim(claims, "external_decision_receipt_boundary_visible")["level"],
        "verified",
        "OpenFeature decision receipts should surface the bounded decision receipt boundary claim"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "absent",
        "OpenFeature decision receipts are not inventory receipts"
    );
}
