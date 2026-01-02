//! End-to-end tests for Assay CLI commands
//!
//! Tests the full CLI workflow: check, coverage, explain, baseline

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn assay_cmd() -> Command {
    Command::cargo_bin("assay").unwrap()
}

fn create_policy(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("policy.yaml");
    fs::write(&path, r#"
version: "1.1"
name: "test-policy"
tools:
  allow:
    - Search
    - Create
    - Update
  deny:
    - Delete
sequences:
  - type: before
    first: Search
    then: Create
  - type: max_calls
    tool: Search
    max: 3
"#).unwrap();
    path
}

fn create_traces(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("traces.jsonl");
    fs::write(&path, r#"{"tools": ["Search", "Create"]}
{"tools": ["Search", "Update"]}
{"tools": ["Search", "Create", "Update"]}
"#).unwrap();
    path
}

fn create_trace_single(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("trace.json");
    fs::write(&path, r#"[
        {"tool": "Search"},
        {"tool": "Create"},
        {"tool": "Update"}
    ]"#).unwrap();
    path
}

// ==================== VERSION ====================

#[test]
fn test_version() {
    assay_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("assay"));
}

#[test]
fn test_help() {
    assay_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Deterministic policy enforcement"));
}

// ==================== COVERAGE COMMAND ====================

#[test]
fn test_coverage_basic() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let traces = create_traces(&dir);
    
    assay_cmd()
        .args(["coverage", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .assert()
        .success();
}

#[test]
fn test_coverage_json_format() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let traces = create_traces(&dir);
    
    assay_cmd()
        .args(["coverage", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .args(["--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("overall_coverage_pct"));
}

#[test]
fn test_coverage_threshold_pass() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let traces = create_traces(&dir);
    
    assay_cmd()
        .args(["coverage", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .args(["--min-coverage", "50"])
        .assert()
        .success();
}

#[test]
fn test_coverage_threshold_fail() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let traces = create_traces(&dir);
    
    assay_cmd()
        .args(["coverage", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .args(["--min-coverage", "100"])
        .assert()
        .code(1);
}

#[test]
fn test_coverage_github_format() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let traces = create_traces(&dir);
    
    assay_cmd()
        .args(["coverage", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .args(["--format", "github"])
        .assert()
        .success();
}

// ==================== EXPLAIN COMMAND ====================

#[test]
fn test_explain_basic() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let trace = create_trace_single(&dir);
    
    assay_cmd()
        .args(["explain", "--policy"])
        .arg(&policy)
        .args(["--trace"])
        .arg(&trace)
        .assert()
        .success()
        .stdout(predicate::str::contains("Timeline"));
}

#[test]
fn test_explain_json_format() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let trace = create_trace_single(&dir);
    
    assay_cmd()
        .args(["explain", "--policy"])
        .arg(&policy)
        .args(["--trace"])
        .arg(&trace)
        .args(["--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("steps"));
}

#[test]
fn test_explain_markdown_format() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let trace = create_trace_single(&dir);
    
    assay_cmd()
        .args(["explain", "--policy"])
        .arg(&policy)
        .args(["--trace"])
        .arg(&trace)
        .args(["--format", "markdown"])
        .assert()
        .success()
        .stdout(predicate::str::contains("## Trace Explanation"));
}

#[test]
fn test_explain_blocked_trace() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    
    // Create trace that violates before rule
    let trace = dir.path().join("bad-trace.json");
    fs::write(&trace, r#"[{"tool": "Create"}]"#).unwrap();
    
    assay_cmd()
        .args(["explain", "--policy"])
        .arg(&policy)
        .args(["--trace"])
        .arg(&trace)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("BLOCKED"));
}

#[test]
fn test_explain_experimental_flag_ignored() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let trace = create_trace_single(&dir);
    
    // --experimental should be silently ignored (backward compat)
    assay_cmd()
        .args(["explain", "--experimental", "--policy"])
        .arg(&policy)
        .args(["--trace"])
        .arg(&trace)
        .assert()
        .success();
}

// ==================== BASELINE COMMANDS ====================

#[test]
fn test_baseline_save() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let traces = create_traces(&dir);
    let baseline = dir.path().join("baseline.yaml");
    
    assay_cmd()
        .args(["baseline", "save", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .args(["--output"])
        .arg(&baseline)
        .args(["--git-auto", "false"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Baseline saved"));
    
    assert!(baseline.exists());
}

#[test]
fn test_baseline_show() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let traces = create_traces(&dir);
    let baseline = dir.path().join("baseline.yaml");
    
    // First save
    assay_cmd()
        .args(["baseline", "save", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .args(["--output"])
        .arg(&baseline)
        .args(["--git-auto", "false"])
        .assert()
        .success();
    
    // Then show
    assay_cmd()
        .args(["baseline", "show", "--baseline"])
        .arg(&baseline)
        .assert()
        .success()
        .stdout(predicate::str::contains("Coverage:"));
}

#[test]
fn test_baseline_diff_no_regression() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let traces = create_traces(&dir);
    let baseline = dir.path().join("baseline.yaml");
    
    // Save baseline
    assay_cmd()
        .args(["baseline", "save", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .args(["--output"])
        .arg(&baseline)
        .args(["--git-auto", "false"])
        .assert()
        .success();
    
    // Diff against same traces - no regression
    assay_cmd()
        .args(["baseline", "diff", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .args(["--baseline"])
        .arg(&baseline)
        .assert()
        .success();
}

#[test]
fn test_baseline_missing_file() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    let traces = create_traces(&dir);
    
    assay_cmd()
        .args(["baseline", "diff", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .args(["--baseline", "/nonexistent/baseline.yaml"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Error loading baseline"));
}

// ==================== ERROR CASES ====================

#[test]
fn test_missing_policy() {
    let dir = TempDir::new().unwrap();
    let traces = create_traces(&dir);
    
    assay_cmd()
        .args(["coverage", "--policy", "/nonexistent.yaml", "--traces"])
        .arg(&traces)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Error"));
}

#[test]
fn test_invalid_policy() {
    let dir = TempDir::new().unwrap();
    let policy = dir.path().join("bad.yaml");
    fs::write(&policy, "not: valid: yaml: {{").unwrap();
    let traces = create_traces(&dir);
    
    assay_cmd()
        .args(["coverage", "--policy"])
        .arg(&policy)
        .args(["--traces"])
        .arg(&traces)
        .assert()
        .code(2);
}

#[test]
fn test_missing_traces() {
    let dir = TempDir::new().unwrap();
    let policy = create_policy(&dir);
    
    assay_cmd()
        .args(["coverage", "--policy"])
        .arg(&policy)
        .args(["--traces", "/nonexistent/traces.jsonl"])
        .assert()
        .code(2);
}
