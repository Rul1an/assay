#![cfg(feature = "runner")]

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use assay_runner_core::RunnerSpikeArchive;
use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use sha2::{Digest, Sha256};
use std::process::Command;

#[test]
fn verifies_runner_archive_and_refuses_a_changed_external_expectation() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("runner.tar.gz");
    let mut bytes = Vec::new();
    RunnerSpikeArchive::empty("cli_archive_test", "linux")
        .write(&mut bytes)
        .unwrap();
    std::fs::write(&archive, &bytes).unwrap();
    let digest = hex::encode(Sha256::digest(&bytes));
    let run = |expected: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
        command
            .args(["runner-spike", "verify"])
            .arg(&archive)
            .args(["--sha256", expected, "--run-id", "cli_archive_test"]);
        run_bounded(
            command,
            b"",
            GOLDEN_PATH_LIMITS,
            "runner archive verification",
        )
        .unwrap()
    };
    let accepted = run(&digest);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(output["archive_integrity"], "verified");
    assert_eq!(output["layer_semantics"], "not_verified");
    assert!(!run(&"0".repeat(64)).status.success());
    assert_eq!(std::fs::read(&archive).unwrap(), bytes);
}
