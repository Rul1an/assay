#![allow(deprecated)]
//! E1.1 soak: CLI contract for ADR-025 E1.1 — two-mode, clap constraints, stub coverage.
//! Goal: assert CLI contract without brittle full-output matching.
//! - (1) Clap-enforced errors (missing required, conflicts)
//! - (1b) Runtime stub path for --mode=run when run_cmd present

use assert_cmd::Command;
use std::path::Path;

fn bundle_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/evidence/test-bundle.tar.gz")
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected output to contain {needle:?}\n--- output ---\n{haystack}\n--- end ---"
    );
}

#[test]
fn soak_mode_run_requires_run_cmd_clap_error() {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    let output = cmd.args(["sim", "soak", "--mode=run"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        code == 2 || code == 1,
        "expected clap usage exit, got {code}\n{stderr}"
    );
    assert_contains(&stderr.to_lowercase(), "required");
    assert_contains(&stderr.to_lowercase(), "run-cmd");
}

#[test]
fn soak_mode_run_stub_hits_runtime_message() {
    let mut cmd = Command::cargo_bin("assay").unwrap();
    let output = cmd
        .args([
            "sim",
            "soak",
            "--mode=run",
            "--run-cmd",
            "echo noop",
            "--iterations",
            "1",
        ])
        .output()
        .unwrap();

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        code, 2,
        "expected config error exit 2; got {code}\n{stderr}"
    );
    assert_contains(&stderr, "Config error:");
    assert_contains(&stderr, "--mode=run is not implemented yet");
    assert_contains(&stderr, "ADR-025");
    assert_contains(&stderr, "E1.2");
}

#[test]
fn soak_mode_artifact_requires_target() {
    // Default mode=artifact; without --target → runtime or clap error
    let mut cmd = Command::cargo_bin("assay").unwrap();
    let output = cmd
        .args(["sim", "soak", "--iterations", "3"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        code == 2 || code == 1,
        "expected usage/config exit, got {code}\n{stderr}"
    );
    assert_contains(&stderr.to_lowercase(), "target");
    assert_contains(&stderr.to_lowercase(), "required");
}

#[test]
fn soak_target_conflicts_with_run_cmd_clap_error() {
    let bundle = bundle_fixture();
    if !bundle.exists() {
        return;
    }

    let mut cmd = Command::cargo_bin("assay").unwrap();
    let output = cmd
        .args([
            "sim",
            "soak",
            "--mode=artifact",
            "--target",
            bundle.to_str().unwrap(),
            "--run-cmd",
            "echo noop",
            "--iterations",
            "1",
        ])
        .output()
        .unwrap();

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lc = stderr.to_lowercase();

    assert!(
        code == 2 || code == 1,
        "expected clap conflict exit, got {code}\n{stderr}"
    );
    assert_contains(&lc, "run-cmd");
    assert_contains(&lc, "target");
    assert!(
        lc.contains("cannot be used with") || lc.contains("conflicts"),
        "expected conflict phrasing; got:\n{stderr}"
    );
}

#[test]
fn soak_artifact_quiet_suppresses_warning_exits_ok_json_contract() {
    let bundle = bundle_fixture();
    if !bundle.exists() {
        return;
    }

    let mut cmd = Command::cargo_bin("assay").unwrap();
    let output = cmd
        .args([
            "sim",
            "soak",
            "--mode=artifact",
            "--target",
            bundle.to_str().unwrap(),
            "--iterations",
            "1",
            "--quiet",
            "--report",
            "-",
        ])
        .output()
        .unwrap();

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(code, 0, "expected success; got {code}\n{stderr}");

    assert!(
        !stderr.contains("Note: --mode=artifact"),
        "expected no artifact warning under --quiet"
    );

    // Minimal JSON contract (avoid full schema validation)
    assert_contains(&stdout, "\"schema_version\"");
    assert_contains(&stdout, "\"soak-report-v1\"");
    assert_contains(&stdout, "\"soak_mode\"");
    assert_contains(&stdout, "\"artifact\"");
    assert_contains(&stdout, "\"variation_source\"");
    assert_contains(&stdout, "\"deterministic_repeat\"");
}
