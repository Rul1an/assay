//! `assay mcp preflight` publishes one `assay.mcp_preflight.v0` document (#2195).

#[path = "../../../tests/support/bounded_process.rs"]
#[allow(dead_code)]
mod bounded_process;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use serde_json::Value;
use std::process::{Command, Output};

const PREFLIGHT_SCHEMA: &str = "assay.mcp_preflight.v0";

fn preflight(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    command.env("NO_COLOR", "1").env("PATH", "");
    command.args(["mcp", "preflight"]).args(args);
    run_bounded(command, b"", GOLDEN_PATH_LIMITS, "assay mcp preflight").expect("preflight ran")
}

fn exit_code(output: &Output) -> i32 {
    output
        .status
        .code()
        .expect("preflight exited by code rather than by signal")
}

#[test]
fn policy_root_dot_format_json_is_one_preflight_document() {
    let output = preflight(&["--policy-root", ".", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        exit_code(&output),
        2,
        "empty PATH must not be ready; stderr={stderr}"
    );
    let document: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!("stdout is not one JSON document: {error}\nstdout={stdout}\nstderr={stderr}")
    });
    assert_eq!(document["schema"], PREFLIGHT_SCHEMA);
    assert_eq!(document["phase"], "missing");
    assert_eq!(document["expected_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(document["policy_root"], ".");
    assert!(
        document.get("actual_version").is_none(),
        "missing must omit actual_version: {document}"
    );
    let mut keys: Vec<_> = document
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "expected_version",
            "message",
            "next_step",
            "phase",
            "policy_root",
            "schema",
        ]
    );
    assert_eq!(
        document["message"],
        "assay-mcp-server was not found on PATH"
    );
    assert_eq!(
        document["next_step"],
        format!(
            "Install assay-mcp-server on PATH (cargo install assay-mcp-server --version {} --locked), then re-run assay mcp preflight.",
            env!("CARGO_PKG_VERSION")
        )
    );
}

#[test]
fn format_terminal_is_not_json() {
    let output = preflight(&["--format", "terminal"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(exit_code(&output), 2);
    assert!(
        stdout.starts_with("missing:"),
        "terminal format must stay human; stdout={stdout}"
    );
    assert!(serde_json::from_slice::<Value>(&output.stdout).is_err());
}
