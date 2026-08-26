//! #2510: `assay policy resolve --input PATH --format json`
//!
//! Success stdout is one `assay.policy.resolved.v0` document. Fail-closed
//! paths are nonzero with empty stdout. Validate JSON stays `assay.run_summary.v1`.

use std::fs;
use std::path::{Path, PathBuf};

use assay_core::mcp::policy::McpPolicy;
use assert_cmd::Command;
use serde_json::Value;

const SCHEMA: &str = "assay.policy.resolved.v0";
const PROFILE: &str = "jcs:mcp_policy";
const MAX_INPUT_BYTES: usize = 1_000_000;

const VALID: &str = r#"version: "2.0"
name: resolve-fixture
tools:
  allow:
    - echo
"#;

const REORDERED: &str = r#"name: resolve-fixture
tools:
  allow:
    - echo
version: "2.0"
"#;

const EFFECTIVE: &str = r#"version: "2.0"
name: resolve-fixture
tools:
  allow:
    - echo
    - extra
"#;

const LEGACY: &str = r#"version: "1.0"
allow:
  - echo
"#;

const LEGACY_NORMALIZED: &str = r#"version: "1.0"
tools:
  allow:
    - echo
"#;

const BAD_SCHEMA: &str = r#"version: "2.0"
tools:
  allow: [demo]
schemas:
  demo:
    type: object
    properties:
      value:
        type: string
        pattern: "["
"#;

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

fn resolve(path: &Path) -> assert_cmd::assert::Assert {
    assay()
        .args([
            "policy",
            "resolve",
            "--input",
            path.to_str().expect("utf8"),
            "--format",
            "json",
        ])
        .assert()
}

fn parse_ok(path: &Path) -> Value {
    let out = resolve(path).success().get_output().clone();
    serde_json::from_slice(&out.stdout).expect("resolved json")
}

fn assert_fail_empty(path: &Path) {
    let out = resolve(path).failure().get_output().clone();
    assert!(
        out.stdout.is_empty(),
        "fail-closed stdout must be empty, got {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn valid_policy_emits_resolved_v0() {
    let dir = tmp();
    let path = write(dir.path(), "ok.yaml", VALID);
    let v = parse_ok(&path);
    assert_eq!(v["schema"], SCHEMA);
    assert_eq!(v["canonicalization_profile"], PROFILE);
    assert_eq!(v["assay_version"], env!("CARGO_PKG_VERSION"));
    assert!(
        v["input_sha256"]
            .as_str()
            .is_some_and(|s| s.starts_with("sha256:")),
        "input_sha256 {v:?}"
    );
    assert!(
        v["policy_digest"]
            .as_str()
            .is_some_and(|s| s.starts_with("sha256:")),
        "policy_digest {v:?}"
    );
    assert!(v.get("policy").is_some(), "policy object required");
    assert!(v.get("schema_version").is_none());
    assert!(v.get("path").is_none());
    assert!(v.get("input").is_none());
    assert!(v.get("declared_constraint_digest").is_none());
    assert!(v.get("declared_constraint_digest_experimental").is_none());
    let loaded = McpPolicy::from_slice(VALID.as_bytes()).expect("load");
    assert_eq!(
        v["policy_digest"],
        loaded.policy_digest().expect("digest"),
        "document digest must be McpPolicy::policy_digest()"
    );
}

#[test]
fn effective_change_moves_digest_not_schema() {
    let dir = tmp();
    let a = parse_ok(&write(dir.path(), "a.yaml", VALID));
    let b = parse_ok(&write(dir.path(), "b.yaml", EFFECTIVE));
    assert_eq!(a["schema"], b["schema"]);
    assert_ne!(a["policy_digest"], b["policy_digest"]);
}

#[test]
fn reordered_equivalent_mapping_does_not_move_digest() {
    let dir = tmp();
    let a = parse_ok(&write(dir.path(), "a.yaml", VALID));
    let b = parse_ok(&write(dir.path(), "b.yaml", REORDERED));
    assert_eq!(a["policy_digest"], b["policy_digest"]);
    assert_ne!(
        a["input_sha256"], b["input_sha256"],
        "source bytes differ so input_sha256 must move"
    );
}

#[test]
fn path_rename_does_not_move_digest_or_input_hash() {
    let dir = tmp();
    let a = write(dir.path(), "one.yaml", VALID);
    let b = write(dir.path(), "two.yaml", VALID);
    let va = parse_ok(&a);
    let vb = parse_ok(&b);
    assert_eq!(va["policy_digest"], vb["policy_digest"]);
    assert_eq!(va["input_sha256"], vb["input_sha256"]);
    assert_eq!(va["policy"], vb["policy"]);
}

#[test]
fn legacy_normalized_form_is_digested() {
    let dir = tmp();
    let a = parse_ok(&write(dir.path(), "legacy.yaml", LEGACY));
    let b = parse_ok(&write(dir.path(), "norm.yaml", LEGACY_NORMALIZED));
    assert_eq!(a["policy_digest"], b["policy_digest"]);
}

#[test]
fn schema_compilation_failure_emits_no_document() {
    let dir = tmp();
    assert_fail_empty(&write(dir.path(), "bad-schema.yaml", BAD_SCHEMA));
}

#[test]
fn malformed_yaml_emits_no_document() {
    let dir = tmp();
    assert_fail_empty(&write(dir.path(), "bad.yaml", ":\n  -"));
}

#[test]
fn oversized_input_emits_no_document() {
    let dir = tmp();
    let mut body = VALID.to_string();
    body.push_str(&" ".repeat(MAX_INPUT_BYTES));
    assert!(body.len() > MAX_INPUT_BYTES);
    assert_fail_empty(&write(dir.path(), "huge.yaml", &body));
}

#[test]
fn missing_input_is_clap_nonzero_empty_stdout() {
    let out = assay()
        .args(["policy", "resolve", "--format", "json"])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(out.stdout.is_empty());
}

#[test]
fn validate_json_stays_run_summary_v1() {
    let dir = tmp();
    let path = write(dir.path(), "ok.yaml", VALID);
    let out = assay()
        .args([
            "policy",
            "validate",
            "--input",
            path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let v: Value = serde_json::from_slice(&out.stdout).expect("validate json");
    assert_eq!(v["schema"], "assay.run_summary.v1");
}
