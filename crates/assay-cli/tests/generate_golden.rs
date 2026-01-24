//! Golden tests for assay generate

use assert_cmd::Command;
use std::path::PathBuf;

fn normalize(yaml: &str) -> String {
    yaml.lines()
        .filter(|l| {
            !l.contains("Generated at") && !l.contains("Source:") && !l.contains("generated_at")
        }) // Handle both new comments and potential old keys
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(deprecated)]
fn run_golden(name: &str, strictness: Option<f64>) {
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

    if let Some(s) = strictness {
        cmd.arg("--strictness").arg(s.to_string());
    }

    let output = cmd.output().expect("failed to execute process");

    assert!(output.status.success());

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
    run_golden("passthrough", None);
}

#[test]
fn golden_dedup() {
    run_golden("dedup", None);
}

#[test]
fn golden_empty() {
    run_golden("empty", None);
}

#[test]
fn golden_entropy() {
    run_golden("entropy", None);
}

#[test]
fn golden_fanout() {
    // Needs strictness 1.0 to trigger fanout warning for 11 IPs (default threshold is ~27)
    run_golden("fanout", Some(1.0));
}
