#![allow(deprecated)]

use assert_cmd::Command;
#[cfg(target_os = "linux")]
use std::io::Write;
#[cfg(target_os = "linux")]
use tempfile::{NamedTempFile, TempDir};

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

#[cfg(target_os = "linux")]
fn ipv6_runtime_policy() -> NamedTempFile {
    let mut policy = NamedTempFile::new().expect("temp policy");
    policy
        .write_all(
            br#"runtime_monitor:
  enabled: true
  provider: "ebpf"
  rules:
    - id: "deny-ipv6"
      type: "net_connect"
      match:
        dest_globs: ["2001:db8::/32"]
      action: "deny"
"#,
        )
        .expect("write IPv6 policy");
    policy
}

#[cfg(target_os = "linux")]
fn ipv4_runtime_policy() -> NamedTempFile {
    let mut policy = NamedTempFile::new().expect("temp policy");
    policy
        .write_all(
            br#"runtime_monitor:
  enabled: true
  provider: "ebpf"
  rules:
    - id: "deny-ipv4"
      type: "net_connect"
      match:
        dest_globs: ["198.51.100.0/24"]
      action: "deny"
"#,
        )
        .expect("write IPv4 policy");
    policy
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_missing_ebpf_retains_failed_health_for_requested_enforcement() {
    let policy = ipv4_runtime_policy();
    let output_dir = TempDir::new().expect("temp output dir");
    let health_path = output_dir.path().join("enforcement-health.json");

    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.arg("monitor")
        .arg("--policy")
        .arg(policy.path())
        .arg("--ebpf")
        .arg("/definitely/missing/assay-ebpf.o")
        .arg("--enforcement-health")
        .arg(&health_path)
        .assert()
        .code(40);

    let health: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&health_path).expect("read retained enforcement health"),
    )
    .expect("parse retained enforcement health");
    assert_eq!(
        health["network_enforcement"], "failed",
        "requested enforcement that cannot start must never read as absent"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_missing_ebpf_returns_infra_error_when_failed_health_cannot_be_written() {
    let policy = ipv4_runtime_policy();
    let unwritable_target = TempDir::new().expect("directory cannot be overwritten as a file");

    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    let assert = cmd
        .arg("monitor")
        .arg("--policy")
        .arg(policy.path())
        .arg("--ebpf")
        .arg("/definitely/missing/assay-ebpf.o")
        .arg("--enforcement-health")
        .arg(unwritable_target.path())
        .assert()
        .code(3);

    let stderr = normalize(&assert.get_output().stderr);
    assert!(
        stderr.contains("failed to write enforcement_health artifact"),
        "artifact-write failure diagnostic changed unexpectedly: {stderr}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_missing_ebpf_retains_absent_health_without_network_enforcement() {
    let output_dir = TempDir::new().expect("temp output dir");
    let health_path = output_dir.path().join("enforcement-health.json");

    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    cmd.arg("monitor")
        .arg("--ebpf")
        .arg("/definitely/missing/assay-ebpf.o")
        .arg("--enforcement-health")
        .arg(&health_path)
        .assert()
        .code(40);

    let health: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&health_path).expect("read retained enforcement health"),
    )
    .expect("parse retained enforcement health");
    assert_eq!(
        health["network_enforcement"], "absent",
        "a startup failure without requested network enforcement must not claim failure"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_missing_ebpf_returns_infra_error_when_absent_health_cannot_be_written() {
    let unwritable_target = TempDir::new().expect("directory cannot be overwritten as a file");

    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    let assert = cmd
        .arg("monitor")
        .arg("--ebpf")
        .arg("/definitely/missing/assay-ebpf.o")
        .arg("--enforcement-health")
        .arg(unwritable_target.path())
        .assert()
        .code(3);

    let stderr = normalize(&assert.get_output().stderr);
    assert!(
        stderr.contains("failed to write enforcement_health artifact"),
        "artifact-write failure diagnostic changed unexpectedly: {stderr}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_ipv6_refusal_precedes_ebpf_load_and_writes_failed_health() {
    let policy = ipv6_runtime_policy();
    let output_dir = TempDir::new().expect("temp output dir");
    let health_path = output_dir.path().join("enforcement-health.json");

    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    let assert = cmd
        .arg("monitor")
        .arg("--policy")
        .arg(policy.path())
        .arg("--ebpf")
        .arg("/definitely/missing/assay-ebpf.o")
        .arg("--enforcement-health")
        .arg(&health_path)
        .assert()
        .code(4);

    let stderr = normalize(&assert.get_output().stderr);
    assert!(
        stderr.contains("current enforcement target supports IPv4/TCP only"),
        "IPv6 refusal diagnostic changed unexpectedly: {stderr}"
    );
    assert!(
        health_path.is_file(),
        "fail-closed refusal must retain the requested health artifact"
    );
    let health: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&health_path).expect("read retained enforcement health"),
    )
    .expect("parse retained enforcement health");
    assert_eq!(
        health["network_enforcement"], "failed",
        "unsupported policy must retain failed, never absent"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn contract_monitor_ipv6_refusal_returns_infra_error_when_health_write_fails() {
    let policy = ipv6_runtime_policy();
    let unwritable_target = TempDir::new().expect("directory cannot be overwritten as a file");

    let mut cmd = Command::cargo_bin("assay").expect("assay binary");
    let assert = cmd
        .arg("monitor")
        .arg("--policy")
        .arg(policy.path())
        .arg("--ebpf")
        .arg("/definitely/missing/assay-ebpf.o")
        .arg("--enforcement-health")
        .arg(unwritable_target.path())
        .assert()
        .code(3);

    let stderr = normalize(&assert.get_output().stderr);
    assert!(
        stderr.contains("failed to write enforcement_health artifact"),
        "artifact-write failure diagnostic changed unexpectedly: {stderr}"
    );
}
