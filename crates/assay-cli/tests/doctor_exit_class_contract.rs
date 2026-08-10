//! `assay doctor` must return the same exit class on both channels for the same tree (#2215).
//!
//! Before this contract existed the exit class depended on a *formatting* flag: the text branch
//! computed `has_errors` from the diagnostics it had just printed and returned `1`, while the JSON
//! branch returned `Ok(0)` unconditionally once the config loaded. One command, two answers about
//! one tree.
//!
//! The sharper half is which channel got the wrong answer. `AGENTS.md` forbids turning a failed
//! validation into a clean result, and the channel that did it is the one an agent reads: a machine
//! driving the golden path saw exit `0`, treated preflight as passed, and proceeded — while the
//! document it had just been handed said an input it needed did not exist. There was no error to
//! notice, only a `0`.
//!
//! The negative control matters as much as the defect. "Return 1 whenever any diagnostic exists"
//! would pass the first test and be wrong, because a warning is not a failed validation. So a tree
//! whose only diagnostic is `warn` must still exit `0` on both channels, and both tests read the
//! exit code alone — the same thing the consumer keys on.

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use std::path::Path;
use std::process::Command;

/// A config that loads cleanly, so every difference below comes from the diagnostics rather than
/// from the load path that `doctor_config_check_contract.rs` already pins.
const EVAL_YAML: &str = r#"configVersion: 1
suite: "exit_class"
model: "trace"
tests:
  - id: "exit_class_regex"
    input:
      prompt: "hello_prompt"
    expected:
      type: regex_match
      pattern: "Hello\\s+Assay"
      flags: ["i"]
"#;

/// Satisfies the test's prompt, so no `E_TRACE_MISS` error is raised.
const MATCHING_TRACE: &str = r#"{"schema_version": 1, "type": "assay.trace", "request_id": "hello_1", "prompt": "hello_prompt", "response": "Hello Assay", "model": "trace", "provider": "trace"}
"#;

/// A second entry in the legacy OpenAI shape. `analyze_trace_schema` raises `E_TRACE_INVALID` at
/// `warn` for it, which is how this fixture gets a diagnostic that is not an error.
const LEGACY_LINE: &str = r#"{"schema_version": 1, "type": "assay.trace", "request_id": "legacy_1", "prompt": "p2", "response": "r", "model": "trace", "provider": "trace", "function_call": {"name": "x"}}
"#;

/// Runs `doctor` in `cwd` and returns its exit code. Both channels go through this one function so
/// a difference in the result cannot come from a difference in how they were driven.
fn doctor_exit_code(cwd: &Path, format: &str, trace: &str) -> i32 {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_")
        {
            command.env_remove(name);
        }
    }
    command
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .args(["doctor", "--format", format])
        .args(["--config", "eval.yaml"])
        .args(["--trace-file", trace]);
    let output = run_bounded(command, b"", GOLDEN_PATH_LIMITS, "assay doctor").expect("doctor ran");
    output
        .status
        .code()
        .expect("doctor exited by code rather than by signal")
}

/// Writes a tree whose config loads, and returns its path.
fn tree(dir: &Path, trace_body: &str) {
    std::fs::write(dir.join("eval.yaml"), EVAL_YAML).expect("wrote eval.yaml");
    std::fs::create_dir_all(dir.join("traces")).expect("created traces/");
    std::fs::write(dir.join("traces/present.jsonl"), trace_body).expect("wrote trace");
}

#[test]
fn both_channels_agree_when_a_validation_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree(dir.path(), MATCHING_TRACE);

    // The trace this names does not exist, so `validate` raises `E_PATH_NOT_FOUND` at `error`.
    let text = doctor_exit_code(dir.path(), "text", "traces/absent.jsonl");
    let json = doctor_exit_code(dir.path(), "json", "traces/absent.jsonl");
    // `E_PATH_NOT_FOUND` is `ExitClass::Config` in the ADR-046 table, so `decide_exit` answers 2.
    // `assay-cli` is bin-only, so an integration test cannot import the constant; the class table
    // is pinned in-crate, and what this test owns is that both channels get the same answer.
    let expected = 2;

    assert_eq!(
        json, text,
        "doctor returned exit {json} on the JSON channel and exit {text} on the text channel for \
         one tree. The exit class is a property of what was checked, not of how it was printed."
    );
    assert_eq!(
        json, expected,
        "doctor returned exit {json} while publishing an error-severity diagnostic. A failed \
         validation published as a clean result is the one thing the machine channel must never do. \
         The class itself comes from `decide_exit`, so `assay validate` and `assay run` answer \
         {expected} for this same diagnostic and doctor does not get a third answer."
    );
}

#[test]
fn a_warning_is_not_a_failed_validation_on_either_channel() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree(dir.path(), &format!("{MATCHING_TRACE}{LEGACY_LINE}"));

    let text = doctor_exit_code(dir.path(), "text", "traces/present.jsonl");
    let json = doctor_exit_code(dir.path(), "json", "traces/present.jsonl");

    assert_eq!(
        json, text,
        "doctor returned exit {json} on the JSON channel and exit {text} on the text channel for \
         one tree."
    );
    assert_eq!(
        json, 0,
        "doctor returned exit {json} for a tree whose only diagnostic is a warning. This is the \
         control on the fix above: keying the exit class on 'any diagnostic exists' rather than on \
         error severity would pass that test and turn every advisory into a failure here."
    );
}
