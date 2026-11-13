//! `assay policy validate` says when `assay monitor` will refuse the policy.
//!
//! A `runtime_monitor` rule the monitor cannot act on makes `assay monitor` exit 2 at start, on
//! Linux, at run time. `policy validate` runs everywhere and before that, and it used to print
//! "Policy OK" for such a policy without a word. It still exits 0, because the policy is valid for
//! every other reader (`runtime_monitor` has no effect under `assay mcp wrap`), but it names each
//! rule and the reason, from the same function the monitor asks.

use assert_cmd::Command;
use tempfile::tempdir;

fn validate(body: &str) -> (Option<i32>, String) {
    let dir = tempdir().expect("tempdir");
    let policy = dir.path().join("policy.yaml");
    std::fs::write(&policy, format!("version: \"2.0\"\nname: probe\n{body}")).expect("write");
    let output = Command::cargo_bin("assay")
        .expect("assay binary")
        .args(["policy", "validate", "--input"])
        .arg(&policy)
        .output()
        .expect("run assay policy validate");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const EXEC_DENY: &str = "    - id: exec-deny\n      type: proc_exec\n      match:\n        path_globs: ['**/curl']\n      action: deny\n";
const EXEC_KILL: &str = "    - id: exec-kill\n      type: proc_exec\n      match:\n        path_globs: ['**/curl']\n      action: trigger_kill\n";
const FILE_DENY: &str = "    - id: file-deny\n      type: file_open\n      match:\n        path_globs: ['/etc/secret/**']\n      action: deny\n";

fn monitor(rules: &str) -> String {
    format!("runtime_monitor:\n  enabled: true\n  rules:\n{rules}")
}

#[test]
fn a_rule_the_monitor_will_refuse_is_named() {
    let (code, stderr) = validate(&monitor(&format!("{EXEC_DENY}{FILE_DENY}")));
    assert_eq!(
        code,
        Some(0),
        "the policy is valid for other readers: {stderr}"
    );
    assert!(
        stderr.contains("assay monitor will refuse this policy")
            && stderr.contains("rule 'exec-deny': proc_exec cannot be denied"),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("file-deny"),
        "an accepted rule was named: {stderr}"
    );
}

#[test]
fn the_kill_switch_is_read_as_the_monitor_reads_it() {
    let (_, without) = validate(&monitor(EXEC_KILL));
    assert!(
        without
            .contains("rule 'exec-kill': trigger_kill on proc_exec needs an enabled kill_switch"),
        "stderr: {without}"
    );
    let (_, with) = validate(&format!(
        "kill_switch:\n  enabled: true\n{}",
        monitor(EXEC_KILL)
    ));
    assert!(!with.contains("will refuse"), "stderr: {with}");
    let (_, off) = validate(&format!(
        "kill_switch:\n  enabled: false\n{}",
        monitor(EXEC_KILL)
    ));
    assert!(off.contains("will refuse"), "stderr: {off}");
}

/// Controls: nothing is said when the monitor would start, or would not read the rules at all.
#[test]
fn a_policy_the_monitor_accepts_or_does_not_read_gets_no_warning() {
    let (code, stderr) = validate(&monitor(FILE_DENY));
    assert_eq!(code, Some(0));
    assert!(!stderr.contains("will refuse"), "stderr: {stderr}");

    // `enabled: false`: the monitor exits before it reads a rule.
    let (_, disabled) = validate(&format!(
        "runtime_monitor:\n  enabled: false\n  rules:\n{EXEC_DENY}"
    ));
    assert!(!disabled.contains("will refuse"), "stderr: {disabled}");

    let (_, none) = validate("tools:\n  allow: ['read_file']\n");
    assert!(!none.contains("will refuse"), "stderr: {none}");
}
