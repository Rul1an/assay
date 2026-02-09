#![allow(deprecated)]

use assert_cmd::Command;
use std::io::Write;
use tempfile::NamedTempFile;

fn normalize(s: &[u8]) -> String {
    String::from_utf8_lossy(s).replace("\r\n", "\n")
}

#[test]
fn contract_monitor_missing_ebpf_path_exit_40() {
    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    let assert = cmd
        .arg("monitor")
        .arg("--ebpf")
        .arg("/definitely/missing/assay-ebpf.o")
        .assert()
        .code(40);

    let stderr = normalize(&assert.get_output().stderr);
    assert!(
        stderr.contains("only supported on Linux") || stderr.contains("eBPF object not found"),
        "missing-ebpf diagnostic line changed unexpectedly: {stderr}"
    );
}

#[test]
fn contract_monitor_invalid_ebpf_payload_exit_40() {
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
        stderr.contains("only supported on Linux") || stderr.contains("Failed to load eBPF"),
        "invalid-ebpf diagnostic line changed unexpectedly: {stderr}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_parse_fail_policy_exit_1() {
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
        .code(1);

    let stderr = normalize(&assert.get_output().stderr).to_lowercase();
    assert!(
        stderr.contains("error") || stderr.contains("yaml") || stderr.contains("policy"),
        "parse-fail diagnostic line changed unexpectedly: {stderr}"
    );
}
