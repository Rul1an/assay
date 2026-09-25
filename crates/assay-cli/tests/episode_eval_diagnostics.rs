//! #3117 minor (6.7.0, variant B1) contract for not-evaluated assertion rows.
//!
//! A missing or ambiguous stored episode is `error` / exit 1 with a registered
//! reason code (`E_TRACE_EPISODE_MISSING` / `E_TRACE_EPISODE_AMBIGUOUS`).
//! The existing `details.assertions` object stays, and the row keeps the
//! `details.assertions_not_evaluated` companion 6.6.3 added. An evaluated
//! failure and a database failure stay `fail` / exit 1 / `E_TEST_FAILED`
//! with no companion. In a mixed run the evaluated failure is reported first.

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
    eval_with_assertion_for("no_forbidden_tool", "tidy")
}

fn eval_with_assertion_for(test_id: &str, prompt: &str) -> String {
    format!(
        r#"configVersion: 1
suite: "episode_diag"
model: "trace"
tests:
  - id: "{test_id}"
    input:
      prompt: "{prompt}"
    expected:
      type: regex_match
      pattern: "done"
    assertions:
      - type: trace_must_not_call_tool
        tool: "delete_repository"
"#
    )
}

fn eval_two_tests() -> String {
    r#"configVersion: 1
suite: "episode_diag_mixed"
model: "trace"
tests:
  - id: "test_a"
    input:
      prompt: "alpha"
    expected:
      type: regex_match
      pattern: "done"
    assertions:
      - type: trace_must_not_call_tool
        tool: "delete_repository"
  - id: "test_b"
    input:
      prompt: "beta"
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
    assay_with_exit_codes(dir, "v2")
}

fn assay_with_exit_codes(dir: &Path, version: &str) -> Command {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.current_dir(dir)
        .env("ASSAY_EXIT_CODES", version)
        .env("ASSAY_VCR_MODE", "off");
    cmd
}

fn run_suite(dir: &Path, trace: &str) -> Output {
    run_suite_with(dir, trace, &[])
}

fn run_suite_with(dir: &Path, trace: &str, extra: &[&str]) -> Output {
    run_config(dir, &eval_with_assertion(), trace, extra, &["run"])
}

fn run_config(dir: &Path, config: &str, trace: &str, extra: &[&str], command: &[&str]) -> Output {
    fs::write(dir.join("eval.yaml"), config).expect("eval.yaml");
    fs::write(dir.join("trace.jsonl"), trace).expect("trace.jsonl");
    let mut args: Vec<&str> = command.to_vec();
    args.extend([
        "--config",
        "eval.yaml",
        "--trace-file",
        "trace.jsonl",
        "--db",
        "eval.db",
        "--no-cache",
        "--format",
        "json",
    ]);
    args.extend(extra);
    assay(dir).args(args).output().expect("assay run")
}

/// Same run in text mode, so the console summary on stderr can be asserted.
fn run_suite_text(dir: &Path, trace: &str) -> Output {
    fs::write(dir.join("eval.yaml"), eval_with_assertion()).expect("eval.yaml");
    fs::write(dir.join("trace.jsonl"), trace).expect("trace.jsonl");
    assay(dir)
        .args([
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--db",
            "eval-text.db",
            "--no-cache",
        ])
        .output()
        .expect("assay run")
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

fn summary_json(dir: &Path) -> serde_json::Value {
    let text = fs::read_to_string(dir.join("summary.json")).expect("summary.json");
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("summary.json: {err}: {text}"))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn row_by_id<'a>(report: &'a serde_json::Value, test_id: &str) -> &'a serde_json::Value {
    report["results"]
        .as_array()
        .expect("results array")
        .iter()
        .find(|row| row["test_id"] == test_id)
        .unwrap_or_else(|| panic!("no row for {test_id}: {report}"))
}

/// B1: not-evaluated rows exit 1 with their own registered code and the row
/// remedy as the run `next_step` — not the explain step.
fn assert_not_evaluated_outcome(
    output: &Output,
    run: &serde_json::Value,
    code: &str,
    remedy: &str,
) {
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(output));
    assert_eq!(run["exit_code"], 1);
    assert_eq!(run["reason_code"], code);
    assert_eq!(run["reason_code_version"], 1);
    let next = run["resolution"]["next_step"].as_str().expect("next_step");
    assert!(
        next.contains(remedy),
        "run next_step carries the row remedy, got: {next}"
    );
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

