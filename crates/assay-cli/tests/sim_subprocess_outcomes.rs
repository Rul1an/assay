//! Exercise the production sim -> report -> exit path, with only the verifier child replaced.
#![cfg(all(unix, feature = "sim"))]

use assert_cmd::Command;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

fn run(verifier: &str) -> (std::process::Output, serde_json::Value) {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("verifier");
    std::fs::write(&script, format!("#!/bin/sh\n{verifier}\n")).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let report = dir.path().join("report.json");
    let output = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "sim",
            "run",
            "--suite",
            "chaos",
            "--target",
            "unused",
            "--seed",
            "42",
            "--time-budget",
            "60",
            "--limits",
            r#"{"max_bundle_bytes":5242880,"max_decode_bytes":33554432}"#,
            "--report",
        ])
        .arg(&report)
        .env("ASSAY_BIN", script)
        .timeout(Duration::from_secs(90))
        .output()
        .unwrap();
    let report = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    (output, report)
}

#[test]
fn verifier_signal_is_infrastructure_failure_through_the_cli() {
    let (output, report) = run("kill -TERM $$");
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let identity = report["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "differential.parity.identity")
        .unwrap();
    assert_eq!(identity["status"], "Error");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("All attacks blocked"));
}

#[test]
fn parity_failure_preserves_json_status_but_exits_nonzero() {
    let (output, report) = run("exit 0");
    assert_eq!(output.status.code(), Some(1));
    assert!(report["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["status"] == "Failed" && r["error_code"] == "SOTA_BYPASS"));
    assert!(report["summary"]["failed"].as_u64().unwrap() > 0);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("All attacks blocked"));
}
