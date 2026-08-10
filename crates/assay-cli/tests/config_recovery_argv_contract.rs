//! Binary contract for argument-safe recovery from `E_CFG_PARSE` (#2200).

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, ProcessLimits};
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

const LIMITS: ProcessLimits = ProcessLimits::new(Duration::from_secs(5), 64 * 1024, 64 * 1024);

fn assay(cwd: &Path, args: &[&str], context: &str) -> Output {
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

#[test]
fn config_parse_recovery_argv_executes_without_a_shell() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_name = "cfg file;$(touch should-not-exist).yaml";
    std::fs::write(dir.path().join(config_name), "version: [\n").expect("write malformed config");

    let failed = assay(
        dir.path(),
        &["run", "--format", "json", "--config", config_name],
        "produce config recovery",
    );
    assert_eq!(
        failed.status.code(),
        Some(2),
        "malformed config must be a config error: {}",
        String::from_utf8_lossy(&failed.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&failed.stdout).expect("failure stdout must be JSON");
    assert_eq!(summary["reason_code"], "E_CFG_PARSE");

    let encoded = summary["next_step"]
        .as_str()
        .expect("config failure next_step")
        .strip_prefix("Run argv: ")
        .expect("config recovery must publish JSON argv");
    let recovery: Vec<String> =
        serde_json::from_str(encoded).expect("config recovery argv must parse");
    assert_eq!(
        recovery,
        ["assay", "doctor", "--config", config_name],
        "the hostile path must remain one argv element"
    );

    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    command
        .current_dir(dir.path())
        .env("NO_COLOR", "1")
        .args(&recovery[1..]);
    let recovered = run_bounded(command, b"", LIMITS, "execute config recovery")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(recovered.status.code(), Some(1));
    let recovered_stdout = String::from_utf8_lossy(&recovered.stdout);
    assert!(
        recovered_stdout.contains("Config Status: FAILED"),
        "published argv must reach doctor: {recovered_stdout}"
    );
    assert!(!recovered_stdout.contains("Usage:"));
    assert!(
        !dir.path().join("should-not-exist").exists(),
        "recovery must not execute shell metacharacters"
    );
}
