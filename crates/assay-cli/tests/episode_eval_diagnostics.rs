//! Corrected #3117 fixtures for comment 5820307639.
//!
//! A missing or ambiguous stored episode stays `fail` / exit 1 / `E_TEST_FAILED`.
//! The existing `details.assertions` object and the run-level `next_step` stay.
//! The row gains a sibling diagnostic. These tests do not treat a missing sibling
//! as proof that some other row was evaluated, and they do not relabel a
//! duplicate prompt.

use assay_core::report::exercised::ASSERTIONS_NOT_EVALUATED;
use assay_core::storage::Store;
use assert_cmd::Command;
use std::fs;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

const MISSING_KIND: &str = "episode_missing";
const AMBIGUOUS_KIND: &str = "episode_ambiguous";
const MISSING_REMEDY: &str = "the episode's meta.test_id must match the suite test id";
const AMBIGUOUS_REMEDY: &str =
    "keep a single stored episode whose meta.test_id is the suite test id";

fn eval_with_assertion() -> String {
    r#"configVersion: 1
suite: "episode_diag"
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
"#
    .to_string()
}

fn episode(episode_id: &str, test_id: &str, prompt: &str, tool: &str) -> String {
    format!(
        r#"{{"type":"episode_start","episode_id":"{episode_id}","timestamp":1000,"input":{{"prompt":"{prompt}"}},"meta":{{"test_id":"{test_id}"}}}}
{{"type":"step","episode_id":"{episode_id}","step_id":"s-{episode_id}","idx":0,"timestamp":1001,"kind":"llm","name":"plan","content":"done"}}
{{"type":"tool_call","episode_id":"{episode_id}","step_id":"s-{episode_id}","timestamp":1002,"tool_name":"{tool}","call_index":0,"args":{{"path":"tmp"}}}}
{{"type":"episode_end","episode_id":"{episode_id}","timestamp":1003,"outcome":"pass","final_output":"done"}}
"#
    )
}

fn assay(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.current_dir(dir)
        .env("ASSAY_EXIT_CODES", "v2")
        .env("ASSAY_VCR_MODE", "off");
    cmd
}

fn run_suite(dir: &Path, trace: &str) -> Output {
    run_suite_with(dir, trace, &[])
}

fn run_suite_with(dir: &Path, trace: &str, extra: &[&str]) -> Output {
    fs::write(dir.join("eval.yaml"), eval_with_assertion()).expect("eval.yaml");
    fs::write(dir.join("trace.jsonl"), trace).expect("trace.jsonl");
    let mut args = vec![
        "run",
        "--config",
        "eval.yaml",
        "--trace-file",
        "trace.jsonl",
        "--db",
        "eval.db",
        "--no-cache",
        "--format",
        "json",
    ];
    args.extend(extra);
    assay(dir).args(args).output().expect("assay run")
}

/// The same blob `id` the fallback decoder rejects. `test_id` is the suite id.
/// `run_id` stays null so primary lookup misses. Timestamp 1 loses to any
/// ingested row for the same test id, so the trace must use a different one.
/// Dropping `episodes` fails earlier, at `prepare`, and never reaches this decoder.
fn seed_blob_episode_for_suite_test(dir: &Path) {
    let store = Store::open(&dir.join("eval.db")).expect("db");
    store.init_schema().expect("schema");
    {
        let conn = store.conn.lock().expect("lock");
        conn.execute(
            "INSERT INTO episodes (id, run_id, test_id, timestamp) VALUES (X'00', NULL, 'no_forbidden_tool', 1)",
            [],
        )
        .expect("insert blob episode id");
    }
}

fn parse_json(bytes: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(bytes);
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("json: {err}: {text}"))
}

fn run_json(dir: &Path) -> serde_json::Value {
    let text = fs::read_to_string(dir.join("run.json")).expect("run.json");
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("run.json: {err}: {text}"))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_unchanged_failure(output: &Output, run: &serde_json::Value) {
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(output));
    assert_eq!(run["exit_code"], 1);
    assert_eq!(run["reason_code"], "E_TEST_FAILED");
    let next = run["resolution"]["next_step"].as_str().expect("next_step");
    assert!(
        next.contains("assay explain"),
        "run next_step stays the explain step: {next}"
    );
}

fn assert_preserved_assertion_error(row: &serde_json::Value) {
    assert_eq!(row["status"], "fail");
    let message = row["message"].as_str().expect("message");
    assert!(
        message.starts_with("assertions error:"),
        "existing assertion-error message stays: {message}"
    );
    let assertions = &row["details"]["assertions"];
    assert!(
        assertions.get("error").and_then(|v| v.as_str()).is_some(),
        "existing assertions object stays an error object: {assertions}"
    );
    assert!(assertions.get("evaluated").is_none(), "{assertions}");
    assert!(assertions.get("code").is_none(), "{assertions}");
    assert!(assertions.get("kind").is_none(), "{assertions}");
}

