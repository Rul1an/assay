//! Golden tests for assay generate (Phase 3)

#![allow(deprecated)] // cargo_bin is deprecated but still works

use assert_cmd::Command;
use std::path::PathBuf;

fn normalize(yaml: &str) -> String {
    yaml.lines()
        .filter(|l| {
            !l.contains("Generated at") && !l.contains("Source:") && !l.contains("generated_at")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn run_golden(name: &str, use_heuristics: bool) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let input_path = dir.join("input.jsonl");
    let expected_path = dir.join("expected.yaml");

    // We run the binary "assay" with the "generate" subcommand
    let mut cmd = Command::cargo_bin("assay").unwrap();
    cmd.arg("generate")
        .arg("--input")
        .arg(&input_path)
        .arg("--name")
        .arg("Test Policy")
        .arg("--dry-run");

    if use_heuristics {
        cmd.arg("--heuristics");
    }

    let output = cmd.output().expect("failed to execute process");

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected = std::fs::read_to_string(expected_path).expect("failed to read expected.yaml");

    assert_eq!(
        normalize(&stdout).trim(),
        normalize(&expected).trim(),
        "Mismatch in {}",
        name
    );
}

#[test]
fn golden_passthrough() {
    run_golden("passthrough", false);
}

#[test]
fn golden_dedup() {
    run_golden("dedup", false);
}

#[test]
fn golden_empty() {
    run_golden("empty", false);
}

#[test]
fn golden_entropy() {
    // With heuristics to trigger entropy detection
    run_golden("entropy", true);
}

#[test]
fn golden_fanout() {
    // Tests policy generation with many network destinations
    // Note: inline heuristics only checks entropy and sensitive ports, not fanout count
    run_golden("fanout", true);
}
