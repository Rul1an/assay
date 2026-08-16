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
//!
//! The second pair of tests pins the same rule across `--fix`, which is where the divergence
//! reappeared once `--format` stopped producing one. `doctor/fixes.rs` recomputed the
//! error-severity predicate by hand and turned the count into a literal `1`, so routing the two
//! output channels through `decide_exit` — which reads the ADR-046 class table and answers `2` for
//! a config-class code — made the two *flag* paths disagree about one tree instead. These tests
//! assert the two runs equal each other rather than a literal, because what the contract owns is
//! that one tree gets one class, not which number that class currently is.

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use std::ffi::OsStr;
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

/// A config that does not load, whose parse error `--fix` can repair, and which still does not load
/// after that repair.
///
/// `inputs` is close enough to `input` for `try_fix_parse_error`'s 0.80 similarity gate to rename
/// it; `bogusfield` is close to nothing, so the second load fails too. Field order matters, because
/// serde reports the first unknown field it meets and the repair only ever renames that one.
const REPAIRABLE_THEN_STILL_BROKEN: &str = r#"configVersion: 1
suite: "exit_class"
model: "trace"
tests:
  - id: "exit_class_regex"
    inputs:
      prompt: "hello_prompt"
    bogusfield: 1
    expected:
      type: regex_match
      pattern: "Hello"
"#;

/// A second entry in the legacy OpenAI shape. `analyze_trace_schema` raises `E_TRACE_INVALID` at
/// `warn` for it, which is how this fixture gets a diagnostic that is not an error.
const LEGACY_LINE: &str = r#"{"schema_version": 1, "type": "assay.trace", "request_id": "legacy_1", "prompt": "p2", "response": "r", "model": "trace", "provider": "trace", "function_call": {"name": "x"}}
"#;

/// Runs `doctor` in `cwd` and returns its exit code. Every channel and flag path goes through this
/// one function so a difference in the result cannot come from a difference in how they were driven.
fn doctor_exit_code_with(cwd: &Path, format: &str, trace: &str, extra: &[&str]) -> i32 {
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
        .args(["--trace-file", trace])
        .args(extra);
    let output = run_bounded(command, b"", GOLDEN_PATH_LIMITS, "assay doctor").expect("doctor ran");
    output
        .status
        .code()
        .expect("doctor exited by code rather than by signal")
}

fn doctor_exit_code(cwd: &Path, format: &str, trace: &str) -> i32 {
    doctor_exit_code_with(cwd, format, trace, &[])
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

/// The same property one flag over: `--fix` must not change the class either.
///
/// This is deliberately not written as `assert_eq!(fixing, 2)`. What the contract owns is that one
/// tree gets one class, so the assertion compares the two runs to each other. If the class for this
/// diagnostic is ever renegotiated, both sides move together and this test keeps holding — it pins
/// the rule rather than today's answer to it.
#[test]
fn the_exit_class_does_not_depend_on_whether_a_fix_was_requested() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree(dir.path(), MATCHING_TRACE);

    let plain = doctor_exit_code(dir.path(), "text", "traces/absent.jsonl");
    let fixing = doctor_exit_code_with(
        dir.path(),
        "text",
        "traces/absent.jsonl",
        &["--fix", "--dry-run"],
    );

    assert!(
        !dir.path().join("traces/absent.jsonl").exists(),
        "--dry-run created the file it previewed, so the two runs above did not see the same tree \
         and the comparison below would not mean what it says."
    );
    assert_eq!(
        fixing, plain,
        "doctor returned exit {plain} and `doctor --fix --dry-run` returned exit {fixing} for one \
         tree. `--fix` says what to attempt about a diagnostic, not what class the diagnostic has, \
         so a repair flag must not reclassify it: the class comes from `decide_exit` on both paths."
    );
}

