use super::*;
use sha2::{Digest, Sha256};

// Exercise the CLI, published schema and verifying bundle reader together.
// A missing guard, byte/grapheme count, trimming or a widened claim must fail.
fn source_ref_contract(importer: &str, schema: &str, claim_id: &str, source: &str) {
    let dir = tempdir().unwrap();
    let input = dir.path().join("source.jsonl");
    let bundle = dir.path().join("receipts.tar.gz");
    fs::write(&input, source).unwrap();
    let import = |reference: Option<&str>| {
        let mut cmd = Command::cargo_bin("assay").unwrap();
        cmd.args(["evidence", "import", importer, "--input"])
            .arg(&input)
            .arg("--bundle-out")
            .arg(&bundle)
            .args(["--import-time", "2026-10-18T12:00:00Z"]);
        if let Some(reference) = reference {
            cmd.arg("--source-artifact-ref").arg(reference);
        }
        cmd.output().unwrap()
    };
    let result = import(None);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let baseline = bundle_payloads(&bundle).remove(0);
    assert_eq!(baseline["source_artifact_ref"], "source.jsonl");
    let validator = compile_schema(&format!("receipts/{schema}.schema.json"));
    assert!(validator.is_valid(&baseline));

    for reference in [String::new(), "é".repeat(513), "e\u{0301}".repeat(257)] {
        let mut invalid = baseline.clone();
        invalid["source_artifact_ref"] = json!(reference);
        assert!(!validator.is_valid(&invalid));
        for input_exists in [true, false] {
            if input_exists {
                fs::write(&input, source).unwrap();
            } else {
                fs::remove_file(&input).unwrap();
            }
            fs::write(&bundle, b"preserve existing output").unwrap();
            let output = import(Some(&reference));
            assert!(
                !output.status.success(),
                "{importer}: invalid reference accepted"
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("source-artifact-ref"),
                "wrong failure: {stderr}"
            );
            assert!(
                !stderr.contains("failed to digest"),
                "input read before validation"
            );
            assert!(output.stdout.is_empty());
            assert_eq!(fs::read(&bundle).unwrap(), b"preserve existing output");
            fs::remove_file(&bundle).unwrap();
            assert!(!import(Some(&reference)).status.success());
            assert!(!bundle.exists());
        }
    }
    fs::write(&input, source).unwrap();
    for (reference, expected_level) in [
        ("x".to_owned(), "verified"),
        ("é".repeat(240), "verified"),
        ("é".repeat(241), "absent"),
        ("é".repeat(512), "absent"),
        ("😀".repeat(512), "absent"),
        ("e\u{0301}".repeat(256), "absent"),
        (" source.jsonl ".to_owned(), "absent"),
        (" ".to_owned(), "absent"),
        ("a\nb\u{0085}".to_owned(), "absent"),
    ] {
        let output = import(Some(&reference));
        assert!(
            output.status.success(),
            "{importer}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let receipts = bundle_payloads(&bundle);
        assert_eq!(receipts.len(), 1);
        let receipt = &receipts[0];
        assert_eq!(receipt["source_artifact_ref"], reference);
        assert!(validator.is_valid(receipt));
        assert_eq!(fs::read(&input).unwrap(), source.as_bytes());
        assert_eq!(
            receipt["source_artifact_digest"],
            format!("sha256:{}", hex::encode(Sha256::digest(source.as_bytes())))
        );
        let mut normalized = receipt.clone();
        normalized["source_artifact_ref"] = baseline["source_artifact_ref"].clone();
        assert_eq!(normalized, baseline, "only source ref should change");
        let output = Command::cargo_bin("assay")
            .unwrap()
            .args(["trust-basis", "generate"])
            .arg(&bundle)
            .output()
            .unwrap();
        assert!(output.status.success());
        let report: Value = serde_json::from_slice(&output.stdout).unwrap();
        for claim in report["claims"].as_array().unwrap() {
            let expected = if claim["id"] == "bundle_verified" {
                "verified"
            } else if claim["id"] == claim_id {
                expected_level
            } else {
                "absent"
            };
            assert_eq!(claim["level"], expected, "{importer}: {}", claim["id"]);
        }
    }
}

#[test]
fn openfeature_source_ref_contract() {
    source_ref_contract(
        "openfeature-details",
        "openfeature.evaluation-details.v1",
        "external_decision_receipt_boundary_visible",
        r#"{"schema":"openfeature.evaluation-details.export.v1","framework":"openfeature","surface":"evaluation_details","target_kind":"feature_flag","flag_key":"checkout.new_flow","result":{"value":true,"variant":"on","reason":"STATIC"}}"#,
    );
}

#[test]
fn pydantic_source_ref_contract() {
    source_ref_contract(
        "pydantic-case-result",
        "pydantic.case-result.v1",
        "",
        r#"{"schema":"pydantic-evals.report-case-result.export.v1","framework":"pydantic_evals","surface":"evaluation_report.cases.case_result","case_name":"case-hello","source_case_name":"source-hello","source_ref":"fixture:pydantic-case-results","results":[{"kind":"assertion","evaluator_name":"EqualsExpected","passed":true},{"kind":"score","evaluator_name":"ExactScorePoints","score":1.0,"reason":"maximum points"}],"timestamp":"2026-05-02T08:00:00Z"}"#,
    );
}

#[test]
fn mastra_source_ref_contract() {
    source_ref_contract(
        "mastra-score-event",
        "mastra.score-event.v1",
        "",
        r#"{"schema":"mastra.score-event.export.v1","framework":"mastra","surface":"observability.score_event","timestamp":"2026-04-30T10:31:38.858Z","score_id_ref":"f6605b31-af00-4b17-ae00-ed6262f4f411","scorer_id":"assay-scoreid-proof-scorer","score":0.91,"target_ref":"span:span-proof-001","trace_id_ref":"trace-proof-001","span_id_ref":"span-proof-001","score_trace_id_ref":"score-trace-proof-001","score_source":"live","metadata_ref":"metadata:scoreid-proof"}"#,
    );
}

#[test]
fn cyclonedx_source_ref_contract() {
    source_ref_contract(
        "cyclonedx-mlbom-model",
        "cyclonedx.mlbom-model-component.v1",
        "external_inventory_receipt_boundary_visible",
        r#"{"bomFormat":"CycloneDX","specVersion":"1.6","version":1,"components":[{"bom-ref":"model-1","type":"machine-learning-model","name":"example-model","version":"1.0.0"}]}"#,
    );
}
