//! `assay monitor` refuses a `runtime_monitor` rule it cannot act on before it loads anything.
//!
//! Linux only: on other platforms the command stops at "only supported on Linux" before it reads
//! a policy. The rule table itself is tested on every platform in `monitor_next/rules.rs`; this
//! pins that the command asks it, and asks before the eBPF object is needed.

#![cfg(target_os = "linux")]

use assert_cmd::Command;
use tempfile::tempdir;

fn monitor_with(rules: &str) -> std::process::Output {
    let dir = tempdir().expect("tempdir");
    let policy = dir.path().join("policy.yaml");
    std::fs::write(
        &policy,
        format!(
            "version: \"2.0\"\nname: probe\nruntime_monitor:\n  enabled: true\n  rules:\n{rules}"
        ),
    )
    .expect("write policy");
    Command::cargo_bin("assay")
        .expect("assay binary")
        .args(["monitor", "--policy"])
        .arg(&policy)
        // No eBPF object exists at this path. A run that gets as far as needing it exits 40.
        .args(["--ebpf"])
        .arg(dir.path().join("absent.o"))
        .output()
        .expect("run assay monitor")
}

#[test]
fn a_proc_exec_deny_rule_is_refused_by_name_before_anything_loads() {
    let output = monitor_with(
        "    - id: exec-deny\n      type: proc_exec\n      match:\n        path_globs: ['**/curl']\n      action: deny\n",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("rule 'exec-deny': proc_exec cannot be denied"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("fail-closed"), "stderr: {stderr}");
}

/// Control: a rule the monitor acts on gets past the policy and stops where the eBPF object is
/// missing. Without this, a monitor that refused every policy would pass the test above.
#[test]
fn a_proc_exec_kill_rule_is_accepted_and_the_run_proceeds_to_loading() {
    let output = monitor_with(
        "    - id: exec-kill\n      type: proc_exec\n      match:\n        path_globs: ['**/curl']\n      action: trigger_kill\n",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(40), "stderr: {stderr}");
    assert!(stderr.contains("eBPF object not found"), "stderr: {stderr}");
    assert!(!stderr.contains("cannot act on"), "stderr: {stderr}");
}
