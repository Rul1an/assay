use assert_cmd::Command;

const FIXTURES_DIR: &str = "../assay-evidence/tests/fixtures/incident_package";

#[cfg(unix)]
#[test]
fn fifo_without_writer_is_refused_without_waiting_for_eof() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("package.tar");
    assert!(std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    let mut cmd = Command::cargo_bin("assay").unwrap();
    let output = cmd
        .args(["evidence", "verify-incident-package"])
        .arg(&fifo)
        .timeout(std::time::Duration::from_secs(5))
        .assert()
        .code(2)
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["reason"], "input_shape");
}

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

#[test]
fn selected_outer_limit_is_inclusive_before_assessment_validation() {
    let dir = tempfile::tempdir().unwrap();
    let package = std::path::Path::new(FIXTURES_DIR).join("valid_present_empty.tar");
    let len = std::fs::metadata(&package).unwrap().len();
    for (limit, reason, code) in [
        (len, Some("stale_assessment"), 2),
        (len - 1, Some("resource_selected"), 2),
    ] {
        let mut context = assay_evidence::ContextInput::default();
        context.limits.outer_bytes = limit;
        let context_path = dir.path().join("context.json");
        std::fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
        let mut cmd = Command::cargo_bin("assay").unwrap();
        let output = cmd
            .args(["evidence", "verify-incident-package"])
            .arg(&package)
            .arg("--context")
            .arg(&context_path)
            .assert()
            .code(code)
            .get_output()
            .stdout
            .clone();
        let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(report["reason"].as_str(), reason);
    }
}

#[cfg(unix)]
#[test]
fn nonregular_inputs_are_refused_after_context_validation_and_symlinks_still_work() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("package.tar");
    assert!(std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap()
        .success());
    let context_path = dir.path().join("context.json");
    let context = assay_evidence::ContextInput {
        schema_pin: "invalid".into(),
        ..Default::default()
    };
    std::fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
    for (path, invalid, reason) in [
        (fifo.as_path(), true, "trust_input"),
        (std::path::Path::new("/dev/zero"), false, "input_shape"),
        (dir.path(), false, "input_shape"),
    ] {
        let mut cmd = Command::cargo_bin("assay").unwrap();
        cmd.args(["evidence", "verify-incident-package"]).arg(path);
        if invalid {
            cmd.arg("--context").arg(&context_path);
        }
        let output = cmd
            .timeout(std::time::Duration::from_secs(5))
            .assert()
            .code(2)
            .get_output()
            .stdout
            .clone();
        let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(report["reason"], reason);
    }
    let link = dir.path().join("link.tar");
    std::os::unix::fs::symlink(
        std::fs::canonicalize(format!("{FIXTURES_DIR}/valid_present_empty.tar")).unwrap(),
        &link,
    )
    .unwrap();
    Command::cargo_bin("assay")
        .unwrap()
        .args(["evidence", "verify-incident-package"])
        .arg(link)
        .assert()
        .success();
}
