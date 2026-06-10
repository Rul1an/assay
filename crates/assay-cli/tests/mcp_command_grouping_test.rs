use std::process::Command;

fn assay_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_assay"))
}

#[test]
fn mcp_group_is_visible_and_legacy_flat_commands_are_hidden() {
    let top_help = assay_cmd().arg("--help").output().unwrap();
    assert!(top_help.status.success());
    let top_stdout = String::from_utf8_lossy(&top_help.stdout);
    assert!(top_stdout.contains("  mcp"));
    assert!(!top_stdout.contains("  discover "));
    assert!(!top_stdout.contains("  kill "));
    assert!(!top_stdout.contains("  tool "));

    let mcp_help = assay_cmd().args(["mcp", "--help"]).output().unwrap();
    assert!(mcp_help.status.success());
    let mcp_stdout = String::from_utf8_lossy(&mcp_help.stdout);
    assert!(mcp_stdout.contains("  discover "));
    assert!(mcp_stdout.contains("  kill "));
    assert!(mcp_stdout.contains("  tool "));
}

#[test]
fn retired_flat_mcp_command_is_rejected() {
    // `assay kill` was a deprecated shim for `assay mcp kill`; the shim is retired, so the flat
    // path is now rejected as an unrecognized subcommand rather than warning and running.
    let output = assay_cmd().arg("kill").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrecognized subcommand"),
        "expected the retired flat `kill` shim to be rejected as an unrecognized subcommand: {stderr}"
    );
}
