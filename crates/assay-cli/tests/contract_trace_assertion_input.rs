//! Contract: `run` / `ci` ingest `--trace-file` when assertions evaluate stored
//! episodes, and those assertions evaluate the episodes this invocation ingested.
//!
//! Refs #3116.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const EVAL: &str = r#"configVersion: 1
suite: "ingest_repro"
model: "trace"
tests:
  - id: "no_forbidden_tool"
    input:
      prompt: "tidy"
    expected:
      type: regex_match
      pattern: "done"
    assertions:
      - type: trace_must_not_call_tool
        tool: "delete_repository"
"#;

fn episode(episode_id: &str, ts: u64, tool: &str, step_id: &str) -> String {
    format!(
        r#"{{"type":"episode_start","episode_id":"{episode_id}","timestamp":{ts},"input":{{"prompt":"tidy"}},"meta":{{"test_id":"no_forbidden_tool"}}}}
{{"type":"step","episode_id":"{episode_id}","step_id":"{step_id}","idx":0,"timestamp":{},"kind":"llm","name":"plan","content":"done"}}
{{"type":"tool_call","episode_id":"{episode_id}","step_id":"{step_id}","timestamp":{},"tool_name":"{tool}","call_index":0,"args":{{"path":"tmp"}}}}
{{"type":"episode_end","episode_id":"{episode_id}","timestamp":{},"outcome":"pass","final_output":"done"}}
"#,
        ts + 1,
        ts + 2,
        ts + 3
    )
}

fn write_suite(dir: &Path, trace: &str) {
    fs::write(dir.join("eval.yaml"), EVAL).expect("eval.yaml");
    fs::write(dir.join("trace.jsonl"), trace).expect("trace.jsonl");
}

fn assay() -> Command {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.env("ASSAY_EXIT_CODES", "v2")
        .env("ASSAY_VCR_MODE", "off");
    cmd
}

fn run_cmd(dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = assay();
    cmd.current_dir(dir).args(args).arg("--no-cache");
    cmd.assert()
}

fn eval_args(sub: &str, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        sub.to_string(),
        "--config".into(),
        "eval.yaml".into(),
        "--trace-file".into(),
        "trace.jsonl".into(),
        "--db".into(),
        "eval.db".into(),
    ];
    args.extend(extra.iter().map(|s| (*s).to_string()));
    args
}

fn fresh_pass(sub: &str, extra: &[&str]) {
    let dir = TempDir::new().expect("tempdir");
    write_suite(dir.path(), &episode("ep-1", 1000, "list_files", "s1"));
    let args = eval_args(sub, extra);
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    run_cmd(dir.path(), &borrowed).success();
}

fn fresh_forbidden_fails(sub: &str, extra: &[&str]) {
    let dir = TempDir::new().expect("tempdir");
    write_suite(
        dir.path(),
        &episode("ep-1", 1000, "delete_repository", "s1"),
    );
    let args = eval_args(sub, extra);
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    run_cmd(dir.path(), &borrowed)
        .failure()
        .code(1)
        .stderr(predicate::str::contains("assertions failed"))
        .stderr(predicate::str::contains("E_TRACE_EPISODE_MISSING").not());
}

#[test]
fn fresh_db_benign_run() {
    fresh_pass("run", &[]);
}

#[test]
fn fresh_db_benign_run_replay_strict() {
    fresh_pass("run", &["--replay-strict"]);
}

#[test]
fn fresh_db_benign_ci() {
    fresh_pass("ci", &[]);
}

#[test]
fn fresh_db_benign_ci_replay_strict() {
    fresh_pass("ci", &["--replay-strict"]);
}

#[test]
fn fresh_db_forbidden_run() {
    fresh_forbidden_fails("run", &[]);
}

#[test]
fn fresh_db_forbidden_run_replay_strict() {
    fresh_forbidden_fails("run", &["--replay-strict"]);
}

#[test]
fn fresh_db_forbidden_ci() {
    fresh_forbidden_fails("ci", &[]);
}

#[test]
fn fresh_db_forbidden_ci_replay_strict() {
    fresh_forbidden_fails("ci", &["--replay-strict"]);
}

