//! Cross-publisher hostile-value parity for every executable recovery argv (#2371).
//!
//! Drives the real CLI. Published argv is executed without a shell. Expected
//! shapes are hand-written literals, not computed through the helpers under test.
//!
//! Discriminating mutations this harness is written to fail:
//! - split option binding (`--flag` / value)
//! - omitted positional `--`
//! - `--` placed after the operand
//! - omitted init `--format json`
//! - formatter changed to a shell join
//! - a publisher or reason variant silently dropped from the table

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, ProcessLimits};
use serde_json::Value;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

const LIMITS: ProcessLimits = ProcessLimits::new(Duration::from_secs(10), 64 * 1024, 64 * 1024);
const MALFORMED_YAML: &str = "version: [\n";
const FROM_TRACE_EVENTS: &str =
    "{\"type\":\"file_open\",\"path\":\"/workspace/app.py\",\"pid\":1,\"timestamp\":1}\n";
const FROM_TRACE: &str = "-events.jsonl";

/// Every publisher of `Run argv:` that carries a caller-controlled value.
/// Removing a row is the "skipped reason variant" mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Publisher {
    CfgParse,
    PolicyParse,
    EvidenceUnreadable,
    InitHelloTrace,
    InitFromTrace,
}

const PUBLISHERS: &[Publisher] = &[
    Publisher::CfgParse,
    Publisher::PolicyParse,
    Publisher::EvidenceUnreadable,
    Publisher::InitHelloTrace,
    Publisher::InitFromTrace,
];

/// Values beginning with `-` (except bare `-`) discriminate split from fused binding.
const HOSTILE_VALUES: &[&str] = &["-x", "--format", "--"];

fn assay<T: AsRef<OsStr>>(cwd: &Path, args: &[T], context: &str) -> Output {
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
    command.current_dir(cwd).env("NO_COLOR", "1").args(args);
    run_bounded(command, b"", LIMITS, context).unwrap_or_else(|error| panic!("{error}"))
}

fn parse_recovery_argv(next_step: &str, context: &str) -> Vec<String> {
    let encoded = next_step.strip_prefix("Run argv: ").unwrap_or_else(|| {
        panic!("{context}: recovery must be JSON argv, not a shell join: {next_step}")
    });
    serde_json::from_str(encoded).unwrap_or_else(|error| {
        panic!("{context}: recovery argv must parse as JSON: {error}: {next_step}")
    })
}

fn json_document(output: &Output, context: &str) -> Value {
    assert!(
        !output.stdout.is_empty(),
        "{context}: empty stdout; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{context}: stdout must be JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn refute_clap_refusal(output: &Output, context: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unexpected argument") && !stderr.contains("Usage:"),
        "{context}: published argv was refused by clap:\n{stderr}"
    );
}

fn expected_argv(publisher: Publisher, value: &str) -> Vec<String> {
    match publisher {
        Publisher::CfgParse => vec![
            "assay".into(),
            "doctor".into(),
            format!("--config={value}"),
            "--format".into(),
            "json".into(),
        ],
        Publisher::PolicyParse => vec![
            "assay".into(),
            "policy".into(),
            "validate".into(),
            format!("--input={value}"),
            "--format".into(),
            "json".into(),
        ],
        Publisher::EvidenceUnreadable => vec![
            "assay".into(),
            "evidence".into(),
            "show".into(),
            "--format".into(),
            "json".into(),
            "--".into(),
            value.into(),
        ],
        Publisher::InitHelloTrace => vec![
            "assay".into(),
            "validate".into(),
            format!("--config={value}"),
            "--trace-file=traces/hello.jsonl".into(),
            "--format".into(),
            "json".into(),
        ],
        Publisher::InitFromTrace => vec![
            "assay".into(),
            "validate".into(),
            format!("--config={value}"),
            format!("--trace-file={FROM_TRACE}"),
            "--format".into(),
            "json".into(),
        ],
    }
}