/// A config that a repair changed and that still does not load gets one class, whichever repair ran.
///
/// Two sites answer this one question. `doctor/fixes.rs` re-loads the config after applying
/// diagnostic repairs; `doctor/parse_error.rs` re-loads it after renaming a misspelled key. Both
/// print "still invalid"/"still has issues" and both then have to say what that is worth. They said
/// `1` and, once `fixes.rs` moved to the reason code for an unloadable config, `2` — a divergence
/// this branch introduced, in the defect class this branch exists to remove.
///
/// The five other returns in `parse_error.rs` are a different question — no repair was applied, so
/// they report the outcome of an offer rather than the state of a config a repair changed — and
/// [#2209](https://github.com/Rul1an/assay/issues/2209) owns them.
///
/// The tree is measured before the repair runs, because `--fix --yes` rewrites `eval.yaml`: two
/// directories, identical content, so the comparison is between two runs and not between a run and a
/// number.
#[test]
fn a_repair_that_leaves_the_config_unloadable_does_not_change_the_exit_class() {
    let plain_dir = tempfile::tempdir().expect("tempdir");
    let fixing_dir = tempfile::tempdir().expect("tempdir");
    for dir in [plain_dir.path(), fixing_dir.path()] {
        std::fs::write(dir.join("eval.yaml"), REPAIRABLE_THEN_STILL_BROKEN).expect("wrote config");
        std::fs::create_dir_all(dir.join("traces")).expect("created traces/");
        std::fs::write(dir.join("traces/present.jsonl"), MATCHING_TRACE).expect("wrote trace");
    }

    let plain = doctor_exit_code(plain_dir.path(), "text", "traces/present.jsonl");
    let fixing = doctor_exit_code_with(
        fixing_dir.path(),
        "text",
        "traces/present.jsonl",
        &["--fix", "--yes"],
    );

    let after = std::fs::read_to_string(fixing_dir.path().join("eval.yaml")).expect("read config");
    assert!(
        after.contains("input:") && after.contains("bogusfield"),
        "the parse repair did not run, or it repaired more than the first key, so this test is no \
         longer driving a config that a repair changed and that still does not load."
    );
    assert_eq!(
        fixing, plain,
        "doctor returned exit {plain} for a config that does not load and exit {fixing} once a \
         repair had rewritten it and it still did not load. Both are the same condition — this \
         config does not load — so both report the class the reason code gives it."
    );
}

/// The condition `--dry-run` cannot reach: a repair that is attempted and fails to apply.
///
/// The parity test above drives `--dry-run`, where `failed` is structurally always zero — every
/// dry-run branch previews and counts the op as applied. So the `failed > 0` return was reachable
/// only with `--yes` against a target that cannot be written, and it returned a literal `1` while
/// `doctor` returned `2` for the very same diagnostic. Independent review found it after the first
/// fix landed, which is the argument for driving the branch rather than reasoning about it.
///
/// The tree makes the repair fail without any permission trickery: the trace path names a child of
/// `traces/present.jsonl`, which is a regular file, so `create_dir_all` on its parent cannot
/// succeed. That keeps the fixture portable — a read-only directory behaves differently for root,
/// and CI containers do run as root.
#[test]
fn a_repair_that_cannot_be_applied_does_not_change_the_exit_class() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree(dir.path(), MATCHING_TRACE);
    let unwritable = "traces/present.jsonl/nested.jsonl";

    let plain = doctor_exit_code(dir.path(), "text", unwritable);
    let fixing = doctor_exit_code_with(dir.path(), "text", unwritable, &["--fix", "--yes"]);

    assert!(
        !dir.path().join(unwritable).exists(),
        "the repair was expected to fail, but the target exists, so this test is no longer driving \
         the failed-to-apply branch."
    );
    assert_eq!(
        fixing, plain,
        "doctor returned exit {plain} and `doctor --fix --yes` returned exit {fixing} for one tree \
         whose repair could not be applied. A repair that fails is a reason to report a fault, not \
         a reason to report a different fault than the one the diagnostic has: `assay fix` answers \
         this same condition with the config-error class, and so must this."
    );
}

