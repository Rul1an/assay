use assay_core::model::{TestResultRow, TestStatus};
use assay_core::report::sarif;

#[test]
fn test_generate_sourceless_sarif_fixture() {
    let results = vec![TestResultRow {
        test_id: "sourceless_failure_demo".to_string(),
        status: TestStatus::Fail,
        score: Some(0.0),
        cached: false,
        message: "Config error or internal logic failure without file context".to_string(),
        details: serde_json::json!({"error": "trace_not_found"}),
        duration_ms: Some(10),
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }];

    // This file is generated for the Review Pack artifact
    let file_path = std::path::PathBuf::from(".repro_artifacts/sourceless_sarif.json");
    // Ensure dir exists
    if let Some(p) = file_path.parent() {
        std::fs::create_dir_all(p).unwrap();
    }

    sarif::write_sarif("assay", &results, &file_path).expect("SARIF generation failed");

    // Verify it matches expectations
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains(".assay/eval.yaml"),
        "Must contain fallback URI"
    );
    println!("Generated fixture at {}", file_path.display());
}
