//! Pty-harness row of the prompt-refusal contract (#2573 slice 2).
//!
//! Slice 1 covered null stdin. This file pins TTY stdin + redirected stderr,
//! which #3058 Q3 only code-read. Unix-only: `nix::pty::openpty`. Same POSIX
//! path on macOS and Linux; no `ptsname` (macOS has no safe `ptsname_r` in
//! nix 0.27).
//!
//! Decision table row (`interaction.rs`):
//! 1. not preapproved
//! 2. `stdin().is_terminal()` is true (pty slave) → early refuse does not fire
//! 3. `dialoguer::Confirm::interact` uses `Term::stderr()`; a pipe is not a
//!    terminal → `NotConnected` → `PromptRefused` (exit 2, names `--yes`)
//!
//! Item 2 (`assay fix` confirm): no measured `validate()` input produces a
//! `SuggestedPatch`. `fix.rs` runs `validate` then `build_suggestions`. Patch
//! arms need codes `validate` never emits (`E_CFG_SCHEMA_UNKNOWN_FIELD` with
//! file/pointer context, `E_TOOL_NOT_ALLOWED`, `E_PATH_SCOPE_VIOLATION`) or
//! `E_PATH_NOT_FOUND` with `file` ending `assay.yaml` plus `field`, which
//! `validate` does not attach (it sets `path` only). The confirm is
//! unreachable today; do not invent a patch arm here.

#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

fn write_minimal_config(path: &Path) {
    fs::write(
        path,
        "version: 1\nsuite: doctor-fix\nmodel: trace\ntests:\n  - id: t1\n    input:\n      prompt: \"hello\"\n    expected:\n      type: must_contain\n      must_contain: [\"hello\"]\n",
    )
    .expect("write eval config");
}

fn write_parse_error_config(path: &Path) {
    fs::write(
        path,
        "version: 1\nsuite: doctor-fix\nmodel: trace\nsettngs: {}\ntests:\n  - id: t1\n    input:\n      prompt: \"hello\"\n    expected:\n      type: must_contain\n      must_contain: [\"hello\"]\n",
    )
    .expect("write misspelled eval config");
}

/// Stdin = pty slave, stderr = pipe, stdout = pipe. Writes a decline and
/// closes the master so a real prompt cannot hang. Times out at 15s.
fn run_pty_stdin_piped_stderr(dir: &Path, args: &[&str]) -> std::process::Output {
    let pty = nix::pty::openpty(None, None).expect("openpty");
    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_assay"));
    cmd.current_dir(dir)
        .env("NO_COLOR", "1")
        .stdin(Stdio::from(pty.slave))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(args);

    let child = cmd.spawn().expect("spawn assay with pty stdin");
    let child_pid = child.id();

    let mut master = std::fs::File::from(pty.master);
    let _ = master.write_all(b"n\n");
    drop(master);

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(result) => result.expect("wait_with_output"),
        Err(_) => {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(child_pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            panic!(
                "pty contract timed out after 15s: assay {args:?} did not exit \
                 with TTY stdin and piped stderr (wrote a decline and closed the master)"
            );
        }
    }
}

/// RED first asserted `code != 2` so the harness was shown to run. The code's
/// decision for this combination is refuse (exit 2, names `--yes`).
#[test]
fn t4_doctor_fix_tty_stdin_piped_stderr_refuses() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    let trace = temp.path().join("traces/main.jsonl");
    write_minimal_config(&config);
    assert!(!trace.exists());

    let output = run_pty_stdin_piped_stderr(
        temp.path(),
        &[
            "doctor",
            "--config",
            config.to_str().expect("utf8 config"),
            "--trace-file",
            trace.to_str().expect("utf8 trace"),
            "--fix",
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let code = output.status.code().expect("exit code");
    assert_eq!(
        code, 2,
        "TTY stdin + piped stderr must refuse (dialoguer uses stderr); \
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("--yes"),
        "TTY stdin + piped stderr refusal must name --yes; stderr:\n{stderr}"
    );
    assert!(
        !trace.exists(),
        "a refused prompt must not create traces/main.jsonl"
    );
}

#[test]
fn t4_doctor_parse_error_tty_stdin_piped_stderr_refuses() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    write_parse_error_config(&config);
    let before = fs::read(&config).expect("read config before");

    let output = run_pty_stdin_piped_stderr(
        temp.path(),
        &[
            "doctor",
            "--config",
            config.to_str().expect("utf8 config"),
            "--fix",
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code().expect("exit code");
    assert_eq!(
        code, 2,
        "TTY stdin + piped stderr must refuse the parse-error prompt; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("--yes"),
        "TTY stdin + piped stderr refusal must name --yes; stderr:\n{stderr}"
    );
    let after = fs::read(&config).expect("read config after");
    assert_eq!(
        before, after,
        "a refused parse-error repair must not change the config"
    );
}
