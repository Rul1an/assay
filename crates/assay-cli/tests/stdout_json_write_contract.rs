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
use std::sync::mpsc;
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
const PARTIAL_READ: usize = 200;
const REAP_GRACE: Duration = Duration::from_secs(1);

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

const MCP_POLICY: &str = r#"version: "2.0"
name: stdout-write
tools:
  allow:
    - read_file
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

#[cfg(unix)]
fn kill_already_gone(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(libc::ESRCH)
}

#[cfg(windows)]
fn kill_already_gone(_error: &std::io::Error) -> bool {
    false
}

fn start_kill_named(child: &mut dyn ChildWrapper, context: &str) -> Result<(), String> {
    match child.start_kill() {
        Ok(()) => Ok(()),
        Err(error) if kill_already_gone(&error) => Ok(()),
        Err(error) => Err(format!("{context}: start_kill failed: {error}")),
    }
}

fn reap_after_kill(child: &mut dyn ChildWrapper, context: &str) -> Result<(), String> {
    let reap_deadline = Instant::now() + REAP_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if Instant::now() < reap_deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                return Err(format!(
                    "{context}: child did not exit within the wait deadline; reap grace expired"
                ));
            }
            Err(error) => return Err(format!("{context}: reap after kill failed: {error}")),
        }
    }
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
                start_kill_named(child, context)?;
                reap_after_kill(child, context)?;
                return Err(format!("{context}: child did not exit within {timeout:?}"));
            }
            Err(error) => return Err(format!("{context}: wait failed: {error}")),
        }
    }
}

/// Wait/kill owns the deadline. Stderr is joined only after a child-chosen exit.
fn collect_closed_stdout(
    child: &mut dyn ChildWrapper,
    stderr_reader: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>,
    timeout: Duration,
    stdout_prefix: Vec<u8>,
    context: &str,
) -> Result<ClosedStdout, String> {
    let code = wait_or_kill(child, timeout, context)?;
    let stderr = join_bounded_reader(stderr_reader, context);
    Ok(ClosedStdout {
        code,
        stdout_prefix,
        stderr,
    })
}

fn read_prefix_bounded(
    mut reader: impl Read + Send + 'static,
    child: &mut dyn ChildWrapper,
    timeout: Duration,
    context: &str,
) -> Result<Vec<u8>, String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = vec![0_u8; PARTIAL_READ];
        let result = reader.read(&mut buf).map(|n| {
            buf.truncate(n);
            buf
        });
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + timeout;
    loop {
        match rx.try_recv() {
            Ok(Ok(buf)) => return Ok(buf),
            Ok(Err(error)) => return Err(format!("{context}: partial read: {error}")),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(format!("{context}: partial-read worker disconnected"));
            }
            Err(mpsc::TryRecvError::Empty) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(mpsc::TryRecvError::Empty) => {
                start_kill_named(child, context)?;
                reap_after_kill(child, context)?;
                return Err(format!(
                    "{context}: partial read did not complete within {timeout:?}"
                ));
            }
        }
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

fn run_partial_then_close_with(
    mut command: Command,
    timeout: Duration,
    context: &str,
) -> Result<ClosedStdout, String> {
    let (reader, writer) = std::io::pipe().expect("pipe");
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
        .map(|pipe| spawn_bounded_reader(pipe, LARGE_RUN_LIMITS.max_stderr_bytes));
    let stdout_prefix = read_prefix_bounded(reader, child.as_mut(), timeout, context)?;
    collect_closed_stdout(
        child.as_mut(),
        stderr_reader,
        timeout,
        stdout_prefix,
        context,
    )
}

fn run_partial_then_close(command: Command, context: &str) -> ClosedStdout {
    run_partial_then_close_with(command, LARGE_RUN_LIMITS.timeout, context)
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
fn closed_stdout_harness_does_not_join_stderr_on_wait_error() {
    let harness = include_str!("stdout_json_write_contract.rs")
        .split("#[test]")
        .next()
        .expect("harness helpers precede the tests");
    assert!(
        harness.contains("let code = wait_or_kill(child, timeout, context)?;"),
        "wait Err must return before joining stderr"
    );
    assert!(
        !harness.contains(concat!("let _ = child.", "start_kill()")),
        "start_kill failures must be surfaced"
    );
    assert!(
        harness.contains("read_prefix_bounded"),
        "the prefix read must be deadline-bounded"
    );
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

#[test]
fn run_json_early_failure_reader_gone_is_exit_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut command = assay_command(dir.path());
    command.args(["run", "--config", "missing.yaml", "--format", "json"]);

    let result = run_reader_already_gone(command, "run early failure reader gone");
    assert_infra_write_failure(&result, "run early failure reader gone");
}

#[test]
fn typed_cli_failure_reader_gone_is_exit_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut command = assay_command(dir.path());
    command.args(["coverage", "--format", "json"]);

    let result = run_reader_already_gone(command, "typed CLI failure reader gone");
    assert_infra_write_failure(&result, "typed CLI failure reader gone");
}

