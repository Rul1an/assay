//! `--format json` puts the diagnosis on stdout when the run fails, not only when it succeeds.
//!
//! The flag documents "machine-readable report on stdout". On the failure path it wrote nothing
//! there: the diagnosis went to `summary.json` and to a human-formatted block on stderr, so a caller
//! that captures stdout and reads the exit code, which is what a non-interactive consumer has, saw
//! an empty stream from a run that had produced a complete answer including a suggested next step
//! (#2150).
//!
//! Driven rather than asserted, for the reason the gap existed at all: every constant involved was
//! already correct. `summary.json` carried `reason_code` and `next_step`, the exit code was right,
//! and the only wrong thing was which stream the bytes reached. A test over constants cannot see
//! that, which is why this one runs the binary and reads its actual stdout.
//!
//! The explicit gate-command set below is intentionally separate from the driven rows: deleting a
//! row must fail rather than silently narrowing the contract the suite claims to cover (#2177).

use std::io::Write;
use std::process::Command;

/// A suite that fails at load: one assertion that no trace could fail (#1949).
fn vacuous_suite(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("suite.yaml");
    let mut f = std::fs::File::create(&path).expect("create suite");
    write!(
        f,
        r#"configVersion: 1
suite: stdout_contract_probe
model: dummy
tests:
  - id: t1
    input: hello
    assertions:
      - type: trace_must_call_tool
        tool: search
        min_calls: 0
"#
    )
    .expect("write suite");
    path
}

#[derive(Clone, Copy, Debug)]
enum GateFailureCase {
    Ci,
    Coverage,
    Run,
    Validate,
}

impl GateFailureCase {
    fn command(self) -> &'static str {
        match self {
            Self::Ci => "ci",
            Self::Coverage => "coverage",
            Self::Run => "run",
            Self::Validate => "validate",
        }
    }

    fn drive(self, dir: &std::path::Path, json: bool) -> std::process::Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
        command.current_dir(dir).env("NO_COLOR", "1");
        for (name, _) in std::env::vars_os() {
            if name
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("ASSAY_")
            {
                command.env_remove(name);
            }
        }

        match self {
            Self::Ci => {
                command.arg("ci");
                if json {
                    command.args(["--format", "json"]);
                }
                command.arg("--config");
                command.arg(dir.join("missing.yaml"));
            }
            Self::Coverage => {
                command.arg("coverage");
                if json {
                    command.args(["--format", "json"]);
                }
            }
            Self::Run => {
                command.arg("run");
                if json {
                    command.args(["--format", "json"]);
                }
                command.arg("--config");
                command.arg(vacuous_suite(dir));
            }
            Self::Validate => {
                command.arg("validate");
                if json {
                    command.args(["--format", "json"]);
                }
                command.arg("--config");
                command.arg(dir.join("missing.yaml"));
            }
        }

        command.output().expect("the binary runs")
    }

    fn expected_schema(self) -> &'static str {
        match self {
            Self::Validate => "assay.validate_report.v1",
            Self::Ci | Self::Coverage | Self::Run => "assay.run_summary.v1",
        }
    }
}

const EXPECTED_GATE_COMMANDS: &[&str] = &["ci", "coverage", "run", "validate"];
const GATE_FAILURE_CASES: &[GateFailureCase] = &[
    GateFailureCase::Ci,
    GateFailureCase::Coverage,
    GateFailureCase::Run,
    GateFailureCase::Validate,
];

#[test]
fn every_gate_command_has_a_driven_json_failure_contract() {
    // Keep this list independent from EXPECTED_GATE_COMMANDS. The equality is what makes deleting
    // a process row fail instead of silently shrinking the claimed gate surface.
    let commands: Vec<&str> = GATE_FAILURE_CASES
        .iter()
        .map(|case| case.command())
        .collect();

    assert_eq!(
        commands, EXPECTED_GATE_COMMANDS,
        "the driven JSON-failure table must cover the explicit gate-command set"
    );

    for &case in GATE_FAILURE_CASES {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = case.drive(dir.path(), true);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{} failure must retain exit 2; stderr: {}",
            case.command(),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !out.stdout.is_empty(),
            "{} advertised JSON but wrote zero bytes; stderr: {}",
            case.command(),
            String::from_utf8_lossy(&out.stderr)
        );
        let document: serde_json::Value =
            serde_json::from_slice(&out.stdout).unwrap_or_else(|error| {
                panic!(
                    "{} failure stdout must be JSON: {error}\n{}",
                    case.command(),
                    String::from_utf8_lossy(&out.stdout)
                )
            });
        assert_eq!(
            document["schema"],
            case.expected_schema(),
            "{} machine-document identity",
            case.command()
        );
        assert_eq!(
            document["schema_version"],
            1,
            "{} schema version",
            case.command()
        );
        assert_eq!(document["exit_code"], 2, "{} document exit", case.command());

        match case {
            GateFailureCase::Ci => assert_summary_failure(
                &document,
                "E_MISSING_CONFIG",
                "ci",
                Some(dir.path().join("summary.json")),
            ),
            GateFailureCase::Coverage => {
                assert_summary_failure(&document, "E_INVALID_ARGS", "coverage", None)
            }
            GateFailureCase::Run => assert_summary_failure(
                &document,
                "E_CFG_PARSE",
                "run",
                Some(dir.path().join("summary.json")),
            ),
            GateFailureCase::Validate => {
                assert_eq!(document["ok"], false);
                assert_eq!(document["diagnostics"][0]["code"], "E_CFG_PARSE");
                assert!(
                    document["suggested_actions"]
                        .as_array()
                        .is_some_and(|actions| !actions.is_empty()),
                    "validate failure must publish actionable remediation: {document}"
                );
            }
        }
    }
}

