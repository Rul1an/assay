use assert_cmd::Command;

const FIXTURES_DIR: &str = "../assay-evidence/tests/fixtures/incident_package";

#[test]
fn test_cli_verify_incident_package_valid() {
    let pkg_path = format!("{FIXTURES_DIR}/valid_present_empty.tar");
    let mut cmd = Command::cargo_bin("assay").expect("binary exists");
    cmd.args([
        "evidence",
        "verify-incident-package",
        &pkg_path,
        "--format",
        "json",
    ]);

    let assert = cmd.assert().success(); // exit code 0
    let output = assert.get_output();
    let stdout_str = std::str::from_utf8(&output.stdout).expect("valid utf-8");

    let val: serde_json::Value = serde_json::from_str(stdout_str).expect("valid json output");
    assert_eq!(val["schema"], "assay.incident.verify.v1");
    assert_eq!(val["outcome"], "package_verified");
    assert!(val["reason"].is_null());
    assert_eq!(val["expectation"], "not_requested");
    assert_eq!(
        val["inventory_sha256"],
        "747ae82961390332a1a203d8a6f7d8ffc3d7ecee77825c5c470dca627138e99b"
    );
    assert_eq!(
        val["assessment_sha256"],
        "921dda0ca89d242f89ed1450748a3934335103cc4fdb34dc731969f53bd7397c"
    );
}

#[test]
fn test_cli_verify_incident_package_refused() {
    let pkg_path = format!("{FIXTURES_DIR}/refusal_p3_unknown_format.tar");
    let mut cmd = Command::cargo_bin("assay").expect("binary exists");
    cmd.args([
        "evidence",
        "verify-incident-package",
        &pkg_path,
        "--format",
        "json",
    ]);

    let assert = cmd.assert().code(2); // exit code 2
    let output = assert.get_output();
    let stdout_str = std::str::from_utf8(&output.stdout).expect("valid utf-8");

    let val: serde_json::Value = serde_json::from_str(stdout_str).expect("valid json output");
    assert_eq!(val["schema"], "assay.incident.verify.v1");
    assert_eq!(val["outcome"], "package_refused");
    assert_eq!(val["reason"], "unknown_format");
}

#[test]
fn test_cli_verify_incident_package_unavailable() {
    let pkg_path = "non_existent_package.tar";
    let mut cmd = Command::cargo_bin("assay").expect("binary exists");
    cmd.args([
        "evidence",
        "verify-incident-package",
        pkg_path,
        "--format",
        "json",
    ]);

    let assert = cmd.assert().code(2); // exit code 2
    let output = assert.get_output();
    let stdout_str = std::str::from_utf8(&output.stdout).expect("valid utf-8");

    let val: serde_json::Value = serde_json::from_str(stdout_str).expect("valid json output");
    assert_eq!(val["schema"], "assay.incident.verify.v1");
    assert_eq!(val["outcome"], "verification_unavailable");
    assert_eq!(val["reason"], "io_unavailable");
}
