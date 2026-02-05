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

/// E2.3: Cheap deterministic truncation test — small limit (10), 25 eligible results → omitted=15, included=10.
/// Uses write_sarif_with_limit to avoid generating 25k results. Asserts run-level properties.assay and ordering.
#[test]
fn test_sarif_truncation_properties() {
    use assay_core::report::sarif::{is_sarif_eligible, write_sarif_with_limit};

    const MAX_RESULTS: usize = 10;
    let n_eligible = 25;
    let results: Vec<TestResultRow> = (0..n_eligible)
        .map(|i| TestResultRow {
            test_id: format!("test_{:02}", i),
            status: TestStatus::Fail,
            score: None,
            cached: false,
            message: "fail".to_string(),
            details: serde_json::Value::Null,
            duration_ms: None,
            fingerprint: None,
            skip_reason: None,
            attempts: None,
            error_policy_applied: None,
        })
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|r| is_sarif_eligible(r.status))
            .count(),
        n_eligible
    );

    let tmp_dir = tempfile::tempdir().expect("temp dir");
    let path = tmp_dir.path().join("out.json");
    let outcome = write_sarif_with_limit("assay", &results, &path, MAX_RESULTS)
        .expect("write_sarif_with_limit");
    let expected_omitted = (n_eligible - MAX_RESULTS) as u64;
    assert_eq!(outcome.omitted_count, expected_omitted);

    let content = std::fs::read_to_string(&path).expect("read");
    let doc: serde_json::Value = serde_json::from_str(&content).expect("json");
    let run = &doc["runs"][0];
    let props = run.get("properties").and_then(|p| p.get("assay"));
    assert!(
        props.is_some(),
        "runs[0].properties.assay must be present when truncated"
    );
    assert_eq!(props.unwrap()["truncated"], true);
    assert_eq!(props.unwrap()["omitted_count"], expected_omitted);

    let results_arr = run["results"].as_array().unwrap();
    assert_eq!(results_arr.len(), MAX_RESULTS);

    // Ordering: deterministic (BlockingRank, SeverityRank, test_id) → first is blocking + lowest test_id
    let first_msg = results_arr[0]["message"]["text"].as_str().unwrap();
    assert!(
        first_msg.starts_with("test_00:"),
        "first result must be lowest test_id (deterministic sort): got {}",
        first_msg
    );
}

/// E2.3: Mixed-status ordering — blocking (Fail/Error) before Warn, within bucket by test_id.
#[test]
fn test_sarif_mixed_status_ordering() {
    use assay_core::report::sarif::write_sarif_with_limit;

    // 2× Error, 2× Fail, 2× Warn with interleaved test_ids; take all 6 so no truncation
    let results = vec![
        (TestStatus::Warn, "w_z"),
        (TestStatus::Error, "e_a"),
        (TestStatus::Fail, "f_m"),
        (TestStatus::Warn, "w_b"),
        (TestStatus::Fail, "f_c"),
        (TestStatus::Error, "e_d"),
    ]
    .into_iter()
    .map(|(status, test_id)| TestResultRow {
        test_id: test_id.to_string(),
        status,
        score: None,
        cached: false,
        message: "x".to_string(),
        details: serde_json::Value::Null,
        duration_ms: None,
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    })
    .collect::<Vec<_>>();

    let tmp_dir = tempfile::tempdir().expect("temp dir");
    let path = tmp_dir.path().join("out.json");
    write_sarif_with_limit("assay", &results, &path, 10).expect("write_sarif_with_limit");

    let content = std::fs::read_to_string(&path).expect("read");
    let doc: serde_json::Value = serde_json::from_str(&content).expect("json");
    let results_arr = doc["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results_arr.len(), 6);

    // Order: blocking (Error/Fail) first by test_id, then Warn by test_id → e_a, e_d, f_c, f_m, w_b, w_z
    let texts: Vec<&str> = results_arr
        .iter()
        .map(|r| r["message"]["text"].as_str().unwrap())
        .collect();
    assert!(
        texts[0].starts_with("e_a:"),
        "first must be blocking+lowest test_id: {:?}",
        texts[0]
    );
    assert!(texts[1].starts_with("e_d:"));
    assert!(texts[2].starts_with("f_c:"));
    assert!(texts[3].starts_with("f_m:"));
    assert!(texts[4].starts_with("w_b:"));
    assert!(
        texts[5].starts_with("w_z:"),
        "last must be warn+highest test_id: {:?}",
        texts[5]
    );
}

/// E2.3: Default limit yields no truncation metadata when under limit.
#[test]
fn test_sarif_no_truncation_under_limit() {
    use assay_core::report::sarif::write_sarif;

    let results = vec![TestResultRow {
        test_id: "one".to_string(),
        status: TestStatus::Fail,
        score: None,
        cached: false,
        message: "x".to_string(),
        details: serde_json::Value::Null,
        duration_ms: None,
        fingerprint: None,
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }];

    let tmp_dir = tempfile::tempdir().expect("temp dir");
    let path = tmp_dir.path().join("out.json");
    let outcome = write_sarif("assay", &results, &path).expect("write_sarif");
    assert_eq!(outcome.omitted_count, 0);

    let content = std::fs::read_to_string(&path).expect("read");
    let doc: serde_json::Value = serde_json::from_str(&content).expect("json");
    let run = &doc["runs"][0];
    assert!(
        run.get("properties").is_none(),
        "no properties when not truncated"
    );
}
