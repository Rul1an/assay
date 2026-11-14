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

fn refused_key(stderr: &str, path: &str) -> bool {
    stderr.contains(&format!(
        "assay monitor will refuse this policy: \"{path}\": assay monitor does not act on this key"
    ))
}

/// Every key under `runtime_monitor` and `kill_switch` is acted on by the monitor or named here.
///
/// These keys load as part of the policy schema, and `policy resolve` printed defaults for them
/// (`kill_children: true`, `kill_scope: cgroup`, `freeze_on_incident: true`), while the monitor
/// signalled the one process that matched. A misspelt key loaded the same way.
#[test]
fn a_key_the_monitor_does_not_act_on_is_named_with_its_path() {
    for (kill_switch, path) in [
        ("  kill_children: true\n", "kill_switch.kill_children"),
        ("  kill_scope: cgroup\n", "kill_switch.kill_scope"),
        ("  capture_state: true\n", "kill_switch.capture_state"),
        ("  output_dir: /tmp/incidents\n", "kill_switch.output_dir"),
        ("  capture_stat: true\n", "kill_switch.capture_stat"),
    ] {
        let (code, stderr) = validate(&format!(
            "kill_switch:\n  enabled: true\n{kill_switch}{}",
            monitor(FILE_DENY)
        ));
        assert_eq!(code, Some(0), "valid for other readers: {stderr}");
        assert!(refused_key(&stderr, path), "{path} not named: {stderr}");
    }

    let (_, stderr) = validate(
        "runtime_monitor:\n  enabled: true\n  provider: ebpf\n  scope:\n    mode: cgroup_v2\n  rules:\n    - id: file-deny\n      type: file_open\n      match:\n        path_globs: ['/etc/secret/**']\n      severity: critical\n      action: deny\n",
    );
    for path in [
        "runtime_monitor.provider",
        "runtime_monitor.scope",
        "runtime_monitor.rules.0.severity",
    ] {
        assert!(refused_key(&stderr, path), "{path} not named: {stderr}");
    }
}

/// A key that only another rule type reads, or a trigger on a rule that never kills, has no effect.
#[test]
fn a_value_the_monitor_does_not_act_on_is_named() {
    let (_, file_with_dest) = validate(&monitor(
        "    - id: file-deny\n      type: file_open\n      match:\n        path_globs: ['/etc/secret/**']\n        dest_globs: ['10.0.0.0/8']\n      action: deny\n",
    ));
    assert!(
        file_with_dest.contains("rule 'file-deny': dest_globs has no effect on a file_open rule"),
        "stderr: {file_with_dest}"
    );

    let (_, net_with_not) = validate(&monitor(
        "    - id: net-deny\n      type: net_connect\n      match:\n        dest_globs: ['10.0.0.0/8']\n        not:\n          dest_globs: ['10.1.0.0/16']\n      action: deny\n",
    ));
    assert!(
        net_with_not.contains("rule 'net-deny': not has no effect on a net_connect rule"),
        "stderr: {net_with_not}"
    );

    let (_, trigger_on_deny) = validate(&format!(
        "kill_switch:\n  enabled: true\n  triggers:\n    - on_rule: file-deny\n{}",
        monitor(FILE_DENY)
    ));
    assert!(
        trigger_on_deny.contains(
            "kill_switch.triggers: rule 'file-deny' does not use trigger_kill, so this trigger has no effect"
        ),
        "stderr: {trigger_on_deny}"
    );
}

/// Controls for the two tests above: the keys the monitor acts on raise nothing, and a disabled
/// `runtime_monitor` is not read, whatever it and the kill switch hold.
#[test]
fn the_keys_the_monitor_acts_on_raise_nothing() {
    let (_, stderr) = validate(&format!(
        "kill_switch:\n  enabled: true\n  mode: immediate\n  grace_period_ms: 500\n  triggers:\n    - on_rule: exec-kill\n      mode: graceful\n{}",
        monitor(&format!("{EXEC_KILL}{FILE_DENY}"))
    ));
    assert!(!stderr.contains("will refuse"), "stderr: {stderr}");

    let (_, disabled) = validate(
        "kill_switch:\n  enabled: true\n  kill_children: true\nruntime_monitor:\n  enabled: false\n  provider: ebpf\n",
    );
    assert!(!disabled.contains("will refuse"), "stderr: {disabled}");
}

/// A glob that does not compile is quoted in the refusal `policy validate` prints. Whatever the
/// policy wrote, stderr holds no control byte but the newline that ends a line, no C1 control, and
/// no line the policy wrote for itself.
#[test]
fn a_refusal_cannot_write_to_the_terminal_through_the_policy() {
    let rule = "    - id: bad-glob\n      type: file_open\n      match:\n        path_globs: [\"/etc/[\\e]52;c;cHduZWQ=\\a\\e[31m\\u009b31m\\nFATAL: forged\"]\n      action: deny\n";
    let (code, stderr) = validate(&monitor(rule));
    assert_eq!(code, Some(0), "stderr: {stderr}");
    assert!(
        stderr.contains("rule 'bad-glob': path_globs do not compile"),
        "the refusal did not quote the glob: {stderr}"
    );
    for (index, byte) in stderr.bytes().enumerate() {
        assert!(
            (byte >= 0x20 && byte != 0x7f) || byte == b'\n',
            "byte {index} is the control byte {byte:#04x}:\n{stderr}"
        );
    }
    assert!(
        !stderr.chars().any(|c| ('\u{80}'..='\u{9f}').contains(&c)),
        "a C1 control reached the terminal:\n{stderr}"
    );
    assert!(
        !stderr.lines().any(|line| line.starts_with("FATAL: forged")),
        "the policy wrote a line of its own:\n{stderr}"
    );
}
