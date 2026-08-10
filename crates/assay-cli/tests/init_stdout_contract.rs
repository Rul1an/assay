//! `assay init` as seen by a caller that reads only stdout and the exit status (#2161).
//!
//! The measured gap was that a failing `init` left partial progress prose on stdout and put the
//! actionable diagnosis on stderr behind `fatal:`, so a caller reading stdout and the exit status
//! saw something that looked like a partial success. These tests drive the real binary and read
//! only stdout and the exit code, because that is the surface the contract promises.

#[path = "../../../tests/support/bounded_process.rs"]
mod bounded_process;

use std::path::Path;
use std::process::Command;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use serde_json::Value;

const INIT_REPORT: &str = "assay.init_report.v0";

struct Run {
    exit_code: i32,
    stdout: String,
}

fn init(dir: &Path, args: &[&str]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    command.current_dir(dir).arg("init").args(args);
    let output = run_bounded(command, b"", GOLDEN_PATH_LIMITS, "assay init").expect("assay init");
    Run {
        exit_code: output
            .status
            .code()
            .expect("assay init terminated without an exit code"),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
    }
}

/// Parses the whole of stdout, not a fragment of it.
///
/// `serde_json::from_str` over the entire stream is the assertion that no progress line survives
/// alongside the document. Searching stdout for a `{` and parsing from there would pass with the
/// human stream still printed above it, which is the defect this contract exists to prevent.
fn sole_document(run: &Run, context: &str) -> Value {
    let parsed: Value = serde_json::from_str(&run.stdout).unwrap_or_else(|error| {
        panic!(
            "{context}: stdout is not one JSON document: {error}; stdout was:\n{}",
            run.stdout
        )
    });
    assert_eq!(
        parsed["schema"], INIT_REPORT,
        "{context}: stdout document is not the init report"
    );
    parsed
}

#[test]
fn an_unknown_preset_publishes_a_registered_reason_and_a_next_step_on_stdout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let run = init(
        dir.path(),
        &["--preset", "not-a-preset", "--format", "json"],
    );

    assert_eq!(
        run.exit_code, 2,
        "an unusable argument is the config/user exit class"
    );
    let report = sole_document(&run, "unknown preset");
    assert_eq!(report["reason_code"], "E_INVALID_ARGS");
    assert_eq!(report["exit_code"], 2);
    let next_step = report["next_step"]
        .as_str()
        .expect("next_step must be a string on failure");
    assert!(
        !next_step.trim().is_empty(),
        "next_step must be non-empty on failure"
    );
    let message = report["message"]
        .as_str()
        .expect("message must be a string on failure");
    assert!(
        message.contains("not-a-preset"),
        "the message must name the rejected preset, got {message:?}"
    );
}

#[test]
fn a_successful_init_publishes_the_files_it_created_and_an_empty_reason_code() {
    let dir = tempfile::tempdir().expect("tempdir");
    let run = init(
        dir.path(),
        &["--preset", "dev", "--hello-trace", "--format", "json"],
    );

    assert_eq!(run.exit_code, 0);
    let report = sole_document(&run, "init success");
    assert_eq!(
        report["reason_code"], "",
        "success carries the empty reason code, as the run summary does"
    );
    assert_eq!(report["exit_code"], 0);

    let created: Vec<&str> = report["created"]
        .as_array()
        .expect("created must be an array")
        .iter()
        .map(|entry| entry.as_str().expect("created entry is a string"))
        .collect();
    for expected in ["policy.yaml", "eval.yaml", "traces/hello.jsonl"] {
        assert!(
            created.contains(&expected),
            "created must name {expected}, got {created:?}"
        );
        assert!(
            dir.path().join(expected).is_file(),
            "{expected} is reported as created but is not on disk"
        );
    }
    assert!(
        report["next_step"]
            .as_str()
            .is_some_and(|step| step.contains("validate")),
        "success must point at the next command, got {:?}",
        report["next_step"]
    );
}

/// The report describes the directory it left behind, including a failure that wrote files first.
///
/// Without this, a caller that reads only the failure envelope cannot tell an `init` that touched
/// nothing from one that wrote `policy.yaml` and then failed, which is the difference between
/// retrying and cleaning up first.
#[test]
fn a_failure_after_partial_work_still_reports_what_it_wrote() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("policy.yaml"), "existing: true\n").expect("seed policy.yaml");

    let run = init(
        dir.path(),
        &["--preset", "not-a-preset", "--format", "json"],
    );

    assert_eq!(run.exit_code, 2);
    let report = sole_document(&run, "failure after partial work");
    assert_eq!(report["reason_code"], "E_INVALID_ARGS");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("policy.yaml")).expect("policy.yaml survives"),
        "existing: true\n",
        "a failing init must not overwrite an existing policy"
    );
}

/// Text stays the default and stays human.
///
/// The issue asked for a machine-readable path *without* changing the existing default output, so
/// this pins the default as prose that is not a JSON document, in both the success and the failure
/// direction.
#[test]
fn text_is_still_the_default_and_still_a_human_progress_stream() {
    let success_dir = tempfile::tempdir().expect("success tempdir");
    let success = init(success_dir.path(), &["--preset", "dev", "--hello-trace"]);
    assert_eq!(success.exit_code, 0);
    assert!(
        success.stdout.contains("Next: assay validate"),
        "default success output lost its human next step: {:?}",
        success.stdout
    );
    assert!(
        serde_json::from_str::<Value>(&success.stdout).is_err(),
        "the default output became a JSON document; --format json is what opts into that"
    );

    let failure_dir = tempfile::tempdir().expect("failure tempdir");
    let failure = init(failure_dir.path(), &["--preset", "not-a-preset"]);
    assert_eq!(
        failure.exit_code, 2,
        "the default failure exit is unchanged"
    );
    assert!(
        serde_json::from_str::<Value>(&failure.stdout).is_err(),
        "the default failure output became a JSON document"
    );
}
