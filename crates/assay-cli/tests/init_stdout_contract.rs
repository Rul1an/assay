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

/// A classified failure reports no files, because every classified failure happens before the
/// first write.
///
/// This test was named `a_failure_after_partial_work_still_reports_what_it_wrote` and asserted
/// nothing of the kind: deleting `created` and `skipped` from the emitted document left it green.
/// The name described a capability no reachable path has. Both `InitReport::fail` call sites sit
/// upstream of every `record_created`/`record_skipped`, so a failing document's file lists are
/// structurally always empty, and a failure that happens *after* a write leaves `?`/`bail!` and
/// emits no document at all — the limit stated as a non-claim rather than papered over.
///
/// So the checkable rule is the structural one, and it is worth pinning in this direction: if a
/// future `fail` site lands after a write, these lists stop being empty and this test fires,
/// which forces the choice between reporting the partial work and keeping the limit honest.
#[test]
fn a_classified_failure_reports_no_files_because_it_fails_before_the_first_write() {
    for (args, reason) in [
        (
            vec!["--preset", "not-a-preset", "--format", "json"],
            "E_INVALID_ARGS",
        ),
        (
            vec!["--from-trace", "absent.jsonl", "--format", "json"],
            "E_TRACE_NOT_FOUND",
        ),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("policy.yaml"), "existing: true\n")
            .expect("seed policy.yaml");

        let run = init(dir.path(), &args);

        assert_eq!(run.exit_code, 2, "{reason} exits 2");
        let report = sole_document(&run, reason);
        assert_eq!(report["reason_code"], reason);
        assert_eq!(
            report["created"].as_array().expect("created is an array"),
            &Vec::<serde_json::Value>::new(),
            "{reason} reported files it created, so a failure now happens after a write and the \
             document must either carry that work or stop claiming to describe the directory"
        );
        assert_eq!(
            report["skipped"].as_array().expect("skipped is an array"),
            &Vec::<serde_json::Value>::new(),
            "{reason} reported skipped files, so this failure now runs past the first write"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("policy.yaml")).expect("policy.yaml survives"),
            "existing: true\n",
            "a failing init must not overwrite an existing policy"
        );
    }
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

/// Where the default text stream stops describing the host and starts describing this run.
///
/// Everything above it reports what MCP configuration the machine happens to have, so it is not
/// the same bytes on a developer's laptop and on a runner. Everything below it is a function of
/// the arguments and the directory alone.
const GENERATING: &str = "🏗️  Generating Assay Policy & Config...";

const CREATED_WITH_HELLO_TRACE: &str = "🏗️  Generating Assay Policy & Config...
   Created policy.yaml (preset: dev)
   Created eval.yaml
   Created traces/hello.jsonl
✅  Initialization complete.
   Note: hello trace uses demo prompt/response text only; treat real traces as potentially sensitive.
   Next: assay validate --config=eval.yaml --trace-file=traces/hello.jsonl --format json
";

const SKIPPED_WITH_HELLO_TRACE: &str = "🏗️  Generating Assay Policy & Config...
   Skipped policy.yaml (exists)
   Skipped eval.yaml (exists)
   Skipped traces/hello.jsonl (exists)
✅  Initialization complete.
   Note: hello trace uses demo prompt/response text only; treat real traces as potentially sensitive.
   Next: assay validate --config=eval.yaml --trace-file=traces/hello.jsonl --format json
";

const CREATED_WITHOUT_HELLO_TRACE: &str = "🏗️  Generating Assay Policy & Config...
   Created policy.yaml (preset: default)
   Created eval.yaml
✅  Initialization complete.
   Next: assay validate
";

/// The part of stdout that this run is responsible for.
fn own_output(stdout: &str) -> &str {
    let start = stdout
        .find(GENERATING)
        .unwrap_or_else(|| panic!("stdout never reaches {GENERATING:?}; stdout was:\n{stdout}"));
    &stdout[start..]
}

/// The default text stream is byte for byte what it was, in the part a run controls.
///
/// The claim that this change leaves the default output alone was measured once, by building the
/// binary on both trees and diffing their stdout over twenty-one invocations. That is the right
/// way to make the claim and the wrong way to keep it: it does not survive into the next edit of
/// the closing block. Swapping the `Note:` and `Next:` lines — which restructuring that block is
/// exactly in a position to do — passed every other test in this crate, because the only
/// assertions on this stream ask whether `Next: assay validate` appears somewhere in it.
///
/// So the stream is pinned instead of described. What is pinned is the ordering of the closing
/// block, the rendering of a created and a skipped file, and the two shapes of the next-step line.
#[test]
fn the_default_text_stream_is_pinned_where_the_run_controls_it() {
    let created_dir = tempfile::tempdir().expect("created tempdir");
    let created = init(created_dir.path(), &["--preset", "dev", "--hello-trace"]);
    assert_eq!(created.exit_code, 0);
    assert_eq!(own_output(&created.stdout), CREATED_WITH_HELLO_TRACE);

    // The same directory again, so every file is a `Skipped … (exists)` line instead.
    let skipped = init(created_dir.path(), &["--preset", "dev", "--hello-trace"]);
    assert_eq!(skipped.exit_code, 0);
    assert_eq!(own_output(&skipped.stdout), SKIPPED_WITH_HELLO_TRACE);

    // Without `--hello-trace` there is no trace to caveat, so the closing block is one line.
    let plain_dir = tempfile::tempdir().expect("plain tempdir");
    let plain = init(plain_dir.path(), &[]);
    assert_eq!(plain.exit_code, 0);
    assert_eq!(own_output(&plain.stdout), CREATED_WITHOUT_HELLO_TRACE);
}
