use std::path::Path;
use std::process::Command;

#[test]
fn test_golden_harness() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let golden_dir = Path::new(manifest_dir).join("../../tests/fixtures/golden"); // Go up to workspace root

    // Build binary first usually, but for test we can use cargo run?
    // Better to use the binary if available. But cargo run is easiest for dev/CI.

    let output = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--bin",
            "assay",
            "--",
            "run",
            "--config",
            golden_dir.join("eval.yaml").to_str().unwrap(),
            "--trace-file",
            golden_dir.join("trace.jsonl").to_str().unwrap(),
            "--db",
            ":memory:",
            "--strict",
            "--replay-strict",
        ])
        .output()
        .expect("Failed to execute assay command");

    assert!(
        output.status.success(),
        "Assay run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8(output.stderr).expect("Stderr not UTF-8");

    let golden_stderr = std::fs::read_to_string(golden_dir.join("stderr.golden"))
        .expect("Failed to read golden stderr");

    // Normalize: Remove variable timing info "(0.0s)" or similar if it changes
    // The golden file has "(0.0s)". If it changes to "(0.1s)", test fails.
    // Strip E7 seed line ("Info: No seed provided. Using generated seed: NNNN") so golden is stable.
    // Verify result (strip ANSI codes for comparison)
    let ansi_re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    let duration_re = regex::Regex::new(r"\(\d+\.\d+s\)").unwrap();
    let seed_re =
        regex::Regex::new(r"Info: No seed provided\. Using generated seed: \d+\n\n").unwrap();

    let stripped_actual = ansi_re.replace_all(&stderr, "").to_string();
    let without_seed = seed_re.replace_all(&stripped_actual, "").to_string();

    let normalized_actual = duration_re
        .replace_all(&without_seed, "(0.0s)")
        .replace("\r\n", "\n")
        .trim()
        .to_string();
    let normalized_expected = golden_stderr.replace("\r\n", "\n").trim().to_string();

    assert_eq!(
        normalized_actual, normalized_expected,
        "Golden output mismatch"
    );
}