fn assert_row_diagnostic(row: &serde_json::Value, kind: &str, remedy: &str) {
    let diagnostic = &row["details"][ASSERTIONS_NOT_EVALUATED];
    assert_eq!(diagnostic["evaluated"], false, "{diagnostic}");
    assert_eq!(diagnostic["kind"], kind, "{diagnostic}");
    assert_eq!(diagnostic["remedy"], remedy, "{diagnostic}");
    assert!(
        diagnostic.get("code").is_none(),
        "unevaluated-episode diagnostic is code-free: {diagnostic}"
    );
}

#[test]
fn missing_episode_keeps_the_failure_and_records_a_row_diagnostic() {
    let dir = TempDir::new().expect("tempdir");
    let output = run_suite(
        dir.path(),
        &episode("ep-1", "other_test", "tidy", "list_files"),
    );
    let report = parse_json(&output.stdout);
    let run = run_json(dir.path());
    assert_unchanged_failure(&output, &run);

    let row = &report["results"][0];
    assert_eq!(row["test_id"], "no_forbidden_tool");
    assert_preserved_assertion_error(row);
    assert_row_diagnostic(row, MISSING_KIND, MISSING_REMEDY);
    assert_ne!(
        row["details"][ASSERTIONS_NOT_EVALUATED]["kind"],
        AMBIGUOUS_KIND
    );
}

#[test]
fn ambiguous_episode_keeps_the_failure_and_records_its_own_remedy() {
    let dir = TempDir::new().expect("tempdir");
    let trace = format!(
        "{}{}",
        episode("ep-1", "no_forbidden_tool", "tidy", "list_files"),
        episode("ep-2", "no_forbidden_tool", "other-prompt", "list_files")
    );
    let output = run_suite(dir.path(), &trace);
    let report = parse_json(&output.stdout);
    let run = run_json(dir.path());
    assert_unchanged_failure(&output, &run);

    let row = &report["results"][0];
    assert_preserved_assertion_error(row);
    assert_row_diagnostic(row, AMBIGUOUS_KIND, AMBIGUOUS_REMEDY);
    assert_ne!(
        row["details"][ASSERTIONS_NOT_EVALUATED]["remedy"],
        MISSING_REMEDY
    );
}

#[test]
fn database_decoding_error_does_not_receive_the_unevaluated_companion() {
    let dir = TempDir::new().expect("tempdir");
    seed_blob_episode_for_suite_test(dir.path());
    // Prompt and final output satisfy the suite. meta.test_id does not, so
    // ingest does not hide the blob behind a newer text id. Latest-stored
    // lookup is what reads that id as text.
    let output = run_suite_with(
        dir.path(),
        &episode("ep-1", "other_test", "tidy", "list_files"),
        &["--latest-stored-episode"],
    );
    let report = parse_json(&output.stdout);
    let run = run_json(dir.path());
    assert_unchanged_failure(&output, &run);

    let row = &report["results"][0];
    assert_eq!(row["test_id"], "no_forbidden_tool");
    assert_preserved_assertion_error(row);
    assert!(
        row["details"].get(ASSERTIONS_NOT_EVALUATED).is_none(),
        "a database decoding failure must not carry the unevaluated-episode diagnostic: {row}"
    );
    let error = row["details"]["assertions"]["error"]
        .as_str()
        .expect("assertions error");
    let error_l = error.to_lowercase();
    assert!(
        error_l.contains("column") || error_l.contains("blob"),
        "row must report the id decoding failure: {error}"
    );
    assert!(
        !error.contains("episode_missing"),
        "database failure was labeled episode_missing: {error}"
    );
    assert!(
        !error.contains("E_TRACE_EPISODE_MISSING"),
        "database failure was labeled as a missing episode: {error}"
    );
    assert!(
        !error.contains("meta.test_id"),
        "database failure inherited the missing-episode remedy: {error}"
    );
}

#[test]
fn evaluated_violation_has_no_unevaluated_episode_diagnostic() {
    let dir = TempDir::new().expect("tempdir");
    let output = run_suite(
        dir.path(),
        &episode("ep-1", "no_forbidden_tool", "tidy", "delete_repository"),
    );
    let report = parse_json(&output.stdout);
    let run = run_json(dir.path());
    assert_unchanged_failure(&output, &run);

    let row = &report["results"][0];
    assert_eq!(row["status"], "fail");
    let message = row["message"].as_str().expect("message");
    assert!(message.contains("assertions failed"), "{message}");
    assert!(
        row["details"]["assertions"].is_array(),
        "evaluated violation stays a diagnostic array: {row}"
    );
    assert!(
        row["details"].get(ASSERTIONS_NOT_EVALUATED).is_none(),
        "this evaluated violation must not carry the unevaluated-episode diagnostic: {row}"
    );
}

#[test]
fn matching_episode_passes_without_the_unevaluated_diagnostic() {
    let dir = TempDir::new().expect("tempdir");
    let output = run_suite(
        dir.path(),
        &episode("ep-1", "no_forbidden_tool", "tidy", "list_files"),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        stderr(&output)
    );
    let run = run_json(dir.path());
    assert_eq!(run["exit_code"], 0);

    let report = parse_json(&output.stdout);
    let row = &report["results"][0];
    assert_eq!(row["status"], "pass");
    assert!(
        row["details"].get(ASSERTIONS_NOT_EVALUATED).is_none(),
        "this passing row must not carry the unevaluated-episode diagnostic: {row}"
    );
}