#[test]
fn policy_validate_json_success_reader_gone_is_exit_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("policy.yaml"), MCP_POLICY).expect("wrote policy");
    let mut command = assay_command(dir.path());
    command.args([
        "policy",
        "validate",
        "--input",
        "policy.yaml",
        "--format",
        "json",
    ]);

    let result = run_reader_already_gone(command, "policy validate reader gone");
    assert_infra_write_failure(&result, "policy validate reader gone");
}

#[cfg(unix)]
fn hanging_stderr_command() -> Command {
    let mut command = Command::new("sleep");
    command.arg("60");
    command
}

#[cfg(windows)]
fn hanging_stderr_command() -> Command {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Start-Sleep -Seconds 60",
    ]);
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

#[test]
fn partial_read_times_out_when_the_child_never_writes_stdout() {
    let started = Instant::now();
    let error = run_partial_then_close_with(
        hanging_stderr_command(),
        Duration::from_millis(250),
        "partial-read hang probe",
    )
    .expect_err("a silent child must not block forever on the prefix read");
    assert!(
        error.contains("partial read did not complete within"),
        "prefix read must name its deadline after kill/reap: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "prefix read hung instead of timing out: {:?}",
        started.elapsed()
    );
}

// --- the six bounded machine-stdout writers (#2441) -------------------------------------------
//
// These commands render a machine document and, with no `--out`, put it on stdout. They are outside
// the golden path #2439 repaired, so they kept a raw `println!` and a closed reader turned them into
// a Rust panic rather than the registered output-write exit.
//
// Every row drives the real binary twice. The first drive writes to `--out` and must succeed: a
// fixture that fails verification exits before the writer is ever reached, and a closed-reader row
// over such a fixture would pass while proving nothing. The second drive removes `--out` and hands
// the process a stdout whose reader is already gone.

/// One row per stdout branch this slice owns. Two `project-otel` variants because the command has
/// two independent write paths, not one shared seam.
#[derive(Clone, Copy, Debug)]
enum SinkCase {
    AdaptSkillScan,
    Attest,
    CaptureSkillSupplyChain,
    ProjectSkillBom,
    ProjectOtelCapabilitySurface,
    ProjectOtelToolDecisionTruth,
}

/// Written out independently of `SINK_CASES`. The equality below is what makes deleting a row fail
/// rather than silently shrinking the contract this file claims to cover.
const EXPECTED_SINK_COMMANDS: &[&str] = &[
    "evidence adapt-skill-scan",
    "evidence attest",
    "evidence capture-skill-supply-chain",
    "evidence project-skill-bom",
    "project-otel --capability-surface",
    "project-otel --tool-decision-truth",
];

const SINK_CASES: &[SinkCase] = &[
    SinkCase::AdaptSkillScan,
    SinkCase::Attest,
    SinkCase::CaptureSkillSupplyChain,
    SinkCase::ProjectSkillBom,
    SinkCase::ProjectOtelCapabilitySurface,
    SinkCase::ProjectOtelToolDecisionTruth,
];

