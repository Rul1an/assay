//! `assay doctor` text interpolations must not write raw terminal-control bytes (#2265).
//!
//! Caller-derived strings — target path, parse-error detail, suite, and diagnostic
//! message — go through `render_safe(Sink::Stdout, …, usize::MAX)`. Assay-owned
//! labels and codes stay outside that helper. The JSON channel stays serializer-owned:
//! no raw `0x1b`/`0x07` on the wire, and decoded string values keep the original
//! control bytes. `--db` is a non-claim: doctor does not echo it today.

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// The four caller-derived interpolations on doctor's text channel.
/// `--db` is intentionally absent: the command does not echo it.
const CALLER_INTERPOLATIONS: &[&str] = &[
    "target_path",
    "parse_error_detail",
    "suite",
    "diagnostic_message",
];

const OSC8: &str = "\u{1b}]8;;http://evil\u{07}click\u{1b}]8;;\u{07}";
const CSI: &str = "\u{1b}[31m";
const LONE_ESC: &str = "\u{1b}";
const BEL: &str = "\u{07}";

const MATCHING_TRACE: &str = r#"{"schema_version": 1, "type": "assay.trace", "request_id": "hello_1", "prompt": "hello_prompt", "response": "Hello Assay", "model": "trace", "provider": "trace"}
"#;

struct DoctorRun {
    code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn all_controls() -> String {
    format!("{OSC8}{CSI}{LONE_ESC}{BEL}")
}

/// YAML double-quoted escapes for the same payload. Raw C0 bytes are rejected by
/// the YAML parser; these decode into `EvalConfig.suite` as the live controls.
const YAML_ESCAPED_CONTROLS: &str =
    r#"\u001b]8;;http://evil\u0007click\u001b]8;;\u0007\u001b[31m\u001b\u0007"#;

fn eval_yaml(suite_yaml: &str) -> String {
    format!(
        r#"configVersion: 1
suite: "{suite_yaml}"
model: "trace"
tests:
  - id: "render_safety"
    input:
      prompt: "hello_prompt"
    expected:
      type: regex_match
      pattern: "Hello\\s+Assay"
      flags: ["i"]
"#
    )
}

fn doctor(cwd: &Path, args: &[&str]) -> DoctorRun {
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
        .arg("doctor")
        .args(args);
    let output = run_bounded(command, b"", GOLDEN_PATH_LIMITS, "assay doctor")
        .unwrap_or_else(|error| panic!("{error}"));
    DoctorRun {
        code: output
            .status
            .code()
            .expect("doctor exited by code rather than by signal"),
        stdout: output.stdout,
        stderr: output.stderr,
    }
}

fn stdout_text(run: &DoctorRun) -> String {
    String::from_utf8_lossy(&run.stdout).into_owned()
}

fn assert_no_raw_esc_or_bel(label: &str, bytes: &[u8]) {
    let esc = bytes.iter().filter(|b| **b == 0x1b).count();
    let bel = bytes.iter().filter(|b| **b == 0x07).count();
    assert_eq!(
        (esc, bel),
        (0, 0),
        "{label} still has raw ESC×{esc} BEL×{bel}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(bytes),
        ""
    );
}

fn write_tree(dir: &Path, suite: &str) {
    std::fs::write(dir.join("eval.yaml"), eval_yaml(suite)).expect("wrote eval.yaml");
    std::fs::create_dir_all(dir.join("traces")).expect("created traces/");
    std::fs::write(dir.join("traces/present.jsonl"), MATCHING_TRACE).expect("wrote trace");
}

#[test]
fn caller_interpolation_classes_are_inventoried() {
    assert_eq!(
        CALLER_INTERPOLATIONS,
        [
            "target_path",
            "parse_error_detail",
            "suite",
            "diagnostic_message"
        ]
    );
}

#[test]
fn benign_doctor_text_and_owned_labels_stay_byte_stable() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_tree(dir.path(), "hello_smoke");
    let long_trace = format!("{}missing.jsonl", "x".repeat(300));

    let run = doctor(
        dir.path(),
        &[
            "--format",
            "text",
            "--config",
            "eval.yaml",
            "--trace-file",
            &long_trace,
        ],
    );
    let text = stdout_text(&run);

    assert_eq!(
        run.code, 2,
        "missing long trace is still a config-class miss"
    );
    assert!(
        !text.contains("(truncated)"),
        "benign caller text must not hit the default 256-char render cap; stdout:\n{text}"
    );
    assert!(
        text.contains(&long_trace),
        "the full benign --trace-file path must survive; stdout:\n{text}"
    );
    for label in [
        "Policy Check:",
        "  Config:   eval.yaml",
        "  Suite:    hello_smoke",
        "    - [E_PATH_NOT_FOUND] [error] Trace file not found: ",
    ] {
        assert!(
            text.contains(label),
            "Assay-owned label {label:?} must stay byte-stable; stdout:\n{text}"
        );
    }
    assert_no_raw_esc_or_bel("benign text", &run.stdout);
}

