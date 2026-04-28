#![allow(deprecated)]
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn claim<'a>(claims: &'a [serde_json::Value], id: &str) -> &'a serde_json::Value {
    claims
        .iter()
        .find(|claim| claim["id"] == id)
        .expect("claim should exist")
}

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
        9,
        "P45 keeps all frozen Trust Basis claims present"
    );
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "verified",
        "Promptfoo receipts should now surface the bounded external receipt boundary claim"
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
        9,
        "P45 keeps all frozen Trust Basis claims present"
    );
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent",
        "OpenFeature decision receipts are not external eval receipts"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "absent",
        "OpenFeature decision receipts are not inventory receipts"
    );
}

#[test]
fn test_mastra_imported_score_receipts_verify_and_feed_trust_basis_generation() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("mastra-score-events.jsonl");
    let bundle = dir.path().join("mastra-score-receipts.tar.gz");
    fs::write(
        &input,
        concat!(
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:53:12.297Z","scorer_id":"p14-live-capture-scorer","score":0.92,"target_ref":"span:7c4180655970aca2","trace_id_ref":"59896b9a054b88cb48748463a0f2ab59","span_id_ref":"7c4180655970aca2","score_source":"live"}"#,
            "\n",
            r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-15T18:58:12.297Z","scorer_name":"P14 Live Capture Scorer","score":0.18,"target_ref":"span:c4b7f4a58f2d90e1","trace_id_ref":"9f5bbab9073de1205f4a1de4925ad2b","span_id_ref":"c4b7f4a58f2d90e1","metadata_ref":"metadata:p14-live-capture"}"#,
            "\n"
        ),
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("mastra-score-event")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("mastra-score-events.jsonl")
        .arg("--run-id")
        .arg("mastra_trust_basis")
        .arg("--import-time")
        .arg("2026-04-28T12:00:00Z")
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
        9,
        "P14c adds an experimental receipt type, not a Trust Basis claim"
    );
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent",
        "Mastra score receipts are not supported eval receipt claims in P14c"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "absent",
        "Mastra score receipts are not inventory receipts"
    );
}

#[test]
fn test_cyclonedx_mlbom_model_receipts_verify_and_feed_trust_basis_generation() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("bom.cdx.json");
    let bundle = dir.path().join("cyclonedx-model-receipts.tar.gz");
    fs::write(
        &input,
        r#"{
  "bomFormat": "CycloneDX",
  "specVersion": "1.7",
  "components": [
    {
      "bom-ref": "pkg:huggingface/example/model@abc123",
      "type": "machine-learning-model",
      "publisher": "Example Inc.",
      "name": "example-model",
      "version": "1.0.0",
      "purl": "pkg:huggingface/example/model@abc123",
      "modelCard": {
        "bom-ref": "model-card-example-model",
        "modelParameters": {
          "datasets": [{ "ref": "component-training-data" }]
        }
      }
    },
    {
      "bom-ref": "component-training-data",
      "type": "data",
      "name": "Training Data"
    }
  ]
}"#,
    )
    .unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("import")
        .arg("cyclonedx-mlbom-model")
        .arg("--input")
        .arg(&input)
        .arg("--bundle-out")
        .arg(&bundle)
        .arg("--source-artifact-ref")
        .arg("bom.cdx.json")
        .arg("--run-id")
        .arg("cyclonedx_trust_basis")
        .arg("--import-time")
        .arg("2026-04-28T12:00:00Z")
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
        9,
        "P45 adds one bounded external inventory receipt boundary claim"
    );
    assert_eq!(claim(claims, "bundle_verified")["level"], "verified");
    assert_eq!(
        claim(claims, "external_eval_receipt_boundary_visible")["level"],
        "absent",
        "CycloneDX ML-BOM model receipts are inventory receipts, not external eval receipts"
    );
    assert_eq!(
        claim(claims, "external_inventory_receipt_boundary_visible")["level"],
        "verified",
        "CycloneDX ML-BOM model receipts should surface the bounded inventory receipt boundary claim"
    );
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

#[test]
fn test_evidence_export_includes_sandbox_degraded_event_when_profile_contains_degradation() {
    let dir = tempdir().unwrap();
    let profile_path = dir.path().join("degraded-profile.yaml");
    let bundle_path = dir.path().join("degraded-bundle.tar.gz");

    let profile_content = r#"
version: "1.0"
name: degraded-flow
created_at: "2026-01-26T23:00:00Z"
updated_at: "2026-01-26T23:00:00Z"
total_runs: 1
run_ids: ["degraded_run_123"]
entries:
  processes:
    "/usr/bin/true":
      first_seen: 100
      last_seen: 100
      runs_seen: 1
      hits_total: 1
sandbox_degradations:
  - reason_code: policy_conflict
    degradation_mode: audit_fallback
    component: landlock
"#;
    fs::write(&profile_path, profile_content).unwrap();

    Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("export")
        .arg("--profile")
        .arg(&profile_path)
        .arg("--out")
        .arg(&bundle_path)
        .assert()
        .success();

    let output = Command::cargo_bin("assay")
        .unwrap()
        .arg("evidence")
        .arg("show")
        .arg(&bundle_path)
        .arg("--format")
        .arg("json")
        .output()
        .unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let events = json["events"].as_array().unwrap();
    let degraded = events
        .iter()
        .find(|event| event["type"] == "assay.sandbox.degraded")
        .expect("expected sandbox degradation event");
    assert_eq!(degraded["subject"], "landlock");
    assert_eq!(degraded["data"]["reason_code"], "policy_conflict");
    assert_eq!(degraded["data"]["degradation_mode"], "audit_fallback");
    assert_eq!(degraded["data"]["component"], "landlock");
}
