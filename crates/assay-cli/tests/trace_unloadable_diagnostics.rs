//! #3117 minor (6.7.0, variant B1) contract for unloadable traces.
//!
//! A trace file that exists but is not a loadable replay trace (duplicate
//! prompt, duplicate `request_id`, malformed line) reports the registered
//! `E_TRACE_UNLOADABLE` at the unchanged exit 2 — never `E_TRACE_NOT_FOUND`.
//! A genuinely missing file stays `E_TRACE_NOT_FOUND`, now naming the real
//! path instead of the `<trace.jsonl>` placeholder.

use assert_cmd::Command;
use std::fs;
use std::path::Path;
use std::process::Output;
use tempfile::TempDir;

/// Config without `assertions:`, so no ingest runs and the failure under test
/// is the replay-trace loader itself.
fn eval_without_assertions() -> String {
    r#"configVersion: 1
suite: "loader_diag"
model: "trace"
tests:
  - id: "hello"
    input:
      prompt: "hello"
    expected:
      type: regex_match
      pattern: "hi"
"#
    .to_string()
}

fn assay(dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.current_dir(dir)
        .env("ASSAY_EXIT_CODES", "v2")
        .env("ASSAY_VCR_MODE", "off");
    cmd
}

fn run_with_trace(dir: &Path, trace_name: &str) -> Output {
    assay(dir)
        .args([
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            trace_name,
            "--db",
            "eval.db",
            "--no-cache",
            "--format",
            "json",
        ])
        .output()
        .expect("assay run")
}

fn setup(dir: &Path, trace: &str) -> Output {
    fs::write(dir.join("eval.yaml"), eval_without_assertions()).expect("eval.yaml");
    fs::write(dir.join("trace.jsonl"), trace).expect("trace.jsonl");
    run_with_trace(dir, "trace.jsonl")
}

fn run_json(dir: &Path) -> serde_json::Value {
    let text = fs::read_to_string(dir.join("run.json")).expect("run.json");
    serde_json::from_str(&text).unwrap_or_else(|err| panic!("run.json: {err}: {text}"))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Exit stays 2 for the whole loader family; only the code changes.
fn assert_unloadable(output: &Output, run: &serde_json::Value, detail: &str) {
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(output));
    assert_eq!(run["exit_code"], 2);
    assert_eq!(run["reason_code"], "E_TRACE_UNLOADABLE");
    assert_eq!(run["reason_code_version"], 1);
    let next = run["resolution"]["next_step"].as_str().expect("next_step");
    assert!(
        next.contains(detail),
        "unloadable next_step names the loader detail, got: {next}"
    );
    let err = stderr(output);
    assert!(
        !err.contains("E_TRACE_NOT_FOUND"),
        "an existing file must not report trace-not-found: {err}"
    );
    assert!(
        !run.to_string().contains("E_TRACE_NOT_FOUND"),
        "run.json must not report trace-not-found: {run}"
    );
}

#[test]
fn duplicate_prompt_is_unloadable_not_not_found() {
    let dir = TempDir::new().expect("tempdir");
    let output = setup(
        dir.path(),
        concat!(
            "{\"prompt\":\"same-prompt\",\"response\":\"a\"}\n",
            "{\"prompt\":\"same-prompt\",\"response\":\"b\"}\n",
        ),
    );
    let run = run_json(dir.path());
    assert_unloadable(&output, &run, "same-prompt");
}

#[test]
fn duplicate_request_id_is_unloadable() {
    let dir = TempDir::new().expect("tempdir");
    let output = setup(
        dir.path(),
        concat!(
            "{\"prompt\":\"p-one\",\"response\":\"a\",\"request_id\":\"r-1\"}\n",
            "{\"prompt\":\"p-two\",\"response\":\"b\",\"request_id\":\"r-1\"}\n",
        ),
    );
    let run = run_json(dir.path());
    assert_unloadable(&output, &run, "r-1");
}

#[test]
fn malformed_line_is_unloadable() {
    let dir = TempDir::new().expect("tempdir");
    let output = setup(
        dir.path(),
        concat!(
            "{\"prompt\":\"p-one\",\"response\":\"a\"}\n",
            "this is not json{{{\n",
        ),
    );
    let run = run_json(dir.path());
    assert_unloadable(&output, &run, "line 2");
}

#[test]
fn non_utf8_trace_line_is_unloadable_with_the_real_path() {
    use std::io::Write as _;

    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("eval.yaml"), eval_without_assertions()).expect("eval.yaml");
    // Reproduction bytes (#3117 follow-up): a valid first line, then a line
    // that is not valid UTF-8. The file exists, so this is unloadable —
    // never not-found — and it names the real trace path.
    let mut trace = fs::File::create(dir.path().join("trace.jsonl")).expect("trace.jsonl");
    trace
        .write_all(b"{\"prompt\":\"tidy\",\"response\":\"done\"}\nbad \xff line\n")
        .expect("trace bytes");
    drop(trace);
    let output = run_with_trace(dir.path(), "trace.jsonl");
    let run = run_json(dir.path());
    assert_unloadable(&output, &run, "line 2");
    let next = run["resolution"]["next_step"].as_str().expect("next_step");
    assert!(
        next.contains("trace.jsonl"),
        "unloadable next_step names the real path, got: {next}"
    );
    assert!(
        !next.contains("<trace.jsonl>"),
        "the placeholder path is gone: {next}"
    );
}

#[test]
fn missing_trace_file_stays_not_found_with_the_real_path() {
    let dir = TempDir::new().expect("tempdir");
    fs::write(dir.path().join("eval.yaml"), eval_without_assertions()).expect("eval.yaml");
    let output = run_with_trace(dir.path(), "absent-trace.jsonl");
    let run = run_json(dir.path());
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert_eq!(run["exit_code"], 2);
    assert_eq!(run["reason_code"], "E_TRACE_NOT_FOUND");
    let next = run["resolution"]["next_step"].as_str().expect("next_step");
    assert!(
        next.contains("absent-trace.jsonl"),
        "not-found next_step names the real path, got: {next}"
    );
    assert!(
        !next.contains("<trace.jsonl>"),
        "the placeholder path is gone: {next}"
    );
}
