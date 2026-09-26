//! S1 caller posture (#2573): global `--quiet` suppresses progress and
//! banners only, and `--color` follows flag > `NO_COLOR` > TTY.
//!
//! - `quiet_suppresses_progress_keeps_diagnostics`: a failing two-test suite
//!   prints `Running 2 tests...` without the flag and neither that banner nor
//!   `Running test i/N` progress lines with it, while the failure summary,
//!   the reason code, and `run.json` are unchanged.
//! - `color_flag_beats_convention_on_piped_stderr`: stderr is a pipe here, so
//!   `auto` renders plain; `--color always` renders decorated and
//!   `--color never` renders plain even with `NO_COLOR` set.
//! - `tool_verify_local_quiet_grandfathered`: the per-command
//!   `mcp tool verify --quiet` keeps suppressing its error text. clap merges
//!   the same-spelling global and local flags into one occurrence, so the
//!   global position triggers the same suppression (exit code unchanged) —
//!   the single documented exception to the never-suppress rule.

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command as StdCommand, Output, Stdio};
use tempfile::tempdir;

fn write_tree(dir: &Path) {
    fs::write(
        dir.join("eval.yaml"),
        "version: 1\nsuite: quiet-color\nmodel: trace\ntests:\n  - id: t1\n    input: { prompt: \"hello\" }\n    expected: { type: must_contain, must_contain: [\"passed\"] }\n  - id: t2\n    input: { prompt: \"goodbye\" }\n    expected: { type: must_contain, must_contain: [\"passed\"] }\n",
    )
    .expect("write eval.yaml");
    fs::write(
        dir.join("trace.jsonl"),
        "{\"type\":\"episode_start\",\"episode_id\":\"t1\",\"timestamp\":1000,\"input\":{\"prompt\":\"hello\"}}\n{\"type\":\"episode_end\",\"episode_id\":\"t1\",\"timestamp\":2000,\"final_output\":\"passed\"}\n{\"type\":\"episode_start\",\"episode_id\":\"t2\",\"timestamp\":3000,\"input\":{\"prompt\":\"goodbye\"}}\n{\"type\":\"episode_end\",\"episode_id\":\"t2\",\"timestamp\":4000,\"final_output\":\"nope\"}\n",
    )
    .expect("write trace.jsonl");
}

fn run_assay(dir: &Path, args: &[&str], envs: &[(&str, &str)], clear_no_color: bool) -> Output {
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_assay"));
    cmd.current_dir(dir)
        .env("ASSAY_EXIT_CODES", "v2")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    if clear_no_color {
        cmd.env_remove("NO_COLOR");
    }
    cmd.output().expect("spawn assay")
}

/// `run.json` carries timing fields that legitimately differ between two
/// invocations. Strip them so the comparison pins the diagnostic content —
/// exit code, reason code, and per-row status — rather than the clock.
fn normalized_run_json(dir: &Path) -> Value {
    let raw = fs::read_to_string(dir.join("run.json")).expect("run.json written");
    let mut doc: Value = serde_json::from_str(&raw).expect("run.json parses");
    if let Some(results) = doc.get_mut("results").and_then(Value::as_array_mut) {
        for row in results.iter_mut() {
            if let Some(obj) = row.as_object_mut() {
                obj.remove("duration_ms");
                if let Some(attempts) = obj.get_mut("attempts").and_then(Value::as_array_mut) {
                    for attempt in attempts.iter_mut() {
                        if let Some(a) = attempt.as_object_mut() {
                            a.remove("duration_ms");
                        }
                    }
                }
            }
        }
    }
    if let Some(obj) = doc.as_object_mut() {
        obj.remove("performance");
        obj.remove("timings");
        obj.remove("order_seed");
    }
    doc
}

#[test]
fn quiet_suppresses_progress_keeps_diagnostics() {
    let noisy_dir = tempdir().expect("tempdir");
    write_tree(noisy_dir.path());
    let noisy = run_assay(
        noisy_dir.path(),
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
        ],
        &[],
        true,
    );
    assert_eq!(noisy.status.code(), Some(1), "fixture suite must fail");
    let noisy_stderr = String::from_utf8_lossy(&noisy.stderr);
    assert!(
        noisy_stderr.contains("Running 2 tests..."),
        "noisy run must print the progress banner; stderr:\n{noisy_stderr}"
    );

    let quiet_dir = tempdir().expect("tempdir");
    write_tree(quiet_dir.path());
    let quiet = run_assay(
        quiet_dir.path(),
        &[
            "--quiet",
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
        ],
        &[],
        true,
    );
    assert_eq!(
        quiet.status.code(),
        Some(1),
        "--quiet must not change the exit"
    );
    let quiet_stderr = String::from_utf8_lossy(&quiet.stderr);
    assert!(
        !quiet_stderr.contains("Running 2 tests..."),
        "--quiet must suppress the progress banner; stderr:\n{quiet_stderr}"
    );
    assert!(
        !quiet_stderr.contains("Running test "),
        "--quiet must suppress progress-sink lines; stderr:\n{quiet_stderr}"
    );
    assert!(
        quiet_stderr.contains("Summary:"),
        "--quiet must keep the failure summary; stderr:\n{quiet_stderr}"
    );
    assert!(
        quiet_stderr.contains("t2"),
        "--quiet must keep the failing test id; stderr:\n{quiet_stderr}"
    );

    let noisy_json = normalized_run_json(noisy_dir.path());
    let quiet_json = normalized_run_json(quiet_dir.path());
    assert_eq!(
        noisy_json.get("reason_code"),
        Some(&Value::from("E_TEST_FAILED")),
        "fixture must classify as E_TEST_FAILED"
    );
    assert_eq!(
        noisy_json, quiet_json,
        "--quiet must leave run.json identical"
    );

    // The flag also parses after the subcommand (clap global=true).
    let trailing_dir = tempdir().expect("tempdir");
    write_tree(trailing_dir.path());
    let trailing = run_assay(
        trailing_dir.path(),
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
            "--quiet",
        ],
        &[],
        true,
    );
    let trailing_stderr = String::from_utf8_lossy(&trailing.stderr);
    assert!(
        !trailing_stderr.contains("Running 2 tests..."),
        "trailing --quiet must suppress the banner too; stderr:\n{trailing_stderr}"
    );
}

