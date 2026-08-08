//! MCP-owned rows of the #1975 stdout-and-exit-code journey contract.

mod jsonrpc_conn;

use jsonrpc_conn::Conn;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_STDOUT_BYTES: u64 = 1024 * 1024;

fn workspace_root() -> &'static Path {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("assay-mcp-server must live two components below the workspace root");
    assert!(
        root.join("Cargo.toml").is_file(),
        "workspace root does not contain Cargo.toml: {}",
        root.display()
    );
    root
}

fn contract() -> Value {
    let path = workspace_root().join("docs/generated/agent-golden-path.json");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "read generated agent golden-path contract {}: {error}",
            path.display()
        )
    });
    let contract: Value = serde_json::from_str(&raw).expect("agent golden-path contract is JSON");
    assert_eq!(contract["schema"], "assay.agent_golden_path.v1");
    assert_eq!(contract["schema_version"], 1);
    contract
}

fn expected_outcome(step_id: &str, outcome_name: &str) -> Value {
    let contract = contract();
    let step = contract["steps"]
        .as_array()
        .expect("contract steps array")
        .iter()
        .find(|step| step["id"] == step_id)
        .unwrap_or_else(|| panic!("contract step {step_id:?} is missing"));
    let mut outcome = step["outcomes"]
        .as_array()
        .expect("step outcomes array")
        .iter()
        .find(|outcome| outcome["name"] == outcome_name)
        .unwrap_or_else(|| panic!("contract outcome {step_id}/{outcome_name} is missing"))
        .clone();
    outcome["command"] = step["command"].clone();
    outcome
}

fn assert_exit(output: &Output, expected: &Value, context: &str) {
    assert_eq!(
        output.status.code().map(i64::from),
        expected["exit_code"].as_i64(),
        "{context} exit differed; stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_gap(expected: &Value, issue: u64) {
    assert_eq!(expected["gap_issue"].as_u64(), Some(issue));
}

fn assert_command(expected: &Value, command: &str) {
    assert_eq!(
        expected["command"], command,
        "the driven invocation drifted from the machine contract"
    );
}

fn clean_server_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"));
    for (name, _) in std::env::vars_os() {
        if name
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("ASSAY_AUTH_")
        {
            command.env_remove(name);
        }
    }
    command
}

fn run_server(cwd: &Path, args: &[&str], stdin: &[u8]) -> Output {
    let mut command = clean_server_command();
    command
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = command.spawn().expect("spawn assay-mcp-server");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(stdin)
        .expect("write child stdin");
    wait_bounded(child)
}

fn wait_bounded(mut child: Child) -> Output {
    let stdout = child.stdout.take().expect("child stdout");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(MAX_STDOUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .expect("read bounded child stdout");
        bytes
    });
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    let status = loop {
        match child.try_wait().expect("poll assay-mcp-server") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let status = child.wait().expect("reap timed-out assay-mcp-server");
                panic!(
                    "assay-mcp-server did not exit within {PROCESS_TIMEOUT:?}; killed it ({status})"
                );
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let stdout = reader.join().expect("join stdout reader");
    assert!(
        stdout.len() <= MAX_STDOUT_BYTES as usize,
        "assay-mcp-server stdout exceeded the {MAX_STDOUT_BYTES}-byte test ceiling"
    );
    Output {
        status,
        stdout,
        stderr: Vec::new(),
    }
}

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

#[test]
fn enforcing_proxy_denial_is_structured_but_startup_failure_is_not() {
    let example = workspace_root().join("examples/privileged-action-gate");
    let mut command = clean_server_command();
    let child = command
        .current_dir(&example)
        .args([
            "proxy-enforce",
            "--upstream-command",
            python(),
            "--upstream-arg",
            "-u",
            "--upstream-arg",
            "mock_github_mcp.py",
            "--enforce-policy",
            "policies/no-allowance.yaml",
            "--declared-mcp-manifest",
            "baseline-approved.json",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn enforcing proxy");
    let mut connection = Conn::attach(child);
    let initialize = connection.request(
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "golden-path-contract", "version": "1"}
        }),
        1,
    );
    assert_eq!(initialize["jsonrpc"], "2.0");
    let denied = connection.request(
        "tools/call",
        serde_json::json!({
            "name": "github.add_deploy_key",
            "arguments": {"owner": "acme", "repo": "prod-app"}
        }),
        9,
    );
    let denied_expected = expected_outcome("protected-action", "policy-denied");
    assert_command(&denied_expected, "assay-mcp-server proxy-enforce <args>");
    assert_eq!(denied_expected["stdout"]["document"], "jsonrpc-2.0");
    assert_eq!(denied_expected["exit_code"], 0);
    assert_eq!(
        denied["error"]["code"],
        denied_expected["jsonrpc_error_code"]
    );
    assert_eq!(denied["error"]["data"]["origin"], denied_expected["origin"]);
    assert_eq!(denied["error"]["data"]["reason"], denied_expected["reason"]);
    let status = connection.shutdown();
    assert_eq!(
        status.code().map(i64::from),
        denied_expected["exit_code"].as_i64()
    );

    let startup_failure = run_server(
        &example,
        &[
            "proxy-enforce",
            "--upstream-command",
            python(),
            "--enforce-policy",
            "missing.yaml",
            "--declared-mcp-manifest",
            "missing.json",
        ],
        b"",
    );
    let startup_expected = expected_outcome("protected-action", "startup-failure");
    assert_exit(&startup_failure, &startup_expected, "proxy startup failure");
    assert!(startup_failure.stdout.is_empty());
    assert_gap(&startup_expected, 2163);
}

#[test]
fn sarif_projection_currently_turns_malformed_input_into_clean_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let deny = serde_json::json!({
        "schema": "assay.enforcement_decision.v0",
        "tool": {
            "name": "github.add_deploy_key",
            "action_class": "github_deploy_key"
        },
        "decision": "deny",
        "reason": "no_declared_allowance",
        "fail_closed": true,
        "drift_state": "not_evaluated"
    });
    let valid_input = format!("{deny}\n");
    let success = run_server(
        dir.path(),
        &["enforcement-sarif", "--input", "-", "--output", "-"],
        valid_input.as_bytes(),
    );
    let expected_success = expected_outcome("sarif-projection", "valid");
    assert_command(
        &expected_success,
        "assay-mcp-server enforcement-sarif --input <decisions.ndjson> --output -",
    );
    assert_exit(&success, &expected_success, "SARIF projection success");
    let success_json: Value = serde_json::from_slice(&success.stdout).expect("valid SARIF stdout");
    assert_eq!(
        format!("sarif-{}", success_json["version"].as_str().unwrap()),
        expected_success["stdout"]["document"]
    );
    assert_eq!(
        success_json["runs"][0]["results"].as_array().unwrap().len(),
        1
    );

    let malformed = run_server(
        dir.path(),
        &["enforcement-sarif", "--input", "-", "--output", "-"],
        b"not-json\n",
    );
    let expected_malformed = expected_outcome("sarif-projection", "malformed");
    assert_exit(&malformed, &expected_malformed, "SARIF malformed input");
    let malformed_json: Value =
        serde_json::from_slice(&malformed.stdout).expect("malformed-input SARIF stdout");
    assert_eq!(
        format!("sarif-{}", malformed_json["version"].as_str().unwrap()),
        expected_malformed["stdout"]["document"]
    );
    assert!(malformed_json["runs"][0]["results"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_gap(&expected_malformed, 2166);
}
