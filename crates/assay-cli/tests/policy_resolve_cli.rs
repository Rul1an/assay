//! #2510: `assay policy resolve --input PATH --format json`
//!
//! Success stdout is one `assay.policy.resolved.v0` document. Fail-closed
//! paths keep empty stdout. Validate JSON stays `assay.run_summary.v1`.

use assay_core::fingerprint::sha256_hex;
use assay_core::mcp::decision::POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY;
use assay_core::mcp::jcs;
use assay_core::mcp::policy::McpPolicy;
use assert_cmd::Command;
use serde_json::Value;

const SCHEMA: &str = "assay.policy.resolved.v0";
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

const V1_CONSTRAINTS: &str = r#"version: "1.0"
name: resolve-legacy
allow:
  - read_file
constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/app/.*"
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

fn write(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("write fixture");
    path
}

fn resolve(path: &std::path::Path) -> assert_cmd::assert::Assert {
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

fn parse_ok(path: &std::path::Path) -> Value {
    let out = resolve(path).success().get_output().clone();
    serde_json::from_slice(&out.stdout).expect("resolved json")
}

fn assert_empty_stdout_failure(output: &std::process::Output) {
    assert!(
        output.stdout.is_empty(),
        "fail-closed stdout must be empty, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

fn assert_reconstructable(v: &Value) {
    assert_eq!(v["schema"], SCHEMA);
    assert_eq!(
        v["canonicalization_profile"],
        POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY
    );
    let canonical = jcs::to_string(&v["policy"]).expect("jcs of emitted policy");
    let digest = format!("sha256:{}", sha256_hex(&canonical));
    assert_eq!(
        v["policy_digest"], digest,
        "RFC8785/JCS of emitted policy must hash to policy_digest"
    );
}

#[test]
fn valid_policy_emits_resolved_v0() {
    let dir = tmp();
    let path = write(dir.path(), "ok.yaml", VALID);
    let v = parse_ok(&path);
    assert_reconstructable(&v);
    assert_eq!(v["assay_version"], env!("CARGO_PKG_VERSION"));
    assert!(
        v["input_sha256"]
            .as_str()
            .is_some_and(|s| s.starts_with("sha256:")),
        "input_sha256 {v:?}"
    );
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
fn emitted_policy_jcs_reconstructs_digest_for_legacy_v1() {
    let dir = tmp();
    assert_reconstructable(&parse_ok(&write(dir.path(), "legacy.yaml", LEGACY)));
    assert_reconstructable(&parse_ok(&write(
        dir.path(),
        "constraints.yaml",
        V1_CONSTRAINTS,
    )));
}

#[test]
fn v1_constraints_are_normalized_into_schemas() {
    let dir = tmp();
    let v = parse_ok(&write(dir.path(), "v1.yaml", V1_CONSTRAINTS));
    assert!(
        v["policy"]["schemas"]["read_file"].is_object(),
        "v1 constraints must become schemas: {v:?}"
    );
    assert_eq!(
        v["policy"]["constraints"],
        serde_json::json!([]),
        "migrated constraints must be empty"
    );
    assert_reconstructable(&v);
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
    let out = resolve(&write(dir.path(), "bad-schema.yaml", BAD_SCHEMA))
        .failure()
        .get_output()
        .clone();
    assert_empty_stdout_failure(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("policy schemas failed to compile"),
        "schema failure must stay honest stderr-only: {stderr}"
    );
    assert!(
        !stderr.contains("E_POLICY_PARSE"),
        "schema compile must not be typed as parse: {stderr}"
    );
}

#[test]
fn malformed_yaml_emits_no_document() {
    let dir = tmp();
    let out = resolve(&write(dir.path(), "bad.yaml", ":\n  -"))
        .failure()
        .get_output()
        .clone();
    assert_empty_stdout_failure(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("E_POLICY_PARSE"),
        "malformed policy must retain typed E_POLICY_PARSE: {stderr}"
    );
}

#[test]
fn missing_policy_file_is_untyped_stderr() {
    let dir = tmp();
    let path = dir.path().join("missing.yaml");
    let out = resolve(&path).failure().get_output().clone();
    assert_empty_stdout_failure(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("failed to load policy") || stderr.contains("fatal:"),
        "missing file must stay honest stderr: {stderr}"
    );
    assert!(
        !stderr.contains("E_POLICY_PARSE"),
        "missing file must not be typed as parse: {stderr}"
    );
}

#[test]
fn oversized_input_emits_no_document() {
    let dir = tmp();
    let mut body = VALID.to_string();
    body.push_str(&" ".repeat(MAX_INPUT_BYTES));
    assert!(body.len() > MAX_INPUT_BYTES);
    let out = resolve(&write(dir.path(), "huge.yaml", &body))
        .failure()
        .get_output()
        .clone();
    assert_empty_stdout_failure(&out);
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
fn deny_deprecations_is_not_a_resolve_flag() {
    let out = assay()
        .args(["policy", "resolve", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("deny-deprecations"),
        "resolve must not expose --deny-deprecations: {stdout}"
    );
    assert!(stdout.contains("--input"));
    assert!(stdout.contains("--format"));
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
