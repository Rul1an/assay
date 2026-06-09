//! Contract tests for `assay project-otel`.
//!
//! The CLI is transport only: it reads files, parses JSON, calls
//! `assay_core::otel::projection::project`, and writes JSON. These tests pin that contract — the
//! happy path reproduces the committed golden projection, and the error paths return a non-zero exit
//! with an empty stdout, a message on stderr, no panic/backtrace, and no raw artifact content echoed
//! back. JSON is compared as `serde_json::Value`, not byte-for-byte.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../assay-core/tests/fixtures/otel_projection"
    ))
}

/// Split the committed wrapper `input.json` into the three separate artifact files the CLI takes,
/// written under a unique per-test dir in CARGO_TARGET_TMPDIR. Returns (dir, cap, obs, enf).
fn write_split_inputs(case: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let input: Value =
        serde_json::from_str(&fs::read_to_string(fixtures_dir().join("input.json")).unwrap())
            .unwrap();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("project_otel_{case}"));
    fs::create_dir_all(&dir).unwrap();
    let cap = dir.join("capability-surface.json");
    let obs = dir.join("observation-health.json");
    let enf = dir.join("enforcement-health.json");
    fs::write(
        &cap,
        serde_json::to_string(&input["capability_surface"]).unwrap(),
    )
    .unwrap();
    fs::write(
        &obs,
        serde_json::to_string(&input["observation_health"]).unwrap(),
    )
    .unwrap();
    fs::write(
        &enf,
        serde_json::to_string(&input["enforcement_health"]).unwrap(),
    )
    .unwrap();
    (dir, cap, obs, enf)
}

fn expected() -> Value {
    serde_json::from_str(&fs::read_to_string(fixtures_dir().join("expected.json")).unwrap())
        .unwrap()
}

#[test]
fn happy_path_matches_golden_projection() {
    let (_dir, cap, obs, enf) = write_split_inputs("happy");
    let out = Command::cargo_bin("assay")
        .unwrap()
        .args([
            "project-otel",
            "--capability-surface",
            cap.to_str().unwrap(),
            "--observation-health",
            obs.to_str().unwrap(),
            "--enforcement-health",
            enf.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let got: Value = serde_json::from_slice(&out).expect("stdout is parseable JSON");
    assert_eq!(
        got,
        expected(),
        "CLI stdout must match the committed golden projection"
    );
}

#[test]
fn missing_file_fails_clean() {
    let missing = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("does-not-exist.json");
    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "project-otel",
            "--capability-surface",
            missing.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("cannot read"));
}

#[test]
fn malformed_json_fails_without_panic_or_content_leak() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("project_otel_malformed");
    fs::create_dir_all(&dir).unwrap();
    let cap = dir.join("capability-surface.json");
    // A unique body string so we can assert it is NOT echoed back in the error.
    let body = "{ this_is_not_valid_json_sentinel ";
    fs::write(&cap, body).unwrap();
    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "project-otel",
            "--capability-surface",
            cap.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("invalid JSON"))
        // no panic/backtrace in normal error output
        .stderr(predicate::str::contains("panicked").not())
        .stderr(predicate::str::contains("RUST_BACKTRACE").not())
        // no raw artifact content echoed into the error
        .stderr(predicate::str::contains("not_valid_json_sentinel").not());
}

#[test]
fn out_flag_writes_file_and_leaves_stdout_empty() {
    let (dir, cap, obs, enf) = write_split_inputs("out");
    let out_path = dir.join("projection.json");
    Command::cargo_bin("assay")
        .unwrap()
        .args([
            "project-otel",
            "--capability-surface",
            cap.to_str().unwrap(),
            "--observation-health",
            obs.to_str().unwrap(),
            "--enforcement-health",
            enf.to_str().unwrap(),
            "--out",
            out_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    let written: Value =
        serde_json::from_str(&fs::read_to_string(&out_path).unwrap()).expect("--out file is JSON");
    assert_eq!(
        written,
        expected(),
        "--out file must match the committed golden projection"
    );
}
