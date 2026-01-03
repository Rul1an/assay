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
    fs::write(&policy_path, r#"
version: "1"
name: threshold_policy
tools:
    allow: [ToolA, ToolB]
"#).unwrap();

    // Trace only calls ToolA (50% coverage)
    fs::write(&trace_path, r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
"#).unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
       .arg("--policy").arg(&policy_path)
       .arg("--traces").arg(&trace_path)
       .arg("--min-coverage").arg("80") // Expect failure
       .assert()
       .failure()
       .stderr(contains("Minimum coverage not met"));
}

#[test]
fn test_coverage_min_threshold_success() {
    let dir = TempDir::new().unwrap();
    let policy_path = dir.path().join("policy.yaml");
    let trace_path = dir.path().join("trace.jsonl");

    fs::write(&policy_path, r#"
version: "1"
name: threshold_policy
tools:
    allow: [ToolA, ToolB]
"#).unwrap();

    // Trace calls both (100% coverage)
    fs::write(&trace_path, r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
{"type": "call_tool", "tool": "ToolB", "test_id": "test1"}
"#).unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
       .arg("--policy").arg(&policy_path)
       .arg("--traces").arg(&trace_path)
       .arg("--min-coverage").arg("80")
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

    fs::write(&policy_path, r#"
version: "1"
name: regression_policy
tools:
    allow: [ToolA, ToolB]
"#).unwrap();

    // 1. Generate Baseline (100% coverage)
    fs::write(&trace_full, r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
{"type": "call_tool", "tool": "ToolB", "test_id": "test1"}
"#).unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
       .arg("--policy").arg(&policy_path)
       .arg("--traces").arg(&trace_full)
       .arg("--export-baseline").arg(&baseline_path)
       .assert()
       .success();

    // Verify baseline file exists and has content
    assert!(baseline_path.exists(), "Baseline file should exist");
    let content = fs::read_to_string(&baseline_path).unwrap();
    assert!(content.contains("\"metric\": \"overall\""), "Baseline should contain overall metric");
    assert!(content.contains("100.0"), "Baseline score should be 100%");

    // 2. Run with Partial Trace (50% coverage) + Baseline Check
    fs::write(&trace_partial, r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
"#).unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
       .arg("--policy").arg(&policy_path)
       .arg("--traces").arg(&trace_partial)
       .arg("--baseline").arg(&baseline_path)
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

    fs::write(&policy_path, r#"
version: "1"
name: stable_policy
tools:
    allow: [ToolA]
"#).unwrap();

    // 1. Generate Baseline
    fs::write(&trace_path, r#"
{"type": "call_tool", "tool": "ToolA", "test_id": "test1"}
"#).unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
       .arg("--policy").arg(&policy_path)
       .arg("--traces").arg(&trace_path)
       .arg("--export-baseline").arg(&baseline_path)
       .assert()
       .success();

    // 2. Diff against same baseline -> Should Pass
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
       .arg("--policy").arg(&policy_path)
       .arg("--traces").arg(&trace_path)
       .arg("--baseline").arg(&baseline_path)
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

    fs::write(&policy_path, r#"
version: "1"
name: combined_policy
tools:
  allow: [SafeTool]
  deny: [CriticalTool]
"#).unwrap();

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

    fs::write(&trace_full, r#"
{"type": "call_tool", "tool": "SafeTool", "test_id": "test1"}
{"type": "call_tool", "tool": "CriticalTool", "test_id": "test2"}
"#).unwrap();

    // Export baseline
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
       .arg("--policy").arg(&policy_path)
       .arg("--traces").arg(&trace_full)
       .arg("--export-baseline").arg(&baseline_path)
       .assert()
       .success();

    // 2. Bad Trace:
    // - SafeTool missing (Regression + Low Coverage)
    // - CriticalTool missing (High Risk Gap)
    fs::write(&trace_bad, "").unwrap(); // Empty trace -> 0 coverage, Gap

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
       .arg("--policy").arg(&policy_path) // Needs policy to know DENY list
       .arg("--traces").arg(&trace_bad)
       .arg("--baseline").arg(&baseline_path) // Check regression
       .arg("--min-coverage").arg("80") // Check threshold
       .assert()
       .failure()
       .stderr(contains("REGRESSION DETECTED"))
       .stderr(contains("High Risk Gaps Detected"))
       .stderr(contains("Minimum coverage not met"));
}

#[test]
fn test_coverage_high_risk_gap_failure() {
    let dir = TempDir::new().unwrap();
    let policy_path = dir.path().join("policy.yaml");
    let trace_path = dir.path().join("trace.jsonl");

     fs::write(&policy_path, r#"
version: "1"
name: strict_policy
tools:
  allow: [SafeTool]
  deny: [CriticalTool]
"#).unwrap();

    // Trace only safe tool -> CriticalTool is UNSEEN -> High Risk Gap
    fs::write(&trace_path, r#"
{"type": "call_tool", "tool": "SafeTool", "test_id": "test1"}
"#).unwrap();

    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("coverage")
       .arg("--policy").arg(&policy_path)
       .arg("--traces").arg(&trace_path)
       .assert()
       .failure()
       .stderr(contains("High Risk Gaps Detected"));
}
