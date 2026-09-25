use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_coverage_min_threshold_failure() {
    let dir = TempDir::new().unwrap();
    let policy_path = dir.path().join("policy.yaml");
    let trace_path = dir.path().join("trace.jsonl");

    // Write policy
    fs::write(
        &policy_path,
        r#"
version: "1"
name: threshold_policy
tools:
    allow: [ToolA, ToolB]
"#,
    )
    .unwrap();

    // Trace only calls ToolA (50% coverage)
    fs::write(
        &trace_path,
        r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
"#,
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--min-coverage")
        .arg("80") // Expect failure
        .assert()
        .failure()
        .stderr(contains("Minimum coverage not met"));
}

#[test]
fn test_coverage_min_threshold_success() {
    let dir = TempDir::new().unwrap();
    let policy_path = dir.path().join("policy.yaml");
    let trace_path = dir.path().join("trace.jsonl");

    fs::write(
        &policy_path,
        r#"
version: "1"
name: threshold_policy
tools:
    allow: [ToolA, ToolB]
"#,
    )
    .unwrap();

    // Trace calls both (100% coverage)
    fs::write(
        &trace_path,
        r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
{"type": "call_tool", "tool": "ToolB", "test_id": "test1"}
"#,
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--min-coverage")
        .arg("80")
        .assert()
        .success();
}

#[test]
fn test_coverage_baseline_regression_failure() {
    let dir = TempDir::new().unwrap();
    let policy_path = dir.path().join("policy.yaml");
    let trace_full = dir.path().join("trace_full.jsonl");
    let trace_partial = dir.path().join("trace_partial.jsonl");
    let baseline_path = dir.path().join("baseline.json");

    fs::write(
        &policy_path,
        r#"
version: "1"
name: regression_policy
tools:
    allow: [ToolA, ToolB]
"#,
    )
    .unwrap();

    // 1. Generate Baseline (100% coverage)
    fs::write(
        &trace_full,
        r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
{"type": "call_tool", "tool": "ToolB", "test_id": "test1"}
"#,
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_full)
        .arg("--export-baseline")
        .arg(&baseline_path)
        .assert()
        .success();

    // Verify baseline file exists and has content
    assert!(baseline_path.exists(), "Baseline file should exist");
    let content = fs::read_to_string(&baseline_path).unwrap();
    assert!(
        content.contains("\"metric\": \"overall\""),
        "Baseline should contain overall metric"
    );
    assert!(content.contains("100.0"), "Baseline score should be 100%");

    // 2. Run with Partial Trace (50% coverage) + Baseline Check
    fs::write(
        &trace_partial,
        r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
"#,
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_partial)
        .arg("--baseline")
        .arg(&baseline_path)
        .assert()
        .failure()
        .stderr(contains("REGRESSION DETECTED"));
}

#[test]
fn test_coverage_baseline_no_regression() {
    let dir = TempDir::new().unwrap();
    let policy_path = dir.path().join("policy.yaml");
    let trace_path = dir.path().join("trace.jsonl");
    let baseline_path = dir.path().join("baseline.json");

    fs::write(
        &policy_path,
        r#"
version: "1"
name: stable_policy
tools:
    allow: [ToolA]
"#,
    )
    .unwrap();

    // 1. Generate Baseline
    fs::write(
        &trace_path,
        r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
"#,
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--export-baseline")
        .arg(&baseline_path)
        .assert()
        .success();

    // 2. Diff against same baseline -> Should Pass
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--baseline")
        .arg(&baseline_path)
        .assert()
        .success()
        .stderr(contains("No regression against baseline"));
}

#[test]
fn test_coverage_combined_failures() {
    let dir = TempDir::new().unwrap();
    let policy_path = dir.path().join("policy.yaml");
    let trace_full = dir.path().join("trace_full.jsonl");
    let trace_bad = dir.path().join("trace_bad.jsonl");
    let baseline_path = dir.path().join("baseline.json");

    fs::write(
        &policy_path,
        r#"
version: "1"
name: combined_policy
tools:
  allow: [SafeTool]
  deny: [CriticalTool]
"#,
    )
    .unwrap();

    // 1. Establish Good Baseline (100% Coverage, No Gaps)
    // Wait, deny check is separate. If trace has SafeTool, coverage is 100%. CriticalTool is Unseen in traces.
    // If High Risk Gap logic says "If DENY tool is UNSEEN, then FAIL", then we can never have a "Good" baseline if we don't test the DENY tool?
    // Wait, High Risk Gap means "You didn't verify that CriticalTool is blocked".
    // How do verify? We need a trace where CriticalTool call is ATTEMPTED.
    // Assay Core logic: `tools_called` vs policy.
    // If trace has: `{"type": "call_tool", "tool": "CriticalTool"}`.
    // Then `CoverageAnalyzer` sees it as `tools_seen`.
    // And if config/policy says it is DENY, it counts as "Seen" (so verified?), but maybe flagged as violation?
    // `HighRiskGap` definition: `tool` in `deny` AND `tool` NOT in `seen`.
    // So to avoid Gap, we MUST see it.

    // Let's create a trace where we attempt CriticalTool (so gap is closed), but that might be a violation?
    // Assay currently doesn't fail on violations in `coverage` report (logic is thresholding & gaps).
    // Violations are checked in `run/eval`.

    fs::write(
        &trace_full,
        r#"
{"type": "call_tool", "tool": "SafeTool", "test_id": "test1"}
{"type": "call_tool", "tool": "CriticalTool", "test_id": "test2"}
"#,
    )
    .unwrap();

    // Export baseline
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_full)
        .arg("--export-baseline")
        .arg(&baseline_path)
        .assert()
        .failure();

    // 2. Bad Trace:
    // - SafeTool missing (Regression + Low Coverage)
    // - CriticalTool missing (High Risk Gap)
    fs::write(&trace_bad, "").unwrap(); // Empty trace -> 0 coverage, Gap

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path) // Needs policy to know DENY list
        .arg("--traces")
        .arg(&trace_bad)
        .arg("--baseline")
        .arg(&baseline_path) // Check regression
        .arg("--min-coverage")
        .arg("80") // Check threshold
        .assert()
        .failure()
        .stderr(contains("REGRESSION DETECTED"))
        .stderr(contains("High Risk Gaps Detected"))
        .stderr(contains("Minimum coverage not met"));
}

fn write_empty_policy(dir: &TempDir) -> std::path::PathBuf {
    let policy_path = dir.path().join("empty_policy.yaml");
    fs::write(
        &policy_path,
        r#"
version: "1"
name: empty_policy
tools: {}
"#,
    )
    .unwrap();
    policy_path
}

fn write_tools_only_policy(dir: &TempDir) -> std::path::PathBuf {
    let policy_path = dir.path().join("policy.yaml");
    fs::write(
        &policy_path,
        r#"
version: "1"
name: tools_only_policy
tools:
    allow: [ToolA, ToolB]
"#,
    )
    .unwrap();
    policy_path
}

fn write_full_trace(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let trace_path = dir.path().join(name);
    fs::write(
        &trace_path,
        r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
{"type": "call_tool", "tool": "ToolB", "test_id": "test1"}
"#,
    )
    .unwrap();
    trace_path
}

/// Matrix row 5 through the CLI: a policy declaring no tools and no sequence
/// rules refuses the threshold at the default 0.0 with a stated reason.
#[test]
fn test_coverage_empty_policy_refuses_threshold_zero() {
    let dir = TempDir::new().unwrap();
    let policy_path = write_empty_policy(&dir);
    let trace_path = dir.path().join("trace.jsonl");
    fs::write(
        &trace_path,
        r#"
{"type": "episode_end", "test_id": "test1"}
"#,
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--format")
        .arg("text")
        .assert()
        .failure()
        .stderr(contains(
            "Coverage not applicable: policy declares no tools and no rules",
        ));
}

/// Matrix row 6: exporting from a fully covered tools-only policy keeps all
/// three entries, with the n/a rule dimension scoring 0 and marked not_applicable.
#[test]
fn test_coverage_export_marks_empty_rule_dimension_not_applicable() {
    let dir = TempDir::new().unwrap();
    let policy_path = write_tools_only_policy(&dir);
    let trace_path = write_full_trace(&dir, "trace_full.jsonl");
    let baseline_path = dir.path().join("baseline.json");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--export-baseline")
        .arg(&baseline_path)
        .assert()
        .success();

    let content = fs::read_to_string(&baseline_path).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(&content).unwrap();
    let entries = baseline["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "export keeps all three entries");
    for entry in entries {
        let metric = entry["metric"].as_str().unwrap();
        let score = entry["score"].as_f64().unwrap();
        let exercised = entry.get("meta").and_then(|m| m.get("exercised"));
        match metric {
            "overall" => {
                assert_eq!(score, 100.0);
                assert!(exercised.is_none());
            }
            "tool" => {
                assert_eq!(score, 100.0);
                assert!(exercised.is_none());
            }
            "rule" => {
                assert_eq!(score, 0.0);
                assert_eq!(exercised.and_then(|v| v.as_str()), Some("not_applicable"));
            }
            other => panic!("unexpected metric {other}"),
        }
    }
}

fn write_baseline(dir: &TempDir, suite: &str, entries: &serde_json::Value) -> std::path::PathBuf {
    let baseline_path = dir.path().join("baseline.json");
    let baseline = serde_json::json!({
        "schema_version": 1,
        "suite": suite,
        "assay_version": "test",
        "created_at": "2026-01-01T00:00:00Z",
        "config_fingerprint": "fp",
        "entries": entries,
    });
    fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&baseline).unwrap(),
    )
    .unwrap();
    baseline_path
}

fn applicable_entry(metric: &str, score: f64) -> serde_json::Value {
    serde_json::json!({"test_id": "coverage", "metric": metric, "score": score})
}

fn not_applicable_entry(metric: &str, score: f64) -> serde_json::Value {
    serde_json::json!({
        "test_id": "coverage",
        "metric": metric,
        "score": score,
        "meta": {"exercised": "not_applicable"},
    })
}

/// Matrix row 7: an old baseline (empty dimension recorded as 100, no meta)
/// compared against the corrected run prints one REGRESSION line with the
/// re-export hint and exits 1. The accepted one-time migration cost.
#[test]
fn test_coverage_compare_against_old_baseline_regresses_on_rule() {
    let dir = TempDir::new().unwrap();
    let policy_path = write_tools_only_policy(&dir);
    let trace_path = write_full_trace(&dir, "trace_full.jsonl");
    let baseline_path = write_baseline(
        &dir,
        "policy",
        &serde_json::json!([
            applicable_entry("overall", 100.0),
            applicable_entry("tool", 100.0),
            applicable_entry("rule", 100.0),
        ]),
    );

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--baseline")
        .arg(&baseline_path)
        .assert()
        .failure()
        .stderr(contains("REGRESSION DETECTED"))
        .stderr(contains(
            "'rule' is not applicable in the current policy (0 rules declared)",
        ))
        .stderr(contains("Re-export with --export-baseline if intentional"));
}

/// Matrix row 8: a formerly applicable rule dimension that disappears reads as
/// a regression (60 -> 0) with the not-applicable sentence, never silently clean.
#[test]
fn test_coverage_applicable_to_not_applicable_is_a_failing_compare() {
    let dir = TempDir::new().unwrap();
    let policy_path = write_tools_only_policy(&dir);
    let trace_path = write_full_trace(&dir, "trace_full.jsonl");
    let baseline_path = write_baseline(
        &dir,
        "policy",
        &serde_json::json!([
            applicable_entry("overall", 80.0),
            applicable_entry("tool", 100.0),
            applicable_entry("rule", 60.0),
        ]),
    );

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--baseline")
        .arg(&baseline_path)
        .assert()
        .failure()
        .stderr(contains("REGRESSION DETECTED"))
        .stderr(contains("60.00"))
        .stderr(contains(
            "'rule' is not applicable in the current policy (0 rules declared)",
        ));
}

/// Matrix row 9: a baseline entry recorded as not_applicable that becomes
/// applicable again is announced as newly applicable, not as improved coverage.
///
/// (The CLI never populates rule ids from traces, so an end-to-end rule
/// transition cannot be produced here; the tool direction exercises the same
/// diff-loop branch.)
#[test]
fn test_coverage_not_applicable_to_applicable_is_announced() {
    let dir = TempDir::new().unwrap();
    let policy_path = write_tools_only_policy(&dir);
    let trace_path = write_full_trace(&dir, "trace_full.jsonl");
    let baseline_path = write_baseline(
        &dir,
        "policy",
        &serde_json::json!([
            not_applicable_entry("overall", 0.0),
            not_applicable_entry("tool", 0.0),
            not_applicable_entry("rule", 0.0),
        ]),
    );

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--baseline")
        .arg(&baseline_path)
        .assert()
        .success()
        .stderr(contains("newly applicable"));
}

/// Matrix row 10: exporting from an empty policy writes all three entries as
/// 0 / not_applicable, and the run itself still exits 1.
#[test]
fn test_coverage_export_from_empty_policy_marks_all_not_applicable() {
    let dir = TempDir::new().unwrap();
    let policy_path = write_empty_policy(&dir);
    let trace_path = dir.path().join("trace.jsonl");
    fs::write(
        &trace_path,
        r#"
{"type": "episode_end", "test_id": "test1"}
"#,
    )
    .unwrap();
    let baseline_path = dir.path().join("baseline.json");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .arg("--export-baseline")
        .arg(&baseline_path)
        .assert()
        .failure();

    let content = fs::read_to_string(&baseline_path).unwrap();
    let baseline: serde_json::Value = serde_json::from_str(&content).unwrap();
    let entries = baseline["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3);
    for entry in entries {
        assert_eq!(entry["score"].as_f64().unwrap(), 0.0);
        assert_eq!(
            entry["meta"]["exercised"].as_str(),
            Some("not_applicable"),
            "metric {} must be marked not_applicable",
            entry["metric"],
        );
    }
}

#[test]
fn test_coverage_high_risk_gap_failure() {
    let dir = TempDir::new().unwrap();
    let policy_path = dir.path().join("policy.yaml");
    let trace_path = dir.path().join("trace.jsonl");

    fs::write(
        &policy_path,
        r#"
version: "1"
name: strict_policy
tools:
  allow: [SafeTool]
  deny: [CriticalTool]
"#,
    )
    .unwrap();

    // Trace only safe tool -> CriticalTool is UNSEEN -> High Risk Gap
    fs::write(
        &trace_path,
        r#"
{"type": "call_tool", "tool": "SafeTool", "test_id": "test1"}
"#,
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.arg("coverage")
        .arg("--policy")
        .arg(&policy_path)
        .arg("--traces")
        .arg(&trace_path)
        .assert()
        .failure()
        .stderr(contains("High Risk Gaps Detected"));
}
