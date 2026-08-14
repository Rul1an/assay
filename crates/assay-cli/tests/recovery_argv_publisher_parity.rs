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
//! - list-presets recovery omitting `--format json`
//! - preset/non-hello init dropping the generated `--config`
//! - from-trace recovery publishing generator-event JSONL as `--trace-file`
//! - formatter changed to a shell join
//! - a publisher or reason variant silently dropped from the table
//! - a new `InitSuccess` variant with no driven `PUBLISHERS` case

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
    InitPreset,
    InitFromTrace,
    InitListPresets,
}

const PUBLISHERS: &[Publisher] = &[
    Publisher::CfgParse,
    Publisher::PolicyParse,
    Publisher::EvidenceUnreadable,
    Publisher::InitHelloTrace,
    Publisher::InitPreset,
    Publisher::InitFromTrace,
    Publisher::InitListPresets,
];

/// Measured classes from #2371: dash-prefixed option lookalikes, spaces, quotes,
/// newline, BEL, and shell metacharacters. `--` stays because it is both a
/// separator and a legal operand.
const HOSTILE_VALUES: &[&str] = &[
    "-x",
    "--format",
    "--",
    "file with spaces.yaml",
    "file\"quote.yaml",
    "file\nname.yaml",
    "file\u{0007}.yaml",
    "file;$(touch x).yaml",
];

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
        Publisher::InitPreset | Publisher::InitFromTrace => vec![
            "assay".into(),
            "validate".into(),
            format!("--config={value}"),
            "--format".into(),
            "json".into(),
        ],
        Publisher::InitListPresets => vec![
            "assay".into(),
            "init".into(),
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
        Publisher::InitPreset => {
            let output = assay(
                dir,
                &[
                    "init",
                    "--preset",
                    "dev",
                    "--format",
                    "json",
                    &format!("--config={value}"),
                ],
                &format!("init-preset {value}"),
            );
            assert_eq!(output.status.code(), Some(0), "init-preset {value}");
            json_document(&output, &format!("init-preset {value}"))
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
        Publisher::InitListPresets => {
            let output = assay(
                dir,
                &["init", "--list-presets", "--format", "json"],
                "init-list-presets",
            );
            assert_eq!(output.status.code(), Some(0), "init-list-presets");
            json_document(&output, "init-list-presets")
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
        Publisher::InitHelloTrace | Publisher::InitPreset | Publisher::InitFromTrace => {
            assert_eq!(
                recovered.status.code(),
                Some(0),
                "{context}: init recovery must exit 0; stderr:\n{}",
                String::from_utf8_lossy(&recovered.stderr)
            );
            assert_eq!(
                document["schema"], "assay.validate_report.v1",
                "{context}: init recovery must reach validate JSON, not clap usage"
            );
            assert_eq!(
                document["ok"], true,
                "{context}: init recovery must succeed (ok:true), not a failed validate report"
            );
            assert_eq!(
                document["exit_code"], 0,
                "{context}: init recovery must publish exit_code 0"
            );
        }
        Publisher::InitListPresets => {
            assert_eq!(
                recovered.status.code(),
                Some(0),
                "{context}: list-presets recovery must exit 0; stderr:\n{}",
                String::from_utf8_lossy(&recovered.stderr)
            );
            assert_eq!(
                document["schema"], "assay.init_report.v0",
                "{context}: list-presets recovery must keep machine output, not the text stream"
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

/// Scope: `InitSuccess` variants in `init_report.rs`, bound to `PUBLISHERS`.
/// A new variant without a driven `Publisher::Init…` case fails. The succeed
/// signature check below only repeats the type boundary; this test is the
/// variant-set bind. Reason-code publishers stay in
/// `exit_codes::tests::every_reason_that_publishes_argv_is_a_known_executable_recovery`.
#[test]
fn every_init_success_variant_is_a_driven_publisher() {
    let source = include_str!("../src/cli/commands/init_report.rs");
    let table = publishers_table(include_str!("recovery_argv_publisher_parity.rs"));
    let variants = init_success_variants(source);
    assert!(
        !variants.is_empty(),
        "InitSuccess must keep its variants; an empty scan is not a clean inventory"
    );
    for variant in variants {
        for id in driven_ids_for_variant(&variant) {
            assert!(
                table.contains(&format!("Publisher::{id}")),
                "InitSuccess::{variant} publishes Run argv but {id} is absent from PUBLISHERS"
            );
        }
    }
}

fn publishers_table(harness: &str) -> &str {
    const START: &str = "const PUBLISHERS: &[Publisher] = &[";
    let start = harness
        .find(START)
        .expect("recovery harness must keep a PUBLISHERS table");
    let body = &harness[start + START.len()..];
    let end = body.find(']').expect("PUBLISHERS table must close");
    &body[..end]
}

fn init_success_variants(source: &str) -> Vec<String> {
    let start = source
        .find("pub(crate) enum InitSuccess {")
        .expect("InitSuccess must stay in init_report.rs");
    let after = &source[start..];
    let open = after.find('{').expect("InitSuccess body");
    let mut depth = 0;
    let mut end = open;
    for (idx, ch) in after[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + idx;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &after[open + 1..end];
    let mut names = Vec::new();
    let mut depth = 0;
    let mut token = String::new();
    for ch in body.chars() {
        match ch {
            '{' => {
                depth += 1;
                token.push(ch);
            }
            '}' => {
                depth -= 1;
                token.push(ch);
            }
            ',' if depth == 0 => {
                if let Some(name) = variant_ident(&token) {
                    names.push(name);
                }
                token.clear();
            }
            _ => token.push(ch),
        }
    }
    if let Some(name) = variant_ident(&token) {
        names.push(name);
    }
    names
}

fn variant_ident(piece: &str) -> Option<String> {
    let code = piece
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("///") && !trimmed.starts_with("//") && !trimmed.starts_with("#[")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let ident = code
        .split_whitespace()
        .find(|token| token.chars().next().is_some_and(|c| c.is_ascii_uppercase()))?;
    let name = ident
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .next()?;
    (!name.is_empty()).then(|| name.to_string())
}

fn driven_ids_for_variant(variant: &str) -> Vec<String> {
    match variant {
        // One variant, three succeed sites. Other variants map to Init{Name}.
        "Validate" => vec![
            "InitHelloTrace".into(),
            "InitPreset".into(),
            "InitFromTrace".into(),
        ],
        other => vec![format!("Init{other}")],
    }
}

/// Scope: `init.rs` succeed call sites and the `InitReport::succeed` signature.
/// A raw argv slice at either site is unclassified. Reason-code publishers stay
/// in `exit_codes::tests::every_reason_that_publishes_argv_is_a_known_executable_recovery`.
#[test]
fn every_init_succeed_site_uses_the_classified_enum() {
    let init = include_str!("../src/cli/commands/init.rs");
    let report = include_str!("../src/cli/commands/init_report.rs");
    assert!(
        report.contains("fn succeed(self, next: &InitSuccess,"),
        "InitReport::succeed must take InitSuccess so a raw argv cannot be published"
    );
    let calls: Vec<(usize, &str)> = init
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(".succeed("))
        .map(|(idx, line)| (idx + 1, line.trim()))
        .collect();
    assert!(
        !calls.is_empty(),
        "init.rs must keep its succeed sites; an empty scan is not a clean inventory"
    );
    for (line_no, call) in &calls {
        assert!(
            !call.contains("succeed(&["),
            "unclassified init.rs succeed on line {line_no} publishes a raw argv slice: {call}"
        );
    }
}

#[test]
fn dash_prefixed_evidence_positional_recovery_reaches_unreadable() {
    drive_publisher(Publisher::EvidenceUnreadable, "-b.tar.gz");
}

#[test]
fn init_success_publishers_bind_generated_config_and_json() {
    drive_publisher(Publisher::InitHelloTrace, "-weird.yaml");
    drive_publisher(Publisher::InitPreset, "-weird.yaml");
    drive_publisher(Publisher::InitPreset, "custom.yaml");
    drive_publisher(Publisher::InitFromTrace, "-weird.yaml");
}

#[test]
fn init_list_presets_recovery_keeps_machine_output() {
    drive_publisher(Publisher::InitListPresets, "unused");
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