#[test]
fn hostile_trace_and_suite_are_inert_on_text_and_escaped_on_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hostile = all_controls();
    write_tree(dir.path(), &format!("suite-{YAML_ESCAPED_CONTROLS}-end"));
    let trace = format!("{hostile}.jsonl");

    let text_run = doctor(
        dir.path(),
        &[
            "--format",
            "text",
            "--config",
            "eval.yaml",
            "--trace-file",
            &trace,
        ],
    );
    let text = stdout_text(&text_run);
    assert_eq!(text_run.code, 2);
    assert_no_raw_esc_or_bel("doctor text (suite + diagnostic)", &text_run.stdout);
    assert!(
        text.contains("  Suite:    "),
        "suite interpolation must still render; stdout:\n{text}"
    );
    assert!(
        text.contains("click") && text.contains("suite-") && text.contains("-end"),
        "sanitized suite/path needles must remain visible; stdout:\n{text}"
    );
    assert!(
        text.contains("    - [E_PATH_NOT_FOUND] [error] Trace file not found: "),
        "owned diagnostic labels must stay byte-stable; stdout:\n{text}"
    );
    assert!(
        !text.contains("http://evil"),
        "OSC8 target must not survive on the text channel; stdout:\n{text}"
    );

    let json_run = doctor(
        dir.path(),
        &[
            "--format",
            "json",
            "--config",
            "eval.yaml",
            "--trace-file",
            &trace,
        ],
    );
    assert_eq!(json_run.code, 2);
    assert_no_raw_esc_or_bel("doctor JSON wire", &json_run.stdout);
    let wire = String::from_utf8_lossy(&json_run.stdout);
    assert!(
        wire.contains("\\u001b") && wire.contains("\\u0007"),
        "JSON must keep the escaped machine shape, not a stdout-sanitized rewrite; wire:\n{wire}"
    );
    let document: Value = serde_json::from_slice(&json_run.stdout).unwrap_or_else(|error| {
        panic!(
            "doctor JSON is not one document: {error}\nstdout:\n{wire}\nstderr:\n{}",
            String::from_utf8_lossy(&json_run.stderr)
        )
    });
    let message = document["data_diagnostics"][0]["message"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(
        document["data_diagnostics"][0]["code"], "E_PATH_NOT_FOUND",
        "owned JSON code must stay byte-stable; document={document}"
    );
    assert_eq!(
        document["data_diagnostics"][0]["severity"], "error",
        "owned JSON severity must stay byte-stable; document={document}"
    );
    assert!(
        message.contains('\u{1b}') && message.contains('\u{07}') && message.contains(&trace),
        "decoded JSON message must keep the original control bytes; message={message:?}"
    );
}

#[test]
fn hostile_config_path_and_parse_error_are_inert_on_text() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hostile = all_controls();
    let missing = format!("{hostile}missing.yaml");
    let path_run = doctor(dir.path(), &["--format", "text", "--config", &missing]);
    let path_text = stdout_text(&path_run);
    assert_eq!(path_run.code, 2);
    assert_no_raw_esc_or_bel("doctor text (target_path)", &path_run.stdout);
    assert!(
        path_text.contains("  File:     "),
        "target-path interpolation must still render; stdout:\n{path_text}"
    );
    assert!(
        path_text.contains("Config Status: FAILED"),
        "owned config-failure labels must stay byte-stable; stdout:\n{path_text}"
    );
    assert!(
        !path_text.contains("http://evil"),
        "OSC8 in --config must not survive on the text channel; stdout:\n{path_text}"
    );

    std::fs::write(
        dir.path().join("broken.yaml"),
        format!("\"{YAML_ESCAPED_CONTROLS}quoted-root\"\n"),
    )
    .expect("wrote broken.yaml");
    let parse_run = doctor(dir.path(), &["--format", "text", "--config", "broken.yaml"]);
    let parse_text = stdout_text(&parse_run);
    assert_eq!(parse_run.code, 2);
    assert_no_raw_esc_or_bel("doctor text (parse_error_detail)", &parse_run.stdout);
    assert!(
        parse_text.contains("  Error:    "),
        "parse-error interpolation must still render; stdout:\n{parse_text}"
    );
    assert!(
        parse_text.contains("quoted-root") && parse_text.contains("  File:     broken.yaml"),
        "sanitized parse detail and owned file label must remain; stdout:\n{parse_text}"
    );
}