impl SinkCase {
    fn label(self) -> &'static str {
        match self {
            Self::AdaptSkillScan => "evidence adapt-skill-scan",
            Self::Attest => "evidence attest",
            Self::CaptureSkillSupplyChain => "evidence capture-skill-supply-chain",
            Self::ProjectSkillBom => "evidence project-skill-bom",
            Self::ProjectOtelCapabilitySurface => "project-otel --capability-surface",
            Self::ProjectOtelToolDecisionTruth => "project-otel --tool-decision-truth",
        }
    }

    /// Build this row's inputs under `dir` and return the argv that reaches its stdout writer.
    fn argv(self, dir: &Path) -> Vec<std::ffi::OsString> {
        let s = |v: &str| std::ffi::OsString::from(v);
        let p = |v: std::path::PathBuf| v.into_os_string();
        match self {
            Self::AdaptSkillScan => vec![
                s("evidence"),
                s("adapt-skill-scan"),
                s("--carrier"),
                p(write_skill_carrier(dir)),
                s("--sarif"),
                p(write_sarif(dir)),
            ],
            Self::Attest => vec![
                s("evidence"),
                s("attest"),
                s("--bundle"),
                p(skill_supply_chain_bundle(dir)),
                s("--key"),
                p(write_ed25519_key(dir)),
            ],
            Self::CaptureSkillSupplyChain => vec![
                s("evidence"),
                s("capture-skill-supply-chain"),
                s("--root"),
                p(write_skill_root(dir)),
            ],
            Self::ProjectSkillBom => vec![
                s("evidence"),
                s("project-skill-bom"),
                p(skill_supply_chain_bundle(dir)),
            ],
            Self::ProjectOtelCapabilitySurface => vec![
                s("project-otel"),
                s("--capability-surface"),
                p(write_capability_surface(dir)),
            ],
            Self::ProjectOtelToolDecisionTruth => vec![
                s("project-otel"),
                s("--evidence-bundle"),
                p(tool_decision_truth_bundle(dir)),
            ],
        }
    }
}

fn write_skill_root(dir: &Path) -> std::path::PathBuf {
    let root = dir.join("skill");
    std::fs::create_dir_all(&root).expect("create skill root");
    std::fs::write(
        root.join("SKILL.md"),
        "---\nname: stdout-sink-probe\n---\n\nA skill body.\n",
    )
    .expect("write SKILL.md");
    root
}

fn write_skill_carrier(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("carrier.json");
    let carrier = serde_json::json!({
        "schema": "assay.skill_supply_chain.v0",
        "root": {"name": "s", "path": "skills/s"},
        "verdict": "review_complete",
        "reason_codes": [],
        "coverage": {
            "front_matter": "present", "body_text": "present", "scripts": "present",
            "lockfiles": "present", "transitive_traversal": "present"
        },
        "signals": [],
        "non_claims": ["review_complete_is_not_skill_safe"]
    });
    std::fs::write(&path, serde_json::to_string_pretty(&carrier).unwrap()).expect("write carrier");
    path
}

fn write_sarif(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("scan.sarif");
    let sarif = serde_json::json!({
        "version": "2.1.0",
        "runs": [{"tool": {"driver": {"name": "SkillSpector"}}, "results": []}]
    });
    std::fs::write(&path, serde_json::to_string_pretty(&sarif).unwrap()).expect("write sarif");
    path
}

/// A real carrier through the real producer and the real import gate, rather than a hand-written
/// one: the import gate refuses an incoherent carrier, so a bundle that exists here is one
/// `project-skill-bom` will actually verify.
fn skill_supply_chain_bundle(dir: &Path) -> std::path::PathBuf {
    let bundle = dir.join("ssc.tar.gz");
    if bundle.is_file() {
        return bundle;
    }
    let carrier = dir.join("captured-carrier.json");
    let captured = assay(
        dir,
        &[
            OsStr::new("evidence"),
            OsStr::new("capture-skill-supply-chain"),
            OsStr::new("--root"),
            write_skill_root(dir).as_os_str(),
            OsStr::new("--out"),
            carrier.as_os_str(),
        ],
        LIMITS,
        "capture carrier for bundle",
    );
    assert_eq!(
        captured.status.code(),
        Some(0),
        "capture must produce a carrier; stderr={}",
        String::from_utf8_lossy(&captured.stderr)
    );
    let imported = assay(
        dir,
        &[
            OsStr::new("evidence"),
            OsStr::new("import"),
            OsStr::new("skill-supply-chain"),
            OsStr::new("--carrier"),
            carrier.as_os_str(),
            OsStr::new("--bundle-out"),
            bundle.as_os_str(),
        ],
        LIMITS,
        "import carrier into bundle",
    );
    assert_eq!(
        imported.status.code(),
        Some(0),
        "import must produce a bundle; stderr={}",
        String::from_utf8_lossy(&imported.stderr)
    );
    bundle
}

/// Deterministic PKCS#8 PEM. The key material is a test constant, never generated, so the row does
/// not depend on an RNG and the fixture is reproducible.
fn write_ed25519_key(dir: &Path) -> std::path::PathBuf {
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    let path = dir.join("attest-key.pem");
    let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    let pem = key
        .to_pkcs8_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF)
        .expect("encode PKCS#8 PEM");
    std::fs::write(&path, pem.as_bytes()).expect("write key");
    path
}