fn publish(dir: &Path, publisher: Publisher, value: &str) -> Value {
    match publisher {
        Publisher::CfgParse => {
            std::fs::write(dir.join(value), MALFORMED_YAML).expect("write malformed config");
            let output = assay(
                dir,
                &["run", "--format", "json", &format!("--config={value}")],
                &format!("cfg-parse {value}"),
            );
            assert_eq!(output.status.code(), Some(2), "cfg-parse {value}");
            json_document(&output, &format!("cfg-parse {value}"))
        }
        Publisher::PolicyParse => {
            std::fs::write(dir.join(value), MALFORMED_YAML).expect("write malformed policy");
            let output = assay(
                dir,
                &[
                    "policy",
                    "validate",
                    "--format",
                    "json",
                    &format!("--input={value}"),
                ],
                &format!("policy-parse {value}"),
            );
            assert_eq!(output.status.code(), Some(2), "policy-parse {value}");
            json_document(&output, &format!("policy-parse {value}"))
        }
        Publisher::EvidenceUnreadable => {
            let output = assay(
                dir,
                &["evidence", "show", "--format", "json", "--", value],
                &format!("evidence {value}"),
            );
            assert_eq!(output.status.code(), Some(2), "evidence {value}");
            json_document(&output, &format!("evidence {value}"))
        }
        Publisher::InitHelloTrace => {
            let output = assay(
                dir,
                &[
                    "init",
                    "--preset",
                    "dev",
                    "--hello-trace",
                    "--format",
                    "json",
                    &format!("--config={value}"),
                ],
                &format!("init-hello {value}"),
            );
            assert_eq!(output.status.code(), Some(0), "init-hello {value}");
            json_document(&output, &format!("init-hello {value}"))
        }
        Publisher::InitFromTrace => {
            std::fs::write(dir.join(FROM_TRACE), FROM_TRACE_EVENTS).expect("write from-trace");
            let output = assay(
                dir,
                &[
                    "init",
                    "--format",
                    "json",
                    &format!("--from-trace={FROM_TRACE}"),
                    &format!("--config={value}"),
                ],
                &format!("init-from-trace {value}"),
            );
            assert_eq!(output.status.code(), Some(0), "init-from-trace {value}");
            json_document(&output, &format!("init-from-trace {value}"))
        }
    }
}

fn assert_intended_outcome(publisher: Publisher, recovered: &Output, context: &str) {
    refute_clap_refusal(recovered, context);
    let document = json_document(recovered, context);
    match publisher {
        Publisher::CfgParse => {
            assert_eq!(document["reason_code"], "E_CFG_PARSE", "{context}");
        }
        Publisher::PolicyParse => {
            assert_eq!(document["reason_code"], "E_POLICY_PARSE", "{context}");
        }
        Publisher::EvidenceUnreadable => {
            assert_eq!(
                document["reason_code"], "E_EVIDENCE_UNREADABLE",
                "{context}"
            );
        }
        Publisher::InitHelloTrace | Publisher::InitFromTrace => {
            assert_eq!(
                document["schema"], "assay.validate_report.v1",
                "{context}: init recovery must reach validate JSON, not clap usage"
            );
        }
    }
}

fn drive_publisher(publisher: Publisher, value: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let context = format!("{publisher:?} value={value:?}");
    let document = publish(dir.path(), publisher, value);
    let next_step = document["next_step"]
        .as_str()
        .unwrap_or_else(|| panic!("{context}: next_step must be a string: {document}"));
    let recovery = parse_recovery_argv(next_step, &context);
    assert_eq!(
        recovery,
        expected_argv(publisher, value),
        "{context}: published argv must match the hand-written shape"
    );
    let recovered = assay(dir.path(), &recovery[1..], &format!("execute {context}"));
    assert_intended_outcome(publisher, &recovered, &format!("execute {context}"));
}

#[test]
fn every_executable_recovery_publisher_is_driven() {
    assert_eq!(
        PUBLISHERS,
        &[
            Publisher::CfgParse,
            Publisher::PolicyParse,
            Publisher::EvidenceUnreadable,
            Publisher::InitHelloTrace,
            Publisher::InitFromTrace,
        ],
        "dropping a publisher is the skipped-variant mutation"
    );
}

#[test]
fn dash_prefixed_evidence_positional_recovery_reaches_unreadable() {
    drive_publisher(Publisher::EvidenceUnreadable, "-b.tar.gz");
}

#[test]
fn both_init_success_publishers_recover_dash_prefixed_config_as_json() {
    drive_publisher(Publisher::InitHelloTrace, "-weird.yaml");
    drive_publisher(Publisher::InitFromTrace, "-weird.yaml");
}

#[test]
fn every_publisher_round_trips_dash_prefixed_hostile_values() {
    for publisher in PUBLISHERS {
        for value in HOSTILE_VALUES {
            drive_publisher(*publisher, value);
        }
    }
}

#[test]
fn bel_bearing_malformed_config_recovers_as_cfg_parse() {
    drive_publisher(Publisher::CfgParse, "cfg\u{0007}.yaml");
}
