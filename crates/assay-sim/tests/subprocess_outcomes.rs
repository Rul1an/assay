#![cfg(unix)]

use assay_sim::subprocess::subprocess_verify;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::{Duration, Instant};

// Re-execute this test alone so ASSAY_BIN never mutates the parallel parent environment.
#[test]
#[ignore = "invoked in an isolated process by fixture tests"]
fn fixture_probe() {
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { subprocess_verify(b"fixture", Duration::from_millis(500)) })
        .unwrap();
    std::fs::write(
        std::env::var_os("PROBE_RESULT").unwrap(),
        serde_json::to_vec(&serde_json::json!({
            "valid": result.valid, "exit_code": result.exit_code,
            "timed_out": result.timed_out, "stderr": result.stderr,
        }))
        .unwrap(),
    )
    .unwrap();
}

fn probe(body: &str) -> serde_json::Value {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("verifier");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s' \"$$\" > \"$PROBE_PID\"\n{body}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = dir.path().join("result.json");
    let pid_path = dir.path().join("pid");
    let stdin_path = dir.path().join("stdin");
    std::fs::write(&stdin_path, b"inherited-input\n").unwrap();
    let start = Instant::now();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fixture_probe", "--ignored", "--nocapture"])
        .env("ASSAY_BIN", &script)
        .env("PROBE_RESULT", &output)
        .env("PROBE_PID", &pid_path)
        .stdin(std::fs::File::open(stdin_path).unwrap())
        .status()
        .unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "subprocess deadline did not bound the probe"
    );
    assert!(status.success(), "fixture probe failed");
    #[cfg(target_os = "linux")]
    {
        let pid = std::fs::read_to_string(pid_path).unwrap();
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "direct child not reaped"
        );
    }
    serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap()
}

#[test]
fn normal_acceptance_and_rejection_are_preserved() {
    for code in [0, 1, 2, 7] {
        let result = probe(&format!("printf diagnostic >&2\nexit {code}"));
        assert_eq!(result["exit_code"], code);
        assert_eq!(result["valid"], code == 0);
        assert_eq!(result["timed_out"], false);
        assert_eq!(result["stderr"], "diagnostic");
    }
}

#[test]
fn stderr_flood_does_not_turn_a_completed_rejection_into_timeout() {
    let result = probe("head -c 1048576 /dev/zero >&2\nexit 2");
    assert_eq!(result["timed_out"], false);
    assert_eq!(result["exit_code"], 2);
    assert_eq!(result["stderr"].as_str().unwrap().len(), 4096);
}

#[test]
fn timeout_retains_the_diagnostic_prefix() {
    let result = probe("printf before-timeout >&2\nexec sleep 5");
    assert_eq!(result["timed_out"], true);
    assert_eq!(result["valid"], false);
    assert_eq!(result["stderr"], "before-timeout");
}

#[test]
fn stderr_with_invalid_utf8_is_not_silently_lost() {
    let result = probe("printf 'prefix\\377suffix' >&2\nexit 2");
    assert_eq!(result["stderr"], "prefix\u{fffd}suffix");
}

#[test]
fn stderr_cap_may_split_a_multibyte_character_without_panicking() {
    let result =
        probe("head -c 4095 /dev/zero | tr '\\000' x >&2\nprintf '\\303\\251' >&2\nexit 2");
    assert_eq!(result["exit_code"], 2);
    assert!(result["stderr"]
        .as_str()
        .unwrap()
        .starts_with(&"x".repeat(4095)));
}

#[test]
fn an_exited_child_with_a_descendant_holding_stderr_is_bounded() {
    let result = probe("sleep 2 &\nexit 0");
    assert_eq!(
        result["timed_out"], true,
        "pipe drain must share the deadline"
    );
}

#[test]
fn verifier_cannot_consume_the_harness_stdin() {
    let result = probe("if read line; then exit 7; else exit 2; fi");
    assert_eq!(result["exit_code"], 2);
}
