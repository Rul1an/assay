#![allow(deprecated)] // cargo_bin is deprecated but still supported by assert_cmd
//! Stop-line contract tests for RFC-003 generate decomposition.
//! Do not relax assertions without an explicit behavior-change decision.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn write_file(path: &Path, content: &str) {
    fs::write(path, content).expect("write fixture");
}

fn normalize_text(raw: &str, tmp_root: &Path) -> String {
    let mut out = raw.replace("\r\n", "\n");
    if let Some(tmp) = tmp_root.to_str() {
        out = out.replace(tmp, "<TMP>");
    }
    let mut lines = Vec::new();
    for line in out.lines() {
        if line.trim_start().starts_with("generated_at:") {
            continue;
        } else {
            lines.push(line.to_string());
        }
    }
    lines.join("\n")
}

fn run_generate(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.arg("generate");
    for arg in args {
        cmd.arg(arg);
    }
    cmd.output().expect("run generate")
}

#[test]
fn generate_contract_policy_yaml_golden() {
    let root = workspace_root();
    let input = root.join("crates/assay-cli/tests/golden/passthrough/input.jsonl");
    let expected = root.join("crates/assay-cli/tests/golden/passthrough/expected.yaml");
    let tmp = tempdir().expect("tempdir");
    let output = run_generate(&[
        "--input",
        input.to_str().expect("input path"),
        "--name",
        "Test Policy",
        "--dry-run",
    ]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let got = normalize_text(&String::from_utf8_lossy(&output.stdout), tmp.path());
    let exp = normalize_text(
        &fs::read_to_string(expected).expect("read expected golden"),
        tmp.path(),
    );
    assert_eq!(got.trim(), exp.trim());
}

#[test]
fn generate_contract_read_events_warnings_contract() {
    let tmp = tempdir().expect("tempdir");
    let input = tmp.path().join("mixed.jsonl");
    write_file(
        &input,
        r#"{"type":"file_open","path":"/tmp/a","timestamp":1}
not-json
{"type":"proc_exec","path":"/bin/echo","timestamp":2}
{
garbage
"#,
    );

    let output = run_generate(&["--input", input.to_str().expect("input path"), "--dry-run"]);
    assert!(output.status.success(), "expected success");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning: skipping line"));
    assert!(!stderr.contains("warning: skipped"));
}

#[test]
fn generate_contract_read_events_all_invalid_is_hard_error() {
    let tmp = tempdir().expect("tempdir");
    let input = tmp.path().join("all-invalid.jsonl");
    write_file(&input, "not-json\n{\n#comment\n");

    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.arg("generate")
        .arg("--input")
        .arg(&input)
        .arg("--dry-run")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("no valid events found"));
}

#[test]
fn generate_contract_mode_gating_none_or_both() {
    let tmp = tempdir().expect("tempdir");
    let input = tmp.path().join("trace.jsonl");
    write_file(
        &input,
        r#"{"type":"file_open","path":"/tmp/a","timestamp":1}
"#,
    );
    let profile = tmp.path().join("profile.yaml");
    write_file(
        &profile,
        r#"version: "1.0"
name: "p"
created_at: "2026-02-09T00:00:00Z"
updated_at: "2026-02-09T00:00:00Z"
total_runs: 1
run_ids: []
run_id_digests: []
entries:
  files: {}
  network: {}
  processes: {}
"#,
    );

    let mut none_cmd = Command::cargo_bin("assay").expect("assay binary");
    none_cmd
        .arg("generate")
        .arg("--dry-run")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains(
            "specify either --input (single-run) or --profile (multi-run)",
        ));

    let mut both_cmd = Command::cargo_bin("assay").expect("assay binary");
    both_cmd
        .arg("generate")
        .arg("--input")
        .arg(&input)
        .arg("--profile")
        .arg(&profile)
        .arg("--dry-run")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains(
            "cannot use both --input and --profile",
        ));
}

#[test]
fn generate_contract_profile_path_smoke() {
    let tmp = tempdir().expect("tempdir");
    let profile = tmp.path().join("profile.yaml");
    write_file(
        &profile,
        r#"version: "1.0"
name: "profile-smoke"
created_at: "2026-02-09T00:00:00Z"
updated_at: "2026-02-09T00:00:00Z"
total_runs: 10
run_ids: []
run_id_digests: []
entries:
  files:
    "/tmp/a":
      first_seen: 1
      last_seen: 10
      runs_seen: 10
      hits_total: 10
  network: {}
  processes: {}
"#,
    );

    let output = run_generate(&[
        "--profile",
        profile.to_str().expect("profile path"),
        "--dry-run",
    ]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("files:"));
    assert!(stdout.contains("/tmp/a"));
}

