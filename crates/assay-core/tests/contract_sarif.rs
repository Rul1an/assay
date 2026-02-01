use assay_core::model::{TestResultRow, TestStatus};
use assay_core::report::sarif;

#[test]
fn test_invariant_sarif_always_has_locations() {
    // Invariant: Every SARIF result must have at least one location.
    // If TestResultRow provides no location info (which it doesn't currently),
    // the writer MUST inject a synthetic one (e.g. .assay/eval.yaml).

    let results = vec![TestResultRow {
        test_id: "test_invariant_sarif".to_string(),
        status: TestStatus::Fail,
        score: None,
        cached: false,
        message: "Failed without loc".to_string(),
        details: serde_json::Value::Null,
        duration_ms: Some(100),
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }];

    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let file_path = tmp_dir.path().join("sarif.json");

    sarif::write_sarif("assay", &results, &file_path).expect("SARIF generation failed");

    let content = std::fs::read_to_string(&file_path).expect("failed to read Sarif");
    let sarif: serde_json::Value = serde_json::from_str(&content).expect("invalid json");

    let runs = sarif["runs"].as_array().expect("runs array missing");
    let results_arr = runs[0]["results"]
        .as_array()
        .expect("results array missing");

    assert!(!results_arr.is_empty(), "Result should be recorded");

    // Check invariants
    let locations = results_arr[0]["locations"]
        .as_array()
        .expect("locations missing");
    assert!(
        !locations.is_empty(),
        "SARIF result must have at least one location"
    );

    // Verify fallback URI
    let uri = locations[0]["physicalLocation"]["artifactLocation"]["uri"]
        .as_str()
        .unwrap();
    assert_eq!(uri, ".assay/eval.yaml", "Should use canonical fallback");

    // Verify region
    let region = &locations[0]["physicalLocation"]["region"];
    assert_eq!(region["startLine"], 1);
    assert_eq!(region["startColumn"], 1);
}
