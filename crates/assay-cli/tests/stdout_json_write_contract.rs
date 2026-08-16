//! Closed or partially-closed stdout is infrastructure failure, not success or abort (#2263).
//!
//! `println!` panics on a stdout write failure. With `panic = "abort"` that becomes SIGABRT
//! (exit 134), which the exit contract does not define. A partial or absent machine document
//! is not a clean success: the write/flush error maps to `EXIT_INFRA_ERROR` (3).

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, ProcessLimits};
use serde_json::Value;
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessGroup;
use process_wrap::std::{ChildWrapper, CommandWrap};

const LIMITS: ProcessLimits =
    ProcessLimits::new(Duration::from_secs(15), 2 * 1024 * 1024, 64 * 1024);
const LARGE_RUN_LIMITS: ProcessLimits =
    ProcessLimits::new(Duration::from_secs(45), 4 * 1024 * 1024, 64 * 1024);
const PIPE_CAPACITY: usize = 64 * 1024;
/// POSIX minimum `PIPE_BUF`. Linux is typically 4096; either is far below the fixture.
const PIPE_BUF: usize = 512;
const PARTIAL_READ: usize = 200;

const MATCHING_TRACE: &str = r#"{"schema_version": 1, "type": "assay.trace", "request_id": "hello_1", "prompt": "hello_prompt", "response": "Hello Assay", "model": "trace", "provider": "trace"}
"#;

const EVAL_ONE: &str = r#"configVersion: 1
suite: "stdout-write"
model: "trace"
tests:
  - id: "stdout_write_regex"
    input:
      prompt: "hello_prompt"
    expected:
      type: regex_match
      pattern: "Hello\\s+Assay"
      flags: ["i"]
"#;

fn assay_command(cwd: &Path) -> Command {
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
    command.current_dir(cwd).env("NO_COLOR", "1");
    command
}

fn assay<T: AsRef<OsStr>>(cwd: &Path, args: &[T], limits: ProcessLimits, context: &str) -> Output {
    let mut command = assay_command(cwd);
    command.args(args);
    run_bounded(command, b"", limits, context).unwrap_or_else(|error| panic!("{error}"))
}

fn exit_code(output: &Output, context: &str) -> i32 {
    output.status.code().unwrap_or_else(|| {
        panic!("{context}: process died by signal instead of a defined exit; {output:?}")
    })
}

fn sole_json(output: &Output, context: &str) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{context}: stdout is not one JSON document: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_no_rust_panic(stderr: &[u8], context: &str) {
    let text = String::from_utf8_lossy(stderr);
    assert!(
        !text.contains("panicked at") && !text.contains("failed printing to stdout"),
        "{context}: stderr still shows a Rust stdout panic:\n{text}"
    );
}

fn tree_with_eval(dir: &Path, eval: &str) {
    std::fs::write(dir.join("eval.yaml"), eval).expect("wrote eval.yaml");
    std::fs::create_dir_all(dir.join("traces")).expect("created traces/");
    std::fs::write(dir.join("traces/hello.jsonl"), MATCHING_TRACE).expect("wrote trace");
}

fn large_eval_yaml() -> String {
    let pad = "x".repeat(2048);
    let mut body =
        String::from("configVersion: 1\nsuite: \"stdout-write-large\"\nmodel: \"trace\"\ntests:\n");
    for i in 0..48 {
        body.push_str(&format!(
            "  - id: \"t{i:03}-{pad}\"\n    input:\n      prompt: \"hello_prompt\"\n    expected:\n      type: regex_match\n      pattern: \"Hello\\\\s+Assay\"\n      flags: [\"i\"]\n"
        ));
    }
    body
}

#[derive(Debug)]
struct ClosedStdout {
    code: Option<i32>,
    stdout_prefix: Vec<u8>,
    stderr: Vec<u8>,
}

fn wrap_command(command: Command) -> CommandWrap {
    let mut command = CommandWrap::from(command);
    #[cfg(unix)]
    command.wrap(ProcessGroup::leader());
    #[cfg(windows)]
    command.wrap(JobObject);
    command
}

/// Same shape as `bounded_process::spawn_reader`: cap the stream, then `read_to_end`.
fn spawn_bounded_reader<R: Read + Send + 'static>(
    reader: R,
    limit: usize,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::with_capacity(limit.saturating_add(1));
        reader
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn join_bounded_reader(
    reader: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    context: &str,
) -> Vec<u8> {
    let Some(handle) = reader else {
        return Vec::new();
    };
    handle
        .join()
        .unwrap_or_else(|_| panic!("{context}: stderr reader panicked"))
        .unwrap_or_else(|error| panic!("{context}: read stderr: {error}"))
}

