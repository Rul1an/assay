//! `assay monitor` refuses a `runtime_monitor` rule it cannot act on before it loads anything.
//!
//! Linux only: on other platforms the command stops at "only supported on Linux" before it reads
//! a policy. The rule table itself is tested on every platform in `monitor_next/rules.rs`; this
//! pins that the command asks it, and asks before the eBPF object is needed.

#![cfg(target_os = "linux")]

use assert_cmd::Command;
use tempfile::tempdir;

/// A policy with the given rules and an enabled kill switch.
fn monitor_with(rules: &str) -> std::process::Output {
    monitor_with_policy(&format!(
        "kill_switch:\n  enabled: true\n{}",
        monitor_section(rules)
    ))
}

fn monitor_section(rules: &str) -> String {
    format!("runtime_monitor:\n  enabled: true\n  rules:\n{rules}")
}

fn monitor_with_policy(body: &str) -> std::process::Output {
    let dir = tempdir().expect("tempdir");
    let policy = dir.path().join("policy.yaml");
    std::fs::write(&policy, format!("version: \"2.0\"\nname: probe\n{body}"))
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

/// The same kill rule without a kill switch would log and stop nothing, so it is refused.
#[test]
fn a_proc_exec_kill_rule_without_a_kill_switch_is_refused() {
    let output = monitor_with_policy(&monitor_section(
        "    - id: exec-kill\n      type: proc_exec\n      match:\n        path_globs: ['**/curl']\n      action: trigger_kill\n",
    ));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("rule 'exec-kill': trigger_kill on proc_exec needs an enabled kill_switch"),
        "stderr: {stderr}"
    );
}

/// A kill switch that is present but switched off kills nothing either.
#[test]
fn a_proc_exec_kill_rule_with_the_kill_switch_switched_off_is_refused() {
    let output = monitor_with_policy(&format!(
        "kill_switch:\n  enabled: false\n{}",
        monitor_section(
            "    - id: exec-kill\n      type: proc_exec\n      match:\n        path_globs: ['**/curl']\n      action: trigger_kill\n",
        )
    ));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("rule 'exec-kill': trigger_kill on proc_exec needs an enabled kill_switch"),
        "stderr: {stderr}"
    );
}

/// A kill switch key the monitor does not act on is refused by its path, before anything loads.
/// `kill_children: true` used to load, and `policy resolve` printed it, while the monitor
/// signalled the one process that matched.
#[test]
fn a_kill_switch_key_the_monitor_does_not_act_on_is_refused_before_anything_loads() {
    let output = monitor_with_policy(&format!(
        "kill_switch:\n  enabled: true\n  kill_children: true\n{}",
        monitor_section(
            "    - id: exec-kill\n      type: proc_exec\n      match:\n        path_globs: ['**/curl']\n      action: trigger_kill\n",
        )
    ));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("\"kill_switch.kill_children\": assay monitor does not act on this key"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("fail-closed"), "stderr: {stderr}");
    assert!(
        !stderr.contains("eBPF object not found"),
        "stderr: {stderr}"
    );
}