/// The control for the test above, and the reason it cannot be satisfied by a constant.
///
/// Returning the non-`--fix` class unconditionally from the `--fix` path would pass the parity
/// assertion and turn a warning into a failure here.
#[test]
fn a_warning_stays_clean_when_a_fix_is_requested() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree(dir.path(), &format!("{MATCHING_TRACE}{LEGACY_LINE}"));

    let plain = doctor_exit_code(dir.path(), "text", "traces/present.jsonl");
    let fixing = doctor_exit_code_with(
        dir.path(),
        "text",
        "traces/present.jsonl",
        &["--fix", "--dry-run"],
    );

    assert_eq!(
        fixing, plain,
        "doctor returned exit {plain} and `doctor --fix --dry-run` returned exit {fixing} for one \
         warn-only tree."
    );
    assert_eq!(
        fixing, 0,
        "`doctor --fix --dry-run` returned exit {fixing} for a tree whose only diagnostic is a \
         warning. An advisory is not a failed validation on any flag path."
    );
}

/// The third `--fix` branch: a repair that applies and leaves an error-severity diagnostic behind.
///
/// `doctor_fix_e2e.rs` drives this same path for its filesystem effect and asserts only that the
/// exit is non-zero, because the class belongs here rather than in two places. That leaves the
/// class on this branch pinned nowhere unless this test exists — `--fix --yes` creates the missing
/// trace file, the empty file still does not satisfy the test's prompt, and the re-validated report
/// therefore carries an error the repair did not resolve.
#[test]
fn a_repair_that_leaves_an_error_behind_does_not_change_the_exit_class() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree(dir.path(), MATCHING_TRACE);
    let absent = "traces/absent.jsonl";

    let plain = doctor_exit_code(dir.path(), "text", absent);
    let fixing = doctor_exit_code_with(dir.path(), "text", absent, &["--fix", "--yes"]);

    assert!(
        dir.path().join(absent).exists(),
        "`--fix --yes` was expected to create the missing trace file; without that this test is no \
         longer driving the applied-then-re-validated branch."
    );
    assert_eq!(
        fixing, plain,
        "doctor returned exit {plain} and `doctor --fix --yes` returned exit {fixing} for one tree. \
         A repair that runs and leaves an error-severity diagnostic in place reports the class that \
         diagnostic has."
    );
}

/// Drive `assay {command} --format {format}` with one raw-argv path flag.
///
/// The existing `&str` helper cannot name the argv the abort is measured on.
/// This sibling keeps that helper's env scrub and bounded spawn, and returns
/// the full `Output` so the caller can distinguish exit-by-code from signal
/// death instead of collapsing the latter into an expect panic that would
/// hide the diagnosis this issue is about.
#[cfg(unix)]
fn assay_output_with_os_flag(
    cwd: &Path,
    command: &str,
    format: &str,
    extra: &[&str],
    flag: &str,
    value: &OsStr,
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_")
        {
            cmd.env_remove(name);
        }
    }
    cmd.current_dir(cwd)
        .env("NO_COLOR", "1")
        .args([command, "--format", format])
        .args(extra)
        .arg(flag)
        .arg(value);
    run_bounded(cmd, b"", GOLDEN_PATH_LIMITS, command).unwrap_or_else(|error| panic!("{error}"))
}

