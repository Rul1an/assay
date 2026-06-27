use crate::support::*;

// --- startup failures (non-zero exit; both inputs required in enforcing mode) ----------------------

#[test]
fn missing_enforce_policy_flag_fails_startup() {
    let baseline = approved_baseline_path();
    let status = run_startup(&["--declared-mcp-manifest", baseline.to_str().unwrap()]);
    assert!(
        !status.success(),
        "missing --enforce-policy must fail startup"
    );
}

#[test]
fn missing_declared_manifest_flag_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let status = run_startup(&["--enforce-policy", policy.to_str().unwrap()]);
    assert!(
        !status.success(),
        "missing --declared-mcp-manifest must fail startup in enforcing mode"
    );
}

#[test]
fn missing_policy_file_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.yaml");
    let status = run_startup(&[
        "--enforce-policy",
        missing.to_str().unwrap(),
        "--declared-mcp-manifest",
        approved_baseline_path().to_str().unwrap(),
    ]);
    assert!(!status.success(), "unreadable policy must fail startup");
}

#[test]
fn missing_caller_id_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", "allowances: []\n");
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        approved_baseline_path().to_str().unwrap(),
    ]);
    assert!(
        !status.success(),
        "policy without caller.id must fail startup"
    );
}

#[test]
fn missing_declared_manifest_file_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let missing = dir.path().join("nope.json");
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        missing.to_str().unwrap(),
    ]);
    assert!(!status.success(), "unreadable baseline must fail startup");
}

#[test]
fn wrong_schema_declared_manifest_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let baseline = write_file(
        dir.path(),
        "baseline.json",
        r#"{"schema":"assay.mcp_manifest_observed.v0","tools":[{"name":"t","tool_digest":"sha256:abc"}]}"#,
    );
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        baseline.to_str().unwrap(),
    ]);
    assert!(
        !status.success(),
        "a wrong-schema baseline must fail startup"
    );
}

#[test]
fn establish_budget_zero_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        approved_baseline_path().to_str().unwrap(),
        "--manifest-establish-budget-ms",
        "0",
    ]);
    assert!(
        !status.success(),
        "a zero establish budget must be rejected at startup"
    );
}
