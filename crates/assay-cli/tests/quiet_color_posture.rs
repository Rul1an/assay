//! S1 caller posture (#2573): top-level `--quiet` (before the subcommand)
//! suppresses progress and banners of run/ci/watch/replay only, and
//! `--color` follows flag > `ASSAY_COLOR` > `NO_COLOR` > TTY.
//!
//! - `quiet_suppresses_progress_keeps_diagnostics`: a failing two-test suite
//!   prints `Running 2 tests...` without the flag and neither that banner nor
//!   `Running test i/N` progress lines with it, while the failure summary,
//!   the reason code, and `run.json` are unchanged. A trailing `--quiet`
//!   after the subcommand is a usage error, never a propagation.
//! - `color_flag_beats_convention_on_piped_stderr`: stderr is a pipe here, so
//!   `auto` renders plain; `--color always` renders decorated and
//!   `--color never` renders plain even with `NO_COLOR` set.
//! - `tool_verify_quiet_positions_are_independent`: the per-command
//!   `mcp tool verify --quiet` keeps suppressing its error text, while the
//!   top-level `--quiet` and `ASSAY_QUIET=1` leave it alone (exit code
//!   unchanged throughout). There is no exception to the never-suppress rule.
//! - `top_level_quiet_does_not_reach_the_sandbox_banner`: neither the flag
//!   nor the env silences the sandbox banner; only sandbox's own `--quiet`
//!   does.
//! - `empty_quiet_and_color_env_count_as_unset` /
//!   `force_color_and_clicolor_force_are_ignored`: env edge rules.

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
    // Hermetic posture: the runner's own ASSAY_QUIET/ASSAY_COLOR must not
    // leak into the child; each test opts in through `envs`.
    cmd.env_remove("ASSAY_QUIET");
    cmd.env_remove("ASSAY_COLOR");
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
        obj.remove("runner_clone_ms");
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

    // The top-level flag is not clap-global: after the subcommand it is a
    // usage error, so it can never drift into another command's local
    // `--quiet`. The error lands before anything runs (no run.json).
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
    assert_eq!(
        trailing.status.code(),
        Some(2),
        "trailing --quiet must be a usage error"
    );
    let trailing_stderr = String::from_utf8_lossy(&trailing.stderr);
    assert!(
        trailing_stderr.contains("--quiet"),
        "the usage error must name the misplaced flag; stderr:\n{trailing_stderr}"
    );
    assert!(
        !trailing_dir.path().join("run.json").exists(),
        "the usage error must land before anything runs"
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
fn tool_verify_quiet_positions_are_independent() {
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

    // The per-command flag keeps its local meaning: it still suppresses the
    // error text, exit code unchanged.
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

    // F2: the top-level flag no longer propagates, so the error text stays.
    let top = run_assay(
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
    assert_eq!(top.status.code(), noisy.status.code());
    assert!(
        !String::from_utf8_lossy(&top.stderr).is_empty(),
        "top-level --quiet must not suppress verify's error text"
    );

    // F2 via env: ASSAY_QUIET=1 must not suppress verify's error text
    // either. There is no exception to the never-suppress rule.
    let env = run_assay(
        dir.path(),
        &[
            "mcp",
            "tool",
            "verify",
            missing.to_str().expect("utf8"),
            "--allow-embedded-key",
        ],
        &[("ASSAY_QUIET", "1")],
        true,
    );
    assert_eq!(env.status.code(), noisy.status.code());
    assert!(
        !String::from_utf8_lossy(&env.stderr).is_empty(),
        "ASSAY_QUIET=1 must not suppress verify's error text"
    );

    // Both positions at once: the local flag still suppresses, exit
    // unchanged.
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

#[test]
fn top_level_quiet_does_not_reach_the_sandbox_banner() {
    for (label, args, envs) in [
        ("control", vec!["sandbox", "--", "true"], vec![]),
        ("top-flag", vec!["--quiet", "sandbox", "--", "true"], vec![]),
    ] {
        let dir = tempdir().expect("tempdir");
        let out = run_assay(dir.path(), &args, &envs, true);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("Backend:") && stderr.contains("Rules:"),
            "{label}: the sandbox banner must stay; stderr:\n{stderr}"
        );
    }

    // F1 via env: ASSAY_QUIET=1 must not silence the sandbox banner either.
    let dir = tempdir().expect("tempdir");
    let env_quiet = run_assay(
        dir.path(),
        &["sandbox", "--", "true"],
        &[("ASSAY_QUIET", "1")],
        true,
    );
    let env_stderr = String::from_utf8_lossy(&env_quiet.stderr);
    assert!(
        env_stderr.contains("Backend:") && env_stderr.contains("Rules:"),
        "ASSAY_QUIET=1 must not silence the sandbox banner; stderr:\n{env_stderr}"
    );

    // Control: the sandbox-local --quiet keeps its own banner meaning.
    let dir = tempdir().expect("tempdir");
    let local = run_assay(dir.path(), &["sandbox", "--quiet", "--", "true"], &[], true);
    let local_stderr = String::from_utf8_lossy(&local.stderr);
    assert!(
        local_stderr.contains("Assay Sandbox v0.1"),
        "sandbox-local --quiet keeps the version line; stderr:\n{local_stderr}"
    );
    assert!(
        !local_stderr.contains("Backend:"),
        "sandbox-local --quiet still suppresses the banner body; stderr:\n{local_stderr}"
    );
}

#[test]
fn empty_quiet_and_color_env_count_as_unset() {
    // F4: an empty ASSAY_QUIET behaves as unset — the progress banner stays.
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
        &[("ASSAY_QUIET", "")],
        true,
    );
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Running 2 tests..."),
        "empty ASSAY_QUIET must count as unset (banner stays); stderr:\n{stderr}"
    );

    // F4: an empty ASSAY_COLOR behaves as unset — `auto` on a pipe renders
    // plain instead of erroring before anything runs.
    let dir = tempdir().expect("tempdir");
    let out = run_assay(
        dir.path(),
        &["run", "--config", "missing.yaml"],
        &[("ASSAY_COLOR", "")],
        true,
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("[E_MISSING_CONFIG]") && !stderr.contains('\u{274c}'),
        "empty ASSAY_COLOR must count as unset (plain auto); stderr:\n{stderr}"
    );

    // An invalid ASSAY_QUIET is a usage error (exit 2), like clap's old
    // `env =` binding reported it — never a silent off.
    let dir = tempdir().expect("tempdir");
    let out = run_assay(
        dir.path(),
        &["run", "--config", "missing.yaml"],
        &[("ASSAY_QUIET", "maybe")],
        true,
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ASSAY_QUIET"),
        "the usage error must name the variable; stderr:\n{stderr}"
    );
}

#[test]
fn force_color_and_clicolor_force_are_ignored() {
    // F3: agent CI images export FORCE_COLOR; ANSI must not reach parsed
    // output, so neither it nor CLICOLOR_FORCE may force decoration.
    let dir = tempdir().expect("tempdir");
    let out = run_assay(
        dir.path(),
        &["run", "--config", "missing.yaml"],
        &[("FORCE_COLOR", "1"), ("CLICOLOR_FORCE", "1")],
        true,
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("[E_MISSING_CONFIG]") && !stderr.contains('\u{1b}'),
        "FORCE_COLOR/CLICOLOR_FORCE must not force ANSI on a pipe; stderr:\n{stderr}"
    );
}
