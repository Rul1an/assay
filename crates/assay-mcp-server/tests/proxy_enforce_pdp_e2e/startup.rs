use crate::support::*;

// --- startup failures (non-zero exit; both inputs required in enforcing mode) ----------------------

fn assert_machine_startup_failure(extra: &[&str], reason_code: &str, next_step: &str) {
    let output = run_startup_output(extra);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "startup failure polluted MCP stdout"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr must be UTF-8");
    let events: Vec<serde_json::Value> = stderr
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|event: &serde_json::Value| event["event"] == "startup_failure")
        .collect();
    assert_eq!(
        events.len(),
        1,
        "stderr did not carry one startup_failure: {stderr}"
    );
    assert_eq!(events[0]["reason_code"], reason_code);
    assert_eq!(events[0]["next_step"], next_step);
    assert!(
        !stderr.contains("\"event\":\"proxy_start\""),
        "failed startup claimed that the proxy started: {stderr}"
    );
}

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
fn missing_policy_file_emits_one_machine_readable_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.yaml");
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            missing.to_str().unwrap(),
            "--declared-mcp-manifest",
            approved_baseline_path().to_str().unwrap(),
        ],
        "proxy_enforce_policy_invalid",
        "Check --enforce-policy and retry proxy-enforce.",
    );
}

#[test]
fn malformed_policy_file_emits_policy_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", "caller: [");
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            approved_baseline_path().to_str().unwrap(),
        ],
        "proxy_enforce_policy_invalid",
        "Check --enforce-policy and retry proxy-enforce.",
    );
}

#[test]
fn malformed_manifest_emits_manifest_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let malformed = write_file(dir.path(), "malformed.json", "{");

    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            malformed.to_str().unwrap(),
        ],
        "proxy_declared_manifest_invalid",
        "Check --declared-mcp-manifest and retry proxy-enforce.",
    );
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
fn missing_declared_manifest_file_emits_manifest_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let missing = dir.path().join("nope.json");
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            missing.to_str().unwrap(),
        ],
        "proxy_declared_manifest_invalid",
        "Check --declared-mcp-manifest and retry proxy-enforce.",
    );
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
fn establish_budget_zero_emits_budget_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            approved_baseline_path().to_str().unwrap(),
            "--manifest-establish-budget-ms",
            "0",
        ],
        "proxy_establish_budget_invalid",
        "Set --manifest-establish-budget-ms above 0 and retry proxy-enforce.",
    );
}
