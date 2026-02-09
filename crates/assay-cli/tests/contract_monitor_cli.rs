#![allow(deprecated)]

use assert_cmd::Command;
#[cfg(target_os = "linux")]
use std::io::Write;
#[cfg(target_os = "linux")]
use tempfile::NamedTempFile;

fn normalize(s: &[u8]) -> String {
    String::from_utf8_lossy(s).replace("\r\n", "\n")
}

#[cfg(not(target_os = "linux"))]
#[test]
fn contract_monitor_non_linux_exit_40_not_supported() {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    let assert = cmd.arg("monitor").assert().code(40);

    let stderr = normalize(&assert.get_output().stderr);
    assert!(
        stderr.contains("only supported on Linux"),
        "platform-gate diagnostic line changed unexpectedly: {stderr}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_missing_ebpf_path_exit_40_not_found() {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    let assert = cmd
        .arg("monitor")
        .arg("--ebpf")
        .arg("/definitely/missing/assay-ebpf.o")
        .assert()
        .code(40);

    let stderr = normalize(&assert.get_output().stderr);
    assert!(
        stderr.contains("eBPF object not found"),
        "missing-ebpf diagnostic line changed unexpectedly: {stderr}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_invalid_ebpf_payload_exit_40_load_fail() {
    let mut invalid_ebpf = NamedTempFile::new().expect("temp ebpf");
    invalid_ebpf
        .write_all(b"this-is-not-a-valid-ebpf-object")
        .expect("write invalid ebpf");

    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    let assert = cmd
        .arg("monitor")
        .arg("--ebpf")
        .arg(invalid_ebpf.path())
        .assert()
        .code(40);

    let stderr = normalize(&assert.get_output().stderr);
    assert!(
        stderr.contains("Failed to load eBPF"),
        "invalid-ebpf diagnostic line changed unexpectedly: {stderr}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_parse_fail_policy_exit_2() {
    let mut invalid_policy = NamedTempFile::new().expect("temp policy");
    invalid_policy
        .write_all(b"version: [invalid")
        .expect("write invalid policy");

    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    let assert = cmd
        .arg("monitor")
        .arg("--policy")
        .arg(invalid_policy.path())
        .assert()
        .code(2);

    let stderr = normalize(&assert.get_output().stderr).to_lowercase();
    assert!(
        stderr.contains("fatal:")
            || stderr.contains("yaml")
            || stderr.contains("expected")
            || stderr.contains("line")
            || stderr.contains("column"),
        "parse-fail diagnostic line changed unexpectedly: {stderr}"
    );
}
