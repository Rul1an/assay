//! #2511: project one existing enforcement-health document.
//!
//! `assay project-enforcement-health --format json --input PATH`
//!
//! Success bytes are exact. Fail-closed paths are nonzero with empty stdout.
//! Valid `active` fixtures are constructor-legal (v0 attach+strong; v1 both
//! Landlock confirmations, strong, no failure). Forged `active` fails closed.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

const SCHEMA: &str = "assay.enforcement_health_projection.v0";
const V0: &str = "assay.enforcement_health.v0";
const V1: &str = "assay.enforcement_health.v1";
/// Same cap the CLI must apply before parse. A syntactically valid document
/// larger than this must fail closed.
const MAX_INPUT_BYTES: usize = 65_536;

fn assay() -> Command {
    Command::cargo_bin("assay").expect("binary")
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).expect("write fixture");
    path
}

fn v0(status: &str) -> String {
    let (attach, class) = if status == "active" {
        ("true", "strong")
    } else {
        ("false", "basic")
    };
    format!(
        r#"{{"schema":"{V0}","scope":"ipv4_tcp_connect","network_enforcement":"{status}","attach_confirmed":{attach},"blocked_count":0,"allowed_count":0,"enforcement_class":"{class}"}}"#
    )
}

fn v1(status: &str) -> String {
    let (nnp, restrict, class) = if status == "active" {
        ("true", "true", "strong")
    } else {
        ("false", "false", "basic")
    };
    format!(
        r#"{{"schema":"{V1}","status":"{status}","mechanism":"landlock","scope":"tcp_connect_landlock_port","policy_semantics":"allowlist","enforcement_class":"{class}","landlock":{{"abi":4,"no_new_privs_confirmed":{nnp},"restrict_self_confirmed":{restrict}}},"probe":null,"non_claims":[]}}"#
    )
}

fn expected(source_schema: &str, observation: &str) -> String {
    format!(
        r#"{{"schema":"{SCHEMA}","lossy":true,"source_schema":"{source_schema}","observation":"{observation}"}}"#
    )
}

fn project(path: &Path) -> assert_cmd::assert::Assert {
    assay()
        .args([
            "project-enforcement-health",
            "--format",
            "json",
            "--input",
            path.to_str().expect("utf8"),
        ])
        .assert()
}

fn assert_exact_ok(path: &Path, source_schema: &str, observation: &str) {
    let out = project(path).success().get_output().clone();
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    let want = format!("{}\n", expected(source_schema, observation));
    assert_eq!(
        stdout, want,
        "stdout must be exact projection bytes plus newline"
    );
}

fn assert_fail_args(args: &[&str]) {
    let out = assay().args(args).assert().failure().get_output().clone();
    assert!(
        out.stdout.is_empty(),
        "fail-closed stdout must be empty, got {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn v0_active_maps_to_applied() {
    let dir = tmp();
    let path = write(dir.path(), "v0-active.json", &v0("active"));
    assert_exact_ok(&path, V0, "applied");
}

#[test]
fn v0_failed_maps_to_degraded() {
    let dir = tmp();
    let path = write(dir.path(), "v0-failed.json", &v0("failed"));
    assert_exact_ok(&path, V0, "degraded");
}

#[test]
fn v0_absent_maps_to_not_requested() {
    let dir = tmp();
    let path = write(dir.path(), "v0-absent.json", &v0("absent"));
    assert_exact_ok(&path, V0, "not_requested");
}

#[test]
fn v1_active_maps_to_applied() {
    let dir = tmp();
    let path = write(dir.path(), "v1-active.json", &v1("active"));
    assert_exact_ok(&path, V1, "applied");
}

#[test]
fn v1_failed_maps_to_degraded() {
    let dir = tmp();
    let path = write(dir.path(), "v1-failed.json", &v1("failed"));
    assert_exact_ok(&path, V1, "degraded");
}

#[test]
fn source_schema_is_the_input_identity() {
    let dir = tmp();
    let v0_path = write(dir.path(), "src-v0.json", &v0("active"));
    let v1_path = write(dir.path(), "src-v1.json", &v1("failed"));
    assert_exact_ok(&v0_path, V0, "applied");
    assert_exact_ok(&v1_path, V1, "degraded");
}

#[test]
fn missing_input_flag_is_clap_nonzero_empty_stdout() {
    assert_fail_args(&["project-enforcement-health", "--format", "json"]);
}

#[test]
fn missing_path_is_nonzero_empty_stdout() {
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        "/no/such/enforcement-health.json",
    ]);
}

#[test]
fn unknown_schema_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(
        dir.path(),
        "unknown.json",
        r#"{"schema":"assay.not_enforcement_health.v0"}"#,
    );
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn malformed_json_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(dir.path(), "malformed.json", "{");
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn oversized_valid_json_is_nonzero_empty_stdout() {
    let dir = tmp();
    let mut body = v0("active");
    body.push_str(&" ".repeat(MAX_INPUT_BYTES));
    assert!(body.len() > MAX_INPUT_BYTES);
    let path = write(dir.path(), "oversized.json", &body);
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn forged_v1_absent_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(dir.path(), "forged-absent.json", &v1("absent"));
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn v0_not_applicable_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(dir.path(), "v0-na.json", &v0("not_applicable"));
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn v0_active_without_confirmed_attach_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(
        dir.path(),
        "v0-forged-active.json",
        r#"{"schema":"assay.enforcement_health.v0","scope":"ipv4_tcp_connect","network_enforcement":"active","attach_confirmed":false,"blocked_count":0,"allowed_count":0,"enforcement_class":"basic"}"#,
    );
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}

#[test]
fn v1_active_without_restrict_confirmations_is_nonzero_empty_stdout() {
    let dir = tmp();
    let path = write(
        dir.path(),
        "v1-forged-active.json",
        r#"{"schema":"assay.enforcement_health.v1","status":"active","mechanism":"landlock","scope":"tcp_connect_landlock_port","policy_semantics":"allowlist","enforcement_class":"basic","landlock":{"abi":4,"no_new_privs_confirmed":false,"restrict_self_confirmed":false},"probe":null,"non_claims":[]}"#,
    );
    assert_fail_args(&[
        "project-enforcement-health",
        "--format",
        "json",
        "--input",
        path.to_str().unwrap(),
    ]);
}