fn wait_or_kill(
    child: &mut dyn ChildWrapper,
    timeout: Duration,
    context: &str,
) -> Result<Option<i32>, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code()),
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.start_kill();
                let reap_deadline = Instant::now() + Duration::from_secs(1);
                while Instant::now() < reap_deadline {
                    match child.try_wait() {
                        Ok(Some(_)) => {
                            return Err(format!(
                                "{context}: child did not exit within {timeout:?}"
                            ));
                        }
                        Ok(None) => thread::sleep(Duration::from_millis(10)),
                        Err(error) => {
                            return Err(format!("{context}: reap after kill failed: {error}"))
                        }
                    }
                }
                return Err(format!(
                    "{context}: child did not exit within {timeout:?}; reap grace expired"
                ));
            }
            Err(error) => return Err(format!("{context}: wait failed: {error}")),
        }
    }
}

/// Wait/kill owns the deadline. The stderr reader is joined only after that.
fn collect_closed_stdout(
    child: &mut dyn ChildWrapper,
    stderr_reader: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    timeout: Duration,
    stdout_prefix: Vec<u8>,
    context: &str,
) -> Result<ClosedStdout, String> {
    let wait = wait_or_kill(child, timeout, context);
    let stderr = join_bounded_reader(stderr_reader, context);
    match wait {
        Ok(code) => Ok(ClosedStdout {
            code,
            stdout_prefix,
            stderr,
        }),
        Err(error) => Err(format!(
            "{error}; stderr={}",
            String::from_utf8_lossy(&stderr)
        )),
    }
}

fn run_reader_already_gone_with(
    mut command: Command,
    timeout: Duration,
    context: &str,
) -> Result<ClosedStdout, String> {
    let (reader, writer) = std::io::pipe().expect("pipe");
    drop(reader);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(writer))
        .stderr(Stdio::piped());
    let mut child = wrap_command(command)
        .spawn()
        .map_err(|error| format!("{context}: spawn: {error}"))?;
    let stderr_reader = child
        .stderr()
        .take()
        .map(|pipe| spawn_bounded_reader(pipe, LIMITS.max_stderr_bytes));
    collect_closed_stdout(child.as_mut(), stderr_reader, timeout, Vec::new(), context)
}

fn run_reader_already_gone(command: Command, context: &str) -> ClosedStdout {
    run_reader_already_gone_with(command, LIMITS.timeout, context)
        .unwrap_or_else(|error| panic!("{error}"))
}

fn run_partial_then_close(mut command: Command, context: &str) -> ClosedStdout {
    let (mut reader, writer) = std::io::pipe().expect("pipe");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(writer))
        .stderr(Stdio::piped());
    let mut child = wrap_command(command)
        .spawn()
        .unwrap_or_else(|error| panic!("{context}: spawn: {error}"));
    let stderr_reader = child
        .stderr()
        .take()
        .map(|pipe| spawn_bounded_reader(pipe, LARGE_RUN_LIMITS.max_stderr_bytes));
    let mut stdout_prefix = vec![0_u8; PARTIAL_READ];
    let read = reader
        .read(&mut stdout_prefix)
        .unwrap_or_else(|error| panic!("{context}: partial read: {error}"));
    stdout_prefix.truncate(read);
    drop(reader);
    collect_closed_stdout(
        child.as_mut(),
        stderr_reader,
        LARGE_RUN_LIMITS.timeout,
        stdout_prefix,
        context,
    )
    .unwrap_or_else(|error| panic!("{error}"))
}