#[test]
fn generate_contract_diff_missing_baseline_output_file_is_handled() {
    let tmp = tempdir().expect("tempdir");
    let input = tmp.path().join("trace.jsonl");
    let output_path = tmp.path().join("nonexistent-policy.yaml");
    write_file(
        &input,
        r#"{"type":"file_open","path":"/tmp/a","timestamp":1}
"#,
    );

    let output = run_generate(&[
        "--input",
        input.to_str().expect("input path"),
        "--diff",
        "--dry-run",
        "--output",
        output_path.to_str().expect("output path"),
    ]);
    assert!(output.status.success(), "diff should succeed");
    let stderr = normalize_text(&String::from_utf8_lossy(&output.stderr), tmp.path());
    assert!(stderr.contains("Policy diff (<TMP>/nonexistent-policy.yaml -> generated):"));
    assert!(stderr.contains("Summary: +1 added, -0 removed, ~0 changed"));
}

#[test]
fn generate_contract_diff_no_changes_block() {
    let tmp = tempdir().expect("tempdir");
    let input = tmp.path().join("trace.jsonl");
    let policy = tmp.path().join("policy.yaml");
    write_file(
        &input,
        r#"{"type":"file_open","path":"/tmp/a","timestamp":1}
{"type":"proc_exec","path":"/bin/echo","timestamp":2}
"#,
    );

    let first = run_generate(&[
        "--input",
        input.to_str().expect("input path"),
        "--output",
        policy.to_str().expect("policy path"),
    ]);
    assert!(first.status.success(), "initial write should succeed");

    let second = run_generate(&[
        "--input",
        input.to_str().expect("input path"),
        "--output",
        policy.to_str().expect("policy path"),
        "--dry-run",
        "--diff",
    ]);
    assert!(second.status.success(), "diff dry-run should succeed");
    let stderr = normalize_text(&String::from_utf8_lossy(&second.stderr), tmp.path());
    assert!(stderr.contains("Policy diff (<TMP>/policy.yaml -> generated):"));
    assert!(stderr.contains("  (no changes)"));
}

#[test]
fn generate_contract_same_input_twice_deterministic() {
    let tmp = tempdir().expect("tempdir");
    let input = tmp.path().join("trace.jsonl");
    write_file(
        &input,
        r#"{"type":"file_open","path":"/tmp/z","timestamp":2}
{"type":"file_open","path":"/tmp/a","timestamp":1}
{"type":"proc_exec","path":"/bin/echo","timestamp":3}
"#,
    );

    let out1 = run_generate(&["--input", input.to_str().expect("input path"), "--dry-run"]);
    let out2 = run_generate(&["--input", input.to_str().expect("input path"), "--dry-run"]);
    assert!(out1.status.success() && out2.status.success());

    let norm1 = normalize_text(&String::from_utf8_lossy(&out1.stdout), tmp.path());
    let norm2 = normalize_text(&String::from_utf8_lossy(&out2.stdout), tmp.path());
    assert_eq!(norm1, norm2);
}

#[test]
fn generate_contract_dry_run_deterministic_on_shuffled_input() {
    let tmp = tempdir().expect("tempdir");
    let input_a = tmp.path().join("a.jsonl");
    let input_b = tmp.path().join("b.jsonl");
    write_file(
        &input_a,
        r#"{"type":"file_open","path":"/tmp/a","timestamp":1}
{"type":"proc_exec","path":"/bin/echo","timestamp":2}
{"type":"net_connect","dest":"api.example.com:443","timestamp":3}
"#,
    );
    write_file(
        &input_b,
        r#"{"type":"net_connect","dest":"api.example.com:443","timestamp":3}
{"type":"proc_exec","path":"/bin/echo","timestamp":2}
{"type":"file_open","path":"/tmp/a","timestamp":1}
"#,
    );

    let out_a = run_generate(&["--input", input_a.to_str().expect("a path"), "--dry-run"]);
    let out_b = run_generate(&["--input", input_b.to_str().expect("b path"), "--dry-run"]);
    assert!(out_a.status.success() && out_b.status.success());

    let norm_a = normalize_text(&String::from_utf8_lossy(&out_a.stdout), tmp.path());
    let norm_b = normalize_text(&String::from_utf8_lossy(&out_b.stdout), tmp.path());
    assert_eq!(norm_a, norm_b);
}