/// The row as the downstream surfaces see it: JUnit `<error>`, SARIF `error`.
fn assert_error_surfaces(row: &serde_json::Value, test_id: &str) {
    let parsed: assay_core::model::TestResultRow =
        serde_json::from_value(row.clone()).expect("row deserializes");
    assert_eq!(
        parsed.status,
        assay_core::model::TestStatus::Error,
        "row status is the existing Error variant"
    );

    let dir = TempDir::new().expect("tempdir");
    let junit_path = dir.path().join("junit.xml");
    let rows = [parsed];
    assay_core::report::junit::write_junit("episode_diag", &rows, &junit_path).expect("junit");
    let junit = fs::read_to_string(&junit_path).expect("junit.xml");
    assert!(
        junit.contains("<error"),
        "JUnit renders a not-evaluated row as <error>: {junit}"
    );
    assert!(
        !junit.contains("<failure"),
        "JUnit must not render a not-evaluated row as <failure>: {junit}"
    );

    let sarif_path = dir.path().join("sarif.json");
    assay_core::report::sarif::write_sarif("assay", &rows, &sarif_path).expect("sarif");
    let sarif: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&sarif_path).expect("sarif.json"))
            .expect("sarif json");
    let levels: Vec<&str> = sarif["runs"][0]["results"]
        .as_array()
        .expect("sarif results")
        .iter()
        .filter_map(|r| r["level"].as_str())
        .collect();
    assert_eq!(
        levels,
        vec!["error"],
        "SARIF keeps level error for {test_id}: {sarif}"
    );
}

#[test]
fn missing_episode_is_error_exit_1_with_its_registered_code() {
    let dir = TempDir::new().expect("tempdir");
    let output = run_suite(
        dir.path(),
        &episode("ep-1", "other_test", "tidy", "list_files"),
    );
    let report = parse_json(&output.stdout);
    let run = run_json(dir.path());
    let summary = summary_json(dir.path());
    assert_not_evaluated_outcome(&output, &run, "E_TRACE_EPISODE_MISSING", MISSING_REMEDY);

    let row = &report["results"][0];
    assert_eq!(row["test_id"], "no_forbidden_tool");
    assert_eq!(row["status"], "error");
    assert_preserved_assertion_error(row);
    assert_row_diagnostic(row, MISSING_KIND, MISSING_REMEDY);
    assert_ne!(
        row["details"][ASSERTIONS_NOT_EVALUATED]["kind"],
        AMBIGUOUS_KIND
    );
    assert_error_surfaces(row, "no_forbidden_tool");
    assert_eq!(summary["results"]["failed"], 1);
    assert_eq!(summary["results"]["total"], 1);
}

#[test]
fn ambiguous_episode_is_error_exit_1_with_its_own_remedy() {
    let dir = TempDir::new().expect("tempdir");
    let trace = format!(
        "{}{}",
        episode("ep-1", "no_forbidden_tool", "tidy", "list_files"),
        episode("ep-2", "no_forbidden_tool", "other-prompt", "list_files")
    );
    let output = run_suite(dir.path(), &trace);
    let report = parse_json(&output.stdout);
    let run = run_json(dir.path());
    assert_not_evaluated_outcome(&output, &run, "E_TRACE_EPISODE_AMBIGUOUS", AMBIGUOUS_REMEDY);

    let row = &report["results"][0];
    assert_eq!(row["status"], "error");
    assert_preserved_assertion_error(row);
    assert_row_diagnostic(row, AMBIGUOUS_KIND, AMBIGUOUS_REMEDY);
    assert_ne!(
        row["details"][ASSERTIONS_NOT_EVALUATED]["remedy"],
        MISSING_REMEDY
    );
    assert_error_surfaces(row, "no_forbidden_tool");
}

#[test]
fn latest_stored_episode_fallback_miss_reports_missing_with_the_primary_message() {
    let dir = TempDir::new().expect("tempdir");
    let output = run_suite_with(
        dir.path(),
        &episode("ep-1", "other_test", "tidy", "list_files"),
        &["--latest-stored-episode"],
    );
    let report = parse_json(&output.stdout);
    let run = run_json(dir.path());
    assert_not_evaluated_outcome(&output, &run, "E_TRACE_EPISODE_MISSING", MISSING_REMEDY);

    let row = &report["results"][0];
    assert_eq!(row["status"], "error");
    assert_row_diagnostic(row, MISSING_KIND, MISSING_REMEDY);
    let message = row["message"].as_str().expect("message");
    assert!(
        message.contains("run_id="),
        "fallback miss keeps the primary lookup message: {message}"
    );
    assert!(
        !message.contains("fallback"),
        "fallback miss must not report the fallback message: {message}"
    );
}

#[test]
fn missing_episode_console_names_the_row_remedy() {
    let dir = TempDir::new().expect("tempdir");
    let output = run_suite_text(
        dir.path(),
        &episode("ep-1", "other_test", "tidy", "list_files"),
    );
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert!(
        err.contains("ERROR:"),
        "console marks the not-evaluated row: {err}"
    );
    assert!(
        err.contains(MISSING_REMEDY),
        "console carries the row remedy: {err}"
    );
}