fn assert_infra_write_failure(result: &ClosedStdout, context: &str) {
    assert_eq!(
        result.code,
        Some(3),
        "{context}: write failure must be exit 3, not 0 and not a signal; stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_no_rust_panic(&result.stderr, context);
}

#[test]
fn golden_path_json_seams_do_not_println() {
    let doctor = include_str!("../src/cli/commands/doctor/implementation.rs");
    let run = include_str!("../src/cli/commands/run.rs");
    let doctor_hits = doctor
        .matches("println!(\"{}\", serde_json::to_string_pretty")
        .count();
    let run_hits = run
        .matches("println!(\"{}\", assay_core::report::json::render_json")
        .count();
    assert_eq!(
        doctor_hits, 0,
        "doctor JSON seams must call the shared writer, not println!"
    );
    assert_eq!(
        run_hits, 0,
        "run JSON seam must call the shared writer, not println!"
    );
}

#[test]
fn doctor_json_success_is_one_document_and_exit_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = assay(
        dir.path(),
        &["doctor", "--format", "json"],
        LIMITS,
        "doctor json success",
    );
    assert_eq!(exit_code(&output, "doctor json success"), 0);
    let document = sole_json(&output, "doctor json success");
    assert_eq!(document["schema"], "assay.doctor_report.v0");
}

#[test]
fn doctor_json_reader_gone_before_first_write_is_exit_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut command = assay_command(dir.path());
    command.args(["doctor", "--format", "json"]);
    let result = run_reader_already_gone(command, "doctor json reader gone");
    assert_infra_write_failure(&result, "doctor json reader gone");
}

#[test]
fn doctor_json_invalid_args_reader_gone_is_exit_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut command = assay_command(dir.path());
    command.args(["doctor", "--fix", "--format", "json"]);
    let result = run_reader_already_gone(command, "doctor invalid-args reader gone");
    assert_infra_write_failure(&result, "doctor invalid-args reader gone");
}

#[test]
fn doctor_json_config_load_fail_reader_gone_is_exit_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("eval.yaml"), "version: [\n").expect("wrote broken config");
    let mut command = assay_command(dir.path());
    command.args(["doctor", "--format", "json", "--config", "eval.yaml"]);
    let result = run_reader_already_gone(command, "doctor config-fail reader gone");
    assert_infra_write_failure(&result, "doctor config-fail reader gone");
}

#[test]
fn run_json_success_is_one_document_and_exit_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree_with_eval(dir.path(), EVAL_ONE);
    let output = assay(
        dir.path(),
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "traces/hello.jsonl",
            "--format",
            "json",
        ],
        LIMITS,
        "run json success",
    );
    assert_eq!(exit_code(&output, "run json success"), 0);
    let document = sole_json(&output, "run json success");
    assert_eq!(document["schema"], "assay.run_report.v1");
}

#[test]
fn run_json_partial_read_then_close_is_exit_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    tree_with_eval(dir.path(), &large_eval_yaml());
    let drained = assay(
        dir.path(),
        &[
            "run",
            "--config",
            "eval.yaml",
            "--trace-file",
            "traces/hello.jsonl",
            "--format",
            "json",
        ],
        LARGE_RUN_LIMITS,
        "run json size probe",
    );
    assert_eq!(exit_code(&drained, "run json size probe"), 0);
    assert!(
        drained.stdout.len() > PIPE_CAPACITY,
        "partial-read fixture must exceed the pipe buffer, got {} bytes",
        drained.stdout.len()
    );
    assert!(
        drained.stdout.len() > PIPE_BUF,
        "partial-read fixture must exceed PIPE_BUF ({PIPE_BUF}), got {} bytes",
        drained.stdout.len()
    );

    let mut command = assay_command(dir.path());
    command.args([
        "run",
        "--config",
        "eval.yaml",
        "--trace-file",
        "traces/hello.jsonl",
        "--format",
        "json",
    ]);
    let result = run_partial_then_close(command, "run json partial read");
    assert_infra_write_failure(&result, "run json partial read");
    assert!(
        !result.stdout_prefix.is_empty(),
        "partial-read case must deliver a nonempty truncated prefix"
    );
}

#[cfg(unix)]
fn hanging_stderr_command() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", "while :; do :; done"]);
    command
}

#[cfg(windows)]
fn hanging_stderr_command() -> Command {
    let mut command = Command::new("ping");
    command.args(["-t", "127.0.0.1"]);
    command
}

#[test]
fn closed_stdout_harness_times_out_when_child_keeps_stderr_open() {
    let started = Instant::now();
    let error = run_reader_already_gone_with(
        hanging_stderr_command(),
        Duration::from_millis(250),
        "stderr-open hang probe",
    )
    .expect_err("a child that stays alive with stderr open must hit the wait deadline");
    assert!(
        error.contains("did not exit within"),
        "wait/kill must own the deadline: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "harness hung in stderr drain instead of timing out: {:?}",
        started.elapsed()
    );
}