#[cfg(unix)]
fn defined_exit(name: &str, output: &std::process::Output) -> i32 {
    output.status.code().unwrap_or_else(|| {
        panic!(
            "{name} died by signal {:?} (not a defined exit class). \
             This is the #2264 abort: serde Path error -> json! unwrap -> panic=abort. \
             stdout={}B stderr={}B\nstdout:\n{}\nstderr:\n{}",
            output.status,
            output.stdout.len(),
            output.stderr.len(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[cfg(unix)]
fn assert_no_panic_in_stderr(name: &str, output: &std::process::Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked at") && !stderr.contains("invalid UTF-8"),
        "{name} stderr still shows a panic abort:\n{stderr}"
    );
}

#[cfg(unix)]
fn non_utf8_os(prefix: &[u8], suffix: &[u8]) -> std::ffi::OsString {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let mut bytes = prefix.to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(suffix);
    OsString::from_vec(bytes)
}

/// A non-UTF-8 `--trace-file` must exit by a defined code, not SIGABRT (#2264).
///
/// The measured abort is `json!({ "path": path })` on a `PathBuf` whose
/// `to_str()` is `None`. serde returns `Error`; `json!` unwraps; `panic = "abort"`
/// kills the process. The file need not exist: the measured path is
/// `exists() == false`. Drive is argv bytes, not an APFS/ext4 create.
///
/// Windows is a non-claim here: `PathBuf` is WTF-16 and this `from_vec(0xff)`
/// construction is Unix-only.
#[cfg(unix)]
#[test]
fn a_non_utf8_trace_file_exits_by_code_and_diagnoses_path_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree(dir.path(), MATCHING_TRACE);

    let bad = non_utf8_os(b"traces/", b".jsonl");
    assert!(
        !dir.path().join(&bad).exists(),
        "the measured path is exists()==false; creating the file would miss the abort site"
    );

    let extra = ["--config", "eval.yaml"];
    let json =
        assay_output_with_os_flag(dir.path(), "doctor", "json", &extra, "--trace-file", &bad);
    let text =
        assay_output_with_os_flag(dir.path(), "doctor", "text", &extra, "--trace-file", &bad);
    let validate =
        assay_output_with_os_flag(dir.path(), "validate", "json", &extra, "--trace-file", &bad);

    let json_code = defined_exit("doctor --format json --trace-file", &json);
    let text_code = defined_exit("doctor --format text --trace-file", &text);
    let validate_code = defined_exit("validate --format json --trace-file", &validate);

    assert_ne!(
        json_code, 134,
        "exit 134 is SIGABRT reported as a shell-style code; the process must exit by a defined class"
    );
    assert_ne!(
        text_code, 134,
        "exit 134 is SIGABRT reported as a shell-style code"
    );
    assert_ne!(
        validate_code, 134,
        "exit 134 is SIGABRT reported as a shell-style code"
    );

    // `E_PATH_NOT_FOUND` is `ExitClass::Config`, so `decide_exit` answers 2.
    // The class itself is pinned in-crate; this test owns that a non-UTF-8
    // missing path still gets that diagnosis rather than a signal death.
    let expected = 2;
    assert_eq!(
        json_code, text_code,
        "doctor returned exit {json_code} on the JSON channel and exit {text_code} on the text \
         channel for one non-UTF-8 missing --trace-file. The exit class is a property of what \
         was checked, not of how it was printed."
    );
    assert_eq!(
        json_code, expected,
        "doctor returned exit {json_code} for a missing non-UTF-8 --trace-file. \
         Diagnosis stays E_PATH_NOT_FOUND (exists() is already false)."
    );
    assert_eq!(
        validate_code, expected,
        "validate returned exit {validate_code} for the same missing non-UTF-8 --trace-file."
    );

    let json_stdout = String::from_utf8_lossy(&json.stdout);
    let json_stderr = String::from_utf8_lossy(&json.stderr);
    let text_stdout = String::from_utf8_lossy(&text.stdout);
    let text_stderr = String::from_utf8_lossy(&text.stderr);
    let validate_stdout = String::from_utf8_lossy(&validate.stdout);
    let validate_stderr = String::from_utf8_lossy(&validate.stderr);
    assert!(
        json_stdout.contains("E_PATH_NOT_FOUND"),
        "doctor --format json must still diagnose E_PATH_NOT_FOUND on the machine channel. \
         stdout:\n{json_stdout}\nstderr:\n{json_stderr}"
    );
    assert!(
        text_stdout.contains("E_PATH_NOT_FOUND"),
        "doctor --format text must still diagnose E_PATH_NOT_FOUND. \
         stdout:\n{text_stdout}\nstderr:\n{text_stderr}"
    );
    assert!(
        validate_stdout.contains("E_PATH_NOT_FOUND"),
        "validate --format json must still diagnose E_PATH_NOT_FOUND on the machine channel. \
         stdout:\n{validate_stdout}\nstderr:\n{validate_stderr}"
    );
    for (name, output) in [
        ("doctor json", &json),
        ("doctor text", &text),
        ("validate json", &validate),
    ] {
        assert_no_panic_in_stderr(name, output);
    }
}

/// `--baseline` and `--config` are the other reachable raw-argv path flags (#2264).
///
/// `--trace-file` is pinned above, including doctor text. This sibling covers
/// the remaining flags with one helper, not a command matrix. The file need
/// not exist. Drive is `OsString::from_vec` with `0xff`.
///
/// `--baseline` is the same `exists() == false` site as `--trace-file`:
/// diagnosis stays `E_PATH_NOT_FOUND` on the machine channel.
/// `--config` load-fail is `E_CFG_PARSE` / config class — it is not
/// reclassified to `E_PATH_NOT_FOUND`. This test only proves no abort and a
/// defined exit for that flag.
#[cfg(unix)]
#[test]
fn a_non_utf8_baseline_and_config_exit_by_code_without_abort() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree(dir.path(), MATCHING_TRACE);

    let bad_baseline = non_utf8_os(b"baselines/", b".json");
    let bad_config = non_utf8_os(b"cfg-", b".yaml");
    assert!(
        !dir.path().join(&bad_baseline).exists() && !dir.path().join(&bad_config).exists(),
        "the measured paths are exists()==false; creating them would miss the abort site"
    );

    let extra = ["--config", "eval.yaml"];
    let doctor_baseline = assay_output_with_os_flag(
        dir.path(),
        "doctor",
        "json",
        &extra,
        "--baseline",
        &bad_baseline,
    );
    let validate_baseline = assay_output_with_os_flag(
        dir.path(),
        "validate",
        "json",
        &extra,
        "--baseline",
        &bad_baseline,
    );
    let doctor_config =
        assay_output_with_os_flag(dir.path(), "doctor", "json", &[], "--config", &bad_config);
    let validate_config =
        assay_output_with_os_flag(dir.path(), "validate", "json", &[], "--config", &bad_config);

    for (name, output) in [
        ("doctor --format json --baseline", &doctor_baseline),
        ("validate --format json --baseline", &validate_baseline),
        ("doctor --format json --config", &doctor_config),
        ("validate --format json --config", &validate_config),
    ] {
        let code = defined_exit(name, output);
        assert_ne!(
            code, 134,
            "{name} exit 134 is SIGABRT reported as a shell-style code"
        );
        assert_no_panic_in_stderr(name, output);
    }

    let doctor_baseline_out = String::from_utf8_lossy(&doctor_baseline.stdout);
    let doctor_baseline_err = String::from_utf8_lossy(&doctor_baseline.stderr);
    let validate_baseline_out = String::from_utf8_lossy(&validate_baseline.stdout);
    let validate_baseline_err = String::from_utf8_lossy(&validate_baseline.stderr);
    assert!(
        doctor_baseline_out.contains("E_PATH_NOT_FOUND"),
        "doctor --format json --baseline must still diagnose E_PATH_NOT_FOUND on the machine channel. \
         stdout:\n{doctor_baseline_out}\nstderr:\n{doctor_baseline_err}"
    );
    assert!(
        validate_baseline_out.contains("E_PATH_NOT_FOUND"),
        "validate --format json --baseline must still diagnose E_PATH_NOT_FOUND on the machine channel. \
         stdout:\n{validate_baseline_out}\nstderr:\n{validate_baseline_err}"
    );

    // Load-fail on --config stays config-class (E_CFG_PARSE / E_MISSING_CONFIG).
    // Do not reclassify it to E_PATH_NOT_FOUND.
    let doctor_config_out = String::from_utf8_lossy(&doctor_config.stdout);
    let validate_config_out = String::from_utf8_lossy(&validate_config.stdout);
    assert!(
        !doctor_config_out.contains("E_PATH_NOT_FOUND"),
        "doctor --config load-fail must not be reclassified to E_PATH_NOT_FOUND. \
         stdout:\n{doctor_config_out}"
    );
    assert!(
        !validate_config_out.contains("E_PATH_NOT_FOUND"),
        "validate --config load-fail must not be reclassified to E_PATH_NOT_FOUND. \
         stdout:\n{validate_config_out}"
    );
}