#[test]
fn quiet_env_binding_suppresses_the_banner() {
    let dir = tempdir().expect("tempdir");
    write_tree(dir.path());
    let out = run_assay(
        dir.path(),
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "trace.jsonl",
        ],
        &[("ASSAY_QUIET", "1")],
        true,
    );
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("Running 2 tests..."),
        "ASSAY_QUIET=1 must suppress the progress banner; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("Summary:"),
        "ASSAY_QUIET=1 must keep the failure summary; stderr:\n{stderr}"
    );
}

#[test]
fn color_flag_beats_convention_on_piped_stderr() {
    let dir = tempdir().expect("tempdir");

    // Piped stderr is not a TTY, so `auto` renders plain.
    let auto = run_assay(dir.path(), &["run", "--config", "missing.yaml"], &[], true);
    assert_eq!(auto.status.code(), Some(2));
    let auto_stderr = String::from_utf8_lossy(&auto.stderr);
    assert!(
        auto_stderr.contains("[E_MISSING_CONFIG]"),
        "auto on a pipe must render the plain code; stderr:\n{auto_stderr}"
    );
    assert!(
        !auto_stderr.contains('\u{274c}'),
        "auto on a pipe must not decorate; stderr:\n{auto_stderr}"
    );

    // Present-but-empty NO_COLOR keeps today's meaning: plain under auto.
    let empty = run_assay(
        dir.path(),
        &["run", "--config", "missing.yaml"],
        &[("NO_COLOR", "")],
        false,
    );
    let empty_stderr = String::from_utf8_lossy(&empty.stderr);
    assert!(
        empty_stderr.contains("[E_MISSING_CONFIG]") && !empty_stderr.contains('\u{274c}'),
        "NO_COLOR=\"\" must stay plain (unchanged); stderr:\n{empty_stderr}"
    );

    // The flag beats the convention on both sides.
    let always = run_assay(
        dir.path(),
        &["--color", "always", "run", "--config", "missing.yaml"],
        &[("NO_COLOR", "1")],
        false,
    );
    let always_stderr = String::from_utf8_lossy(&always.stderr);
    assert!(
        always_stderr.contains('\u{274c}'),
        "--color always must decorate despite NO_COLOR=1; stderr:\n{always_stderr}"
    );

    let never = run_assay(
        dir.path(),
        &["--color", "never", "run", "--config", "missing.yaml"],
        &[("NO_COLOR", "1")],
        false,
    );
    let never_stderr = String::from_utf8_lossy(&never.stderr);
    assert!(
        never_stderr.contains("[E_MISSING_CONFIG]") && !never_stderr.contains('\u{274c}'),
        "--color never must stay plain; stderr:\n{never_stderr}"
    );
}

#[test]
fn tool_verify_local_quiet_grandfathered() {
    let dir = tempdir().expect("tempdir");
    let missing = dir.path().join("absent-tool.json");

    let noisy = run_assay(
        dir.path(),
        &[
            "mcp",
            "tool",
            "verify",
            missing.to_str().expect("utf8"),
            "--allow-embedded-key",
        ],
        &[],
        true,
    );
    assert_ne!(noisy.status.code(), Some(0));
    let noisy_stderr = String::from_utf8_lossy(&noisy.stderr);
    assert!(
        !noisy_stderr.is_empty(),
        "verify without --quiet must print its error text"
    );

    // Grandfathered: the per-command flag still suppresses the error text.
    let local = run_assay(
        dir.path(),
        &[
            "mcp",
            "tool",
            "verify",
            missing.to_str().expect("utf8"),
            "--allow-embedded-key",
            "--quiet",
        ],
        &[],
        true,
    );
    assert_eq!(local.status.code(), noisy.status.code());
    assert!(
        String::from_utf8_lossy(&local.stderr).is_empty(),
        "verify --quiet must keep suppressing the error text"
    );

    // clap merges the same-spelling global and local `--quiet` into one
    // occurrence (a second id sharing the long is a build error), so the
    // global position also triggers verify's grandfathered suppression. The
    // exit code is unchanged; this is the single documented exception to
    // "global --quiet never suppresses diagnostics".
    let global = run_assay(
        dir.path(),
        &[
            "--quiet",
            "mcp",
            "tool",
            "verify",
            missing.to_str().expect("utf8"),
            "--allow-embedded-key",
        ],
        &[],
        true,
    );
    assert_eq!(global.status.code(), noisy.status.code());
    assert!(
        String::from_utf8_lossy(&global.stderr).is_empty(),
        "merged spelling: global-position --quiet also suppresses verify text"
    );

    // Both positions at once: same suppression, same exit code.
    let both = run_assay(
        dir.path(),
        &[
            "--quiet",
            "mcp",
            "tool",
            "verify",
            missing.to_str().expect("utf8"),
            "--allow-embedded-key",
            "--quiet",
        ],
        &[],
        true,
    );
    assert_eq!(both.status.code(), noisy.status.code());
    assert!(
        String::from_utf8_lossy(&both.stderr).is_empty(),
        "both positions: verify text suppressed, exit unchanged"
    );
}
