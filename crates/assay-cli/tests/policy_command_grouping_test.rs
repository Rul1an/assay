use std::process::Command;

fn assay_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_assay"))
}

#[test]
fn policy_group_exposes_authoring_commands_and_legacy_flat_commands_are_hidden() {
    let top_help = assay_cmd().arg("--help").output().unwrap();
    assert!(top_help.status.success());
    let top_stdout = String::from_utf8_lossy(&top_help.stdout);
    assert!(top_stdout.contains("  policy"));
    assert!(!top_stdout.contains("  generate "));
    assert!(!top_stdout.contains("  record "));

    let policy_help = assay_cmd().args(["policy", "--help"]).output().unwrap();
    assert!(policy_help.status.success());
    let policy_stdout = String::from_utf8_lossy(&policy_help.stdout);
    assert!(policy_stdout.contains("  generate "));
    assert!(policy_stdout.contains("  record "));
    assert!(policy_stdout.contains("  validate "));
    assert!(policy_stdout.contains("  migrate "));
    assert!(policy_stdout.contains("  fmt "));
}

#[test]
fn legacy_flat_policy_generate_prints_deprecation_warning() {
    let output = assay_cmd().arg("generate").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`assay generate` is deprecated; use `assay policy generate` instead"),
        "missing legacy policy deprecation warning: {stderr}"
    );
}