#[test]
fn every_gate_command_default_failure_keeps_stdout_clear() {
    for &case in GATE_FAILURE_CASES {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = case.drive(dir.path(), false);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{} default exit",
            case.command()
        );
        assert!(
            out.stdout.is_empty(),
            "{} default mode must keep stdout clear",
            case.command()
        );
        assert!(
            !out.stderr.is_empty(),
            "{} default mode must retain an operator diagnostic",
            case.command()
        );
    }
}

fn assert_summary_failure(
    document: &serde_json::Value,
    expected_reason: &str,
    command: &str,
    artifact: Option<std::path::PathBuf>,
) {
    assert_eq!(document["reason_code"], expected_reason, "{command} reason");
    assert!(
        document["next_step"]
            .as_str()
            .is_some_and(|step| step.contains("assay")),
        "{command} failure must publish actionable recovery: {document}"
    );

    if let Some(path) = artifact {
        let from_disk: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
        )
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
        assert_eq!(
            from_disk, *document,
            "{command} stdout and summary.json must be one authoritative report"
        );
    }
}

#[test]
fn a_completed_ci_writes_its_summary_to_stdout_under_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let suite = vacuous_suite(dir.path());

    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .current_dir(dir.path())
        .args([
            "ci",
            "--format",
            "json",
            "--allow-ineffective-assertions",
            "--config",
        ])
        .arg(&suite)
        .output()
        .expect("the binary runs");

    assert_eq!(
        out.status.code(),
        Some(1),
        "the trace-free probe completes with a failed test; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("completed ci stdout must be JSON: {e}\n{stdout}"));
    assert_eq!(parsed["schema"], "assay.run_summary.v1");
    assert_eq!(parsed["exit_code"], 1);

    let from_disk: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("summary.json")).expect("read summary"),
    )
    .expect("summary.json parses");
    assert_eq!(
        parsed, from_disk,
        "stdout and summary.json must be the same authoritative report"
    );
}

fn assert_coverage_failure_summary(out: std::process::Output, expected_message: &str) {
    assert_eq!(
        out.status.code(),
        Some(2),
        "an invalid coverage invocation is exit 2; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout).expect("stdout is utf-8");
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("coverage stdout must be JSON: {error}\n{stdout}"));
    assert_eq!(parsed["schema"], "assay.run_summary.v1");
    assert_eq!(parsed["exit_code"], 2);
    assert_eq!(parsed["reason_code"], "E_INVALID_ARGS");
    assert!(
        parsed["message"]
            .as_str()
            .is_some_and(|message| message.contains(expected_message)),
        "coverage diagnosis must retain the concrete argument failure, got {:?}",
        parsed["message"]
    );
    assert!(
        parsed["next_step"]
            .as_str()
            .is_some_and(|step| step.contains("assay")),
        "coverage diagnosis must contain actionable recovery, got {:?}",
        parsed["next_step"]
    );
}

#[test]
fn coverage_rejected_legacy_out_md_writes_the_diagnosis_to_stdout_under_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let markdown = dir.path().join("coverage.md");

    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["coverage", "--format", "json", "--out-md"])
        .arg(&markdown)
        .output()
        .expect("the binary runs");

    assert_coverage_failure_summary(out, "--out-md is only supported with --input mode");

    let text = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["coverage", "--out-md"])
        .arg(&markdown)
        .output()
        .expect("the binary runs");
    assert_eq!(text.status.code(), Some(2));
    assert!(
        text.stdout.is_empty(),
        "coverage text mode stays stdout-clean"
    );
    assert!(
        !text.stderr.is_empty(),
        "the operator diagnosis stays visible"
    );
}

#[test]
fn coverage_input_failure_does_not_publish_a_summary_to_stdout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("missing.jsonl");
    let report = dir.path().join("coverage.json");

    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["coverage", "--input"])
        .arg(missing)
        .arg("--out")
        .arg(report)
        .args(["--declared-tool", "read_file", "--format", "json"])
        .output()
        .expect("the binary runs");

    assert!(!out.status.success());
    assert!(
        out.stdout.is_empty(),
        "--input mode writes its report to --out and must not inherit the legacy stdout envelope"
    );
    assert!(
        !out.stderr.is_empty(),
        "the input-mode failure must remain visible to the operator"
    );
}