#[test]
fn reused_db_benign_ci_then_forbidden_run() {
    let dir = TempDir::new().expect("tempdir");
    write_suite(
        dir.path(),
        &episode("ep-benign", 5000, "list_files", "s-benign"),
    );
    let benign = eval_args("ci", &["--replay-strict"]);
    let benign_ref: Vec<&str> = benign.iter().map(String::as_str).collect();
    run_cmd(dir.path(), &benign_ref).success();

    fs::write(
        dir.path().join("trace.jsonl"),
        episode("ep-forbidden", 1000, "delete_repository", "s-forb"),
    )
    .expect("rewrite forbidden trace");
    let forbidden = eval_args("run", &[]);
    let forbidden_ref: Vec<&str> = forbidden.iter().map(String::as_str).collect();
    run_cmd(dir.path(), &forbidden_ref)
        .failure()
        .code(1)
        .stderr(predicate::str::contains("assertions failed"))
        .stderr(predicate::str::contains("E_TRACE_EPISODE_MISSING").not());
}

#[test]
fn reused_db_newer_benign_then_older_forbidden_ci() {
    let dir = TempDir::new().expect("tempdir");
    write_suite(
        dir.path(),
        &episode("ep-benign", 5000, "list_files", "s-benign"),
    );
    let benign = eval_args("ci", &["--replay-strict"]);
    let benign_ref: Vec<&str> = benign.iter().map(String::as_str).collect();
    run_cmd(dir.path(), &benign_ref).success();

    fs::write(
        dir.path().join("trace.jsonl"),
        episode("ep-forbidden", 1000, "delete_repository", "s-forb"),
    )
    .expect("rewrite older forbidden trace");
    let forbidden = eval_args("ci", &["--replay-strict"]);
    let forbidden_ref: Vec<&str> = forbidden.iter().map(String::as_str).collect();
    run_cmd(dir.path(), &forbidden_ref)
        .failure()
        .code(1)
        .stderr(predicate::str::contains("assertions failed"))
        .stderr(predicate::str::contains("E_TRACE_EPISODE_MISSING").not());
}

#[test]
fn reused_db_same_step_id_reuses_tool_call_slot() {
    let dir = TempDir::new().expect("tempdir");
    write_suite(dir.path(), &episode("ep-1", 1000, "list_files", "s1"));
    let benign = eval_args("ci", &["--replay-strict"]);
    let benign_ref: Vec<&str> = benign.iter().map(String::as_str).collect();
    run_cmd(dir.path(), &benign_ref).success();

    fs::write(
        dir.path().join("trace.jsonl"),
        episode("ep-1", 1000, "delete_repository", "s1"),
    )
    .expect("rewrite same-slot forbidden trace");
    let forbidden = eval_args("ci", &["--replay-strict"]);
    let forbidden_ref: Vec<&str> = forbidden.iter().map(String::as_str).collect();
    run_cmd(dir.path(), &forbidden_ref)
        .failure()
        .code(1)
        .stderr(predicate::str::contains("assertions failed"))
        .stderr(predicate::str::contains("E_TRACE_EPISODE_MISSING").not());
}

#[test]
fn trace_ingest_then_run_db_still_evaluates() {
    let pass_dir = TempDir::new().expect("tempdir");
    write_suite(pass_dir.path(), &episode("ep-1", 1000, "list_files", "s1"));
    assay()
        .current_dir(pass_dir.path())
        .args([
            "trace",
            "ingest",
            "--input",
            "trace.jsonl",
            "--output",
            "x.db",
        ])
        .assert()
        .success();
    run_cmd(
        pass_dir.path(),
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--db",
            "x.db",
        ],
    )
    .success();

    let fail_dir = TempDir::new().expect("tempdir");
    write_suite(
        fail_dir.path(),
        &episode("ep-1", 1000, "delete_repository", "s1"),
    );
    assay()
        .current_dir(fail_dir.path())
        .args([
            "trace",
            "ingest",
            "--input",
            "trace.jsonl",
            "--output",
            "x.db",
        ])
        .assert()
        .success();
    run_cmd(
        fail_dir.path(),
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--db",
            "x.db",
        ],
    )
    .failure()
    .code(1)
    .stderr(predicate::str::contains("assertions failed"));
}

#[test]
fn help_strings_name_the_ingest() {
    assay()
        .args(["run", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ingested into --db when assertions evaluate stored episodes",
        ))
        .stdout(predicate::str::contains("auto-ingest to DB").not());

    assay()
        .args(["ci", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ingested into --db when assertions evaluate stored episodes",
        ));

    assay()
        .args(["trace", "ingest", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(".db"))
        .stdout(predicate::str::contains(".sqlite"));
}
