use std::path::Path;
use std::process::Command;

#[test]
fn test_golden_harness() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let golden_dir = Path::new(manifest_dir).join("../../tests/fixtures/golden"); // Go up to workspace root

    // Build binary first usually, but for test we can use cargo run?
    // Better to use the binary if available. But cargo run is easiest for dev/CI.

    let output = Command::new("cargo")
        .args(&[
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

    // Normalize: Remove variable timing info "(0.0s)" or similar if it changes
    // The golden file has "(0.0s)". If it changes to "(0.1s)", test fails.
    // For now, assume it's fast enough to be 0.0s or stable.
    // We can also strip lines starting with "Finished" or "Running" if --quiet doesn't catch them.

    let golden_stderr = std::fs::read_to_string(golden_dir.join("stderr.golden"))
        .expect("Failed to read golden stderr");

    // Simple normalization: Trim whitespace
    let normalized_actual = stderr.trim();
    let normalized_expected = golden_stderr.trim();

    assert_eq!(
        normalized_actual, normalized_expected,
        "Golden output mismatch"
    );
}
