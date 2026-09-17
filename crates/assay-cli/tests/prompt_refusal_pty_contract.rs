//! Pty-harness row of the prompt-refusal contract (#2573 slice 2 & 3).
//!
//! Slice 1 covered null stdin. Slice 2 added the pty harness for TTY stdin +
//! redirected stderr. Slice 3 makes the refusal attributable to product code:
//! `interaction.rs` checks `stderr().is_terminal()` explicitly before calling
//! dialoguer.
//!
//! Decision table row (`interaction.rs`):
//! 1. not preapproved
//! 2. `stdin().is_terminal()` is true (pty slave) → early stdin refuse does not fire
//! 3. `stderr().is_terminal()` is false (piped stderr) → refuses (`stderr is not a terminal`, exit 2, names `--yes`)
//! 4. `stdin` and `stderr` are both terminals (pty slave) → displays prompt, accepts user response, no refusal
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
use std::io::{Read, Write};
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
    drop(cmd);
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

struct InteractivePtyResult {
    status: std::process::ExitStatus,
    stdout: String,
    master_output: String,
    prompt_seen: bool,
}

/// Stdin = pty slave, stderr = pty slave, stdout = pipe.
///
/// Bounded interactive harness: reads the pty master until `expected_prompt_needle`
/// appears, writes `response` (e.g. `b"n\n"`), and drains output until the
/// child process terminates. Times out at 15s with SIGKILL and a clear failure message.
fn run_pty_interactive_confirm(
    dir: &Path,
    args: &[&str],
    expected_prompt_needle: &str,
    response: &[u8],
) -> InteractivePtyResult {
    let pty = nix::pty::openpty(None, None).expect("openpty");
    let slave_stderr = pty.slave.try_clone().expect("clone pty slave for stderr");

    let mut cmd = StdCommand::new(env!("CARGO_BIN_EXE_assay"));
    cmd.current_dir(dir)
        .env("NO_COLOR", "1")
        .stdin(Stdio::from(pty.slave))
        .stdout(Stdio::piped())
        .stderr(Stdio::from(slave_stderr))
        .args(args);

    let mut child = cmd.spawn().expect("spawn assay with pty stdin + stderr");
    drop(cmd);
    let child_pid = child.id();
    let stdout_pipe = child.stdout.take().expect("stdout handle");

    let expected_prompt = expected_prompt_needle.to_string();
    let response_bytes = response.to_vec();

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let stdout_thread = thread::spawn(move || {
            let mut out = Vec::new();
            let mut pipe = stdout_pipe;
            let _ = pipe.read_to_end(&mut out);
            String::from_utf8_lossy(&out).into_owned()
        });

        let mut master = std::fs::File::from(pty.master);
        let mut master_bytes = Vec::new();
        let mut prompt_seen = false;
        let mut answered = false;
        let mut buf = [0u8; 512];

        loop {
            match master.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    master_bytes.extend_from_slice(&buf[..n]);
                    let current_text = String::from_utf8_lossy(&master_bytes);
                    if !answered && current_text.contains(&expected_prompt) {
                        prompt_seen = true;
                        answered = true;
                        let _ = master.write_all(&response_bytes);
                        let _ = master.flush();
                    }
                }
                Err(_) => {
                    // On Unix/Linux/macOS, closing the slave when the child exits
                    // causes master read to return EIO or EOF.
                    break;
                }
            }
        }

        let stdout = stdout_thread.join().unwrap_or_default();
        let status = child.wait().expect("wait for child");
        let master_output = String::from_utf8_lossy(&master_bytes).into_owned();

        let _ = tx.send(InteractivePtyResult {
            status,
            stdout,
            master_output,
            prompt_seen,
        });
    });

    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(result) => result,
        Err(_) => {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(child_pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            panic!(
                "pty contract timed out after 15s: assay {args:?} did not finish interactive session"
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
        stderr.contains("stderr is not a terminal"),
        "TTY stdin + piped stderr refusal must name stderr reason; stderr:\n{stderr}"
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
        stderr.contains("stderr is not a terminal"),
        "TTY stdin + piped stderr refusal must name stderr reason; stderr:\n{stderr}"
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

#[test]
fn t4_doctor_fix_interactive_session_shows_prompt_and_handles_decline() {
    let temp = tempdir().expect("tempdir");
    let config = temp.path().join("eval.yaml");
    let trace = temp.path().join("traces/main.jsonl");
    write_minimal_config(&config);
    assert!(!trace.exists());

    let result = run_pty_interactive_confirm(
        temp.path(),
        &[
            "doctor",
            "--config",
            config.to_str().expect("utf8 config"),
            "--trace-file",
            trace.to_str().expect("utf8 trace"),
            "--fix",
        ],
        "Apply fix",
        b"n\n",
    );

    assert!(
        result.prompt_seen,
        "interactive session with TTY stdin + stderr must display the prompt; \
         master output:\n{}\nstdout:\n{}",
        result.master_output, result.stdout
    );
    assert!(
        result.master_output.contains("Create missing trace file"),
        "master output must contain the candidate fix title; master output:\n{}",
        result.master_output
    );
    assert!(
        !result.master_output.contains("cannot show prompt"),
        "refusal line must not be emitted when stdin and stderr are terminals; master output:\n{}",
        result.master_output
    );
    assert!(
        !result.stdout.contains("cannot show prompt"),
        "refusal line must not be emitted on stdout; stdout:\n{}",
        result.stdout
    );
    let code = result.status.code().expect("exit code");
    assert_eq!(
        code, 2,
        "declined doctor --fix on unresolved config diagnostics must return exit code 2 (EXIT_CONFIG_ERROR); \
         master output:\n{}\nstdout:\n{}",
        result.master_output, result.stdout
    );
    assert!(
        !trace.exists(),
        "a declined fix confirmation must not create traces/main.jsonl"
    );
}
