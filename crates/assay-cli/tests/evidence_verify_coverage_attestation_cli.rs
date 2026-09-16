//! Real CLI contract for CAP-1 document verification; no producer, claim gate, or signing.
#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use assay_evidence::{Cap1AdmissionLimits, CAP1_SCHEMA_SHA256, CAP1_SCHEMA_SOURCE};
use bounded_process::{run_bounded, ProcessLimits};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../assay-evidence/tests/fixtures/cap1/normative")
        .join(name)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn run(document: &Path, extra: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_assay"));
    cmd.args(["evidence", "verify-coverage-attestation", "--document"])
        .arg(document)
        .args(extra);
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_")
        {
            cmd.env_remove(name);
        }
    }
    run_bounded(
        cmd,
        &[],
        ProcessLimits::new(Duration::from_secs(60), 4096, 4096),
        "coverage attestation verification CLI",
    )
    .expect("bounded CLI run")
}

fn parse(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout must parse as JSON ({e}): {:?}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn padded_json(len: usize) -> Vec<u8> {
    assert!(len >= 8);
    let mut bytes = Vec::with_capacity(len);
    bytes.extend_from_slice(b"{\"p\":\"");
    bytes.resize(len - 2, b'x');
    bytes.extend_from_slice(b"\"}");
    bytes
}

#[test]
fn conforms_pv01() {
    let path = fixture("PV-01.json");
    let bytes = fs::read(&path).unwrap();
    let output = run(&path, &[]);
    let parsed = parse(&output);
    assert_eq!(output.status.code(), Some(0), "{:?}", output);
    assert_eq!(
        parsed,
        json!({
            "schema": "assay.evidence.coverage_attestation.verify.v1",
            "outcome": "cap1_conforms",
            "reason": null,
            "input_sha256": sha256_hex(&bytes),
            "input_bytes": bytes.len(),
            "schema_pin": {
                "source": CAP1_SCHEMA_SOURCE,
                "sha256": CAP1_SCHEMA_SHA256,
            },
            "refusal": null,
            "document": {
                "profile": "cap/1",
                "strata": 1,
                "integrity_complete": true,
                "absence_assertions": 1,
            },
            "claim_gate": "not_evaluated",
            "non_claims": [
                "no_activity_completeness",
                "no_provider_outcome",
                "no_automatic_trust"
            ],
        })
    );
}

#[test]
fn schema_refusal_does_not_echo_instance_value() {
    let output = run(&fixture("NC-02.json"), &[]);
    assert_eq!(output.status.code(), Some(2), "{:?}", output);
    let v = parse(&output);
    assert_eq!(v["outcome"], "cap1_refused");
    assert_eq!(v["reason"], "cap1_shape");
    assert_eq!(v["refusal"]["stage"], "schema");
    assert_eq!(v["refusal"]["rule"], Value::Null);
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("other"),
        "refusal must not echo the unknown disposition"
    );
}

#[test]
fn rule_refusal_names_r1_at_the_stratum() {
    let output = run(&fixture("NC-05.json"), &[]);
    assert_eq!(output.status.code(), Some(2), "{:?}", output);
    let v = parse(&output);
    assert_eq!(v["outcome"], "cap1_refused");
    assert_eq!(v["reason"], "cap1_rules");
    assert_eq!(v["refusal"]["stage"], "rules");
    assert_eq!(v["refusal"]["rule"], "R1-no-silent-remainder");
    assert_eq!(v["refusal"]["at"], "/strata/0");
}

#[test]
fn missing_path_is_unavailable_not_refused() {
    let path = tempfile::tempdir().unwrap().path().join("absent.json");
    let output = run(&path, &[]);
    assert_eq!(output.status.code(), Some(2), "{:?}", output);
    let v = parse(&output);
    assert_eq!(v["outcome"], "verification_unavailable");
    assert_eq!(v["reason"], "io_unavailable");
    assert_eq!(v["refusal"], Value::Null);
    assert_eq!(v["input_sha256"], Value::Null);
}

#[test]
fn hard_max_is_inclusive_and_next_byte_is_admission() {
    let dir = tempfile::tempdir().unwrap();
    let exact = dir.path().join("exact.json");
    let over = dir.path().join("over.json");
    let exact_bytes = padded_json(Cap1AdmissionLimits::HARD_MAX_BYTES);
    let mut over_bytes = exact_bytes.clone();
    over_bytes.push(b'x');
    fs::write(&exact, &exact_bytes).unwrap();
    fs::write(&over, &over_bytes).unwrap();

    let at_limit = run(&exact, &[]);
    assert_eq!(at_limit.status.code(), Some(2), "{:?}", at_limit);
    let v = parse(&at_limit);
    let stage = v["refusal"]["stage"].as_str().unwrap_or("");
    assert!(
        stage == "schema" || stage == "rules",
        "HARD_MAX valid JSON must reach schema/rules, got {v}"
    );
    assert_eq!(v["input_sha256"], sha256_hex(&exact_bytes));

    let plus_one = run(&over, &[]);
    assert_eq!(plus_one.status.code(), Some(2), "{:?}", plus_one);
    let v = parse(&plus_one);
    assert_eq!(v["reason"], "resource_hard");
    assert_eq!(v["refusal"]["stage"], "admission");
    assert_eq!(v["input_sha256"], Value::Null);
}

#[test]
fn selected_budget_cannot_raise_the_hard_maximum() {
    let dir = tempfile::tempdir().unwrap();
    let over = dir.path().join("over.json");
    let mut bytes = padded_json(Cap1AdmissionLimits::HARD_MAX_BYTES);
    bytes.push(b'x');
    fs::write(&over, bytes).unwrap();
    let output = run(&over, &["--max-bytes", "2000000"]);
    assert_eq!(output.status.code(), Some(2), "{:?}", output);
    let v = parse(&output);
    assert_eq!(v["reason"], "resource_hard");
    assert_eq!(v["refusal"]["stage"], "admission");
}

#[test]
fn conforms_stdout_is_byte_identical_across_runs() {
    let path = fixture("PV-01.json");
    let first = run(&path, &[]);
    let second = run(&path, &[]);
    assert_eq!(parse(&first)["outcome"], "cap1_conforms");
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.status.code(), Some(0));
    let text = String::from_utf8_lossy(&first.stdout);
    assert!(!text.contains(path.to_string_lossy().as_ref()));
}