#[test]
fn database_decoding_error_stays_fail_without_the_unevaluated_companion() {
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
    assert_eq!(row["status"], "fail");
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

#[test]
fn mixed_evaluated_failure_and_missing_reports_the_failure_first() {
    let dir = TempDir::new().expect("tempdir");
    // test_a has a stored episode that violates the assertion; test_b replays
    // (its prompt is in the trace) but has no stored episode of its own.
    let trace = format!(
        "{}{}",
        episode("ep-a", "test_a", "alpha", "delete_repository"),
        episode("ep-other", "other_test", "beta", "list_files"),
    );
    let output = run_config(dir.path(), &eval_two_tests(), &trace, &[], &["run"]);
    let report = parse_json(&output.stdout);
    let run = run_json(dir.path());
    let summary = summary_json(dir.path());
    assert_unchanged_failure(&output, &run);

    let row_a = row_by_id(&report, "test_a");
    assert_eq!(row_a["status"], "fail");
    assert!(
        row_a["details"].get(ASSERTIONS_NOT_EVALUATED).is_none(),
        "evaluated row carries no companion: {row_a}"
    );
    let row_b = row_by_id(&report, "test_b");
    assert_eq!(row_b["status"], "error");
    assert_row_diagnostic(row_b, MISSING_KIND, MISSING_REMEDY);
    assert_eq!(summary["results"]["failed"], 2);
    assert_eq!(summary["results"]["total"], 2);
}

#[test]
fn mixed_missing_and_ambiguous_reports_missing_with_counts() {
    let dir = TempDir::new().expect("tempdir");
    // test_a replays but has no stored episode; test_b has two stored episodes.
    let trace = format!(
        "{}{}{}",
        episode("ep-other", "other_test", "alpha", "list_files"),
        episode("ep-b1", "test_b", "beta", "list_files"),
        episode("ep-b2", "test_b", "beta-second", "list_files"),
    );
    let output = run_config(dir.path(), &eval_two_tests(), &trace, &[], &["run"]);
    let report = parse_json(&output.stdout);
    let run = run_json(dir.path());
    assert_not_evaluated_outcome(&output, &run, "E_TRACE_EPISODE_MISSING", MISSING_REMEDY);
    let message = run["resolution"]["message"].as_str().expect("message");
    assert_eq!(
        message, "assertions not evaluated for 2 test(s): 1 missing, 1 ambiguous",
        "{message}"
    );

    assert_eq!(row_by_id(&report, "test_a")["status"], "error");
    assert_eq!(row_by_id(&report, "test_b")["status"], "error");
    assert_row_diagnostic(row_by_id(&report, "test_a"), MISSING_KIND, MISSING_REMEDY);
    assert_row_diagnostic(
        row_by_id(&report, "test_b"),
        AMBIGUOUS_KIND,
        AMBIGUOUS_REMEDY,
    );
}

#[test]
fn not_evaluated_rows_exit_1_under_the_v1_profile_too() {
    let dir = TempDir::new().expect("tempdir");
    fs::write(
        dir.path().join("eval.yaml"),
        eval_with_assertion_for("no_forbidden_tool", "tidy"),
    )
    .expect("eval.yaml");
    fs::write(
        dir.path().join("trace.jsonl"),
        episode("ep-1", "other_test", "tidy", "list_files"),
    )
    .expect("trace.jsonl");
    let output = assay_with_exit_codes(dir.path(), "v1")
        .args([
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
        ])
        .output()
        .expect("assay run");
    let run = run_json(dir.path());
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(run["exit_code"], 1);
    assert_eq!(run["reason_code"], "E_TRACE_EPISODE_MISSING");
}

#[test]
fn ci_reports_the_same_not_evaluated_contract_as_run() {
    let dir = TempDir::new().expect("tempdir");
    let trace = episode("ep-1", "other_test", "tidy", "list_files");
    let output = run_config(dir.path(), &eval_with_assertion(), &trace, &[], &["ci"]);
    let run = run_json(dir.path());
    assert_not_evaluated_outcome(&output, &run, "E_TRACE_EPISODE_MISSING", MISSING_REMEDY);
    let row = row_by_id(&run, "no_forbidden_tool");
    assert_eq!(row["status"], "error");
    assert_row_diagnostic(row, MISSING_KIND, MISSING_REMEDY);

    // `ci` writes the downstream surfaces to disk: JUnit and SARIF included.
    let junit = fs::read_to_string(dir.path().join("junit.xml")).expect("junit.xml");
    assert!(
        junit.contains("<error"),
        "JUnit renders a not-evaluated row as <error>: {junit}"
    );
    assert!(
        !junit.contains("<failure"),
        "JUnit must not render a not-evaluated row as <failure>: {junit}"
    );
    let sarif: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join("sarif.json")).expect("sarif.json"),
    )
    .expect("sarif json");
    let levels: Vec<&str> = sarif["runs"][0]["results"]
        .as_array()
        .expect("sarif results")
        .iter()
        .filter_map(|r| r["level"].as_str())
        .collect();
    assert_eq!(levels, vec!["error"], "SARIF keeps level error: {sarif}");
}