fn write_capability_surface(dir: &Path) -> std::path::PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../assay-core/tests/fixtures/otel_projection/input.json");
    let input: Value =
        serde_json::from_str(&std::fs::read_to_string(&source).expect("read otel input fixture"))
            .expect("otel input fixture parses");
    let path = dir.join("capability-surface.json");
    std::fs::write(
        &path,
        serde_json::to_string(&input["capability_surface"]).unwrap(),
    )
    .expect("write capability surface");
    path
}

/// A carrier plus its recipe row, built through the same producers the verifier checks, so the
/// bundle passes `verify_and_collect` and the projection reaches the writer.
fn tool_decision_truth_bundle(dir: &Path) -> std::path::PathBuf {
    use assay_core::mcp::policy::McpPolicy;
    use assay_core::mcp::tool_decision_truth::{self as tdt, DecisionEvidence};
    use assay_evidence::bundle::BundleWriter;
    use assay_evidence::types::EvidenceEvent;

    let bundle = dir.join("tdt.tar.gz");
    let policy: McpPolicy = serde_json::from_value(serde_json::json!({
        "version": "1",
        "tools": {"allow": ["deploy"], "deny": ["delete_all"]},
        "schemas": {"deploy": {"type": "object", "required": ["env"],
            "properties": {"env": {"enum": ["staging", "prod"]}}}},
        "enforcement": {"unconstrained_tools": "warn"}
    }))
    .expect("policy parses");
    let carrier = tdt::build_classified_record(
        &policy,
        "deploy",
        &serde_json::json!({"env": "prod"}),
        0,
        b"stdout-sink-test-key-v0",
        "fixture-kid-v0",
        "authoritative_boundary",
        "c0",
        "ok",
        "present",
        &DecisionEvidence::default(),
    )
    .expect("build classified record");
    let row = tdt::pack_recipe_row(
        &carrier,
        carrier["decision_verdict"].as_str().expect("verdict"),
        "assay://evidence-event/run/0",
    )
    .expect("pack recipe row");

    let file = std::fs::File::create(&bundle).expect("create tdt bundle");
    let mut writer = BundleWriter::new(file);
    writer.add_event(EvidenceEvent::new(
        "assay.tool_decision_truth.v0",
        "urn:assay:test",
        "run",
        0,
        carrier,
    ));
    writer.add_event(EvidenceEvent::new(
        "assay.tool_decision_truth.recipe_row.v0",
        "urn:assay:test",
        "run",
        1,
        row,
    ));
    writer.finish().expect("finish tdt bundle");
    bundle
}

#[test]
fn the_sink_case_table_covers_the_expected_command_set() {
    let labels: Vec<&str> = SINK_CASES.iter().map(|case| case.label()).collect();
    assert_eq!(
        labels, EXPECTED_SINK_COMMANDS,
        "the driven machine-stdout table must cover exactly the bounded writer set"
    );
}

#[test]
fn every_sink_case_fixture_reaches_its_writer() {
    for &case in SINK_CASES {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("written.json");
        let mut argv = case.argv(dir.path());
        argv.push(std::ffi::OsString::from("--out"));
        argv.push(out.clone().into_os_string());

        let output = assay(dir.path(), &argv, LIMITS, case.label());
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}: the fixture must reach the writer, otherwise the closed-reader row is vacuous; \
             stderr={}",
            case.label(),
            String::from_utf8_lossy(&output.stderr)
        );
        let written = std::fs::read_to_string(&out)
            .unwrap_or_else(|error| panic!("{}: --out file unreadable: {error}", case.label()));
        serde_json::from_str::<Value>(&written)
            .unwrap_or_else(|error| panic!("{}: --out file is not JSON: {error}", case.label()));
    }
}

#[test]
fn every_sink_case_reader_gone_is_exit_three() {
    // Collected rather than asserted per row: one unconverted writer should not hide the state of
    // the other five, and the failure text is the inventory a reader needs.
    let mut offenders = Vec::new();
    for &case in SINK_CASES {
        let dir = tempfile::tempdir().expect("tempdir");
        let argv = case.argv(dir.path());
        let mut command = assay_command(dir.path());
        command.args(&argv);
        let result = run_reader_already_gone(command, case.label());
        if result.code != Some(3) {
            offenders.push(format!("{} => exit {:?}", case.label(), result.code));
            continue;
        }
        assert_no_rust_panic(&result.stderr, case.label());
    }
    assert!(
        offenders.is_empty(),
        "an undelivered machine document must exit 3, not abort or succeed: {}",
        offenders.join("; ")
    );
}
