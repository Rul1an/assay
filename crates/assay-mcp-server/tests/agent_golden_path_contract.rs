//! MCP-owned rows of the #1975 stdout-and-exit-code journey contract.

#[path = "../../../tests/support/bounded_process.rs"]
mod bounded_process;
mod jsonrpc_conn;
#[path = "../../../tests/support/agent_golden_path.rs"]
mod runtime_coverage;

use bounded_process::{run_bounded, GOLDEN_PATH_LIMITS};
use jsonrpc_conn::Conn;
use serde_json::Value;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use runtime_coverage::ExpectedOutcome;

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

fn expected_outcome(step_id: &str, outcome_name: &str) -> ExpectedOutcome {
    runtime_coverage::expected_outcome(&contract(), step_id, outcome_name)
}

fn assert_exit(output: &Output, expected: &ExpectedOutcome, context: &str) {
    let expected_exit = expected["exit_code"]
        .as_i64()
        .expect("contract exit_code must be an integer");
    let actual_exit = output
        .status
        .code()
        .map(i64::from)
        .expect("assay-mcp-server terminated without an exit code");
    assert_eq!(
        actual_exit,
        expected_exit,
        "{context} exit differed; stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    runtime_coverage::record_observation(expected);
}

fn assert_gap(expected: &Value, issue: u64) {
    assert_eq!(expected["gap_issue"].as_u64(), Some(issue));
}

fn contract_argv(expected: &Value, replacements: &[(&str, &str)]) -> Vec<String> {
    assert_eq!(expected["binary"], "assay-mcp-server");
    let argv = expected["argv"]
        .as_array()
        .expect("contract outcome argv array");
    for (placeholder, _) in replacements {
        assert!(
            argv.iter().any(|argument| argument == *placeholder),
            "replacement {placeholder:?} is not present in contract argv"
        );
    }
    argv.iter()
        .map(|argument| {
            let argument = argument.as_str().expect("contract argv string");
            if argument.starts_with('<') && argument.ends_with('>') {
                replacements
                    .iter()
                    .find_map(|(placeholder, value)| (*placeholder == argument).then_some(*value))
                    .unwrap_or_else(|| {
                        panic!("contract argv placeholder {argument:?} is unresolved")
                    })
                    .to_string()
            } else {
                argument.to_string()
            }
        })
        .collect()
}

fn assert_stdout_contract(expected: &Value, kind: &str, document: Option<&str>) {
    assert_eq!(
        expected["stdout"]["kind"], kind,
        "the observed stdout path drifted from the contract kind"
    );
    match document {
        Some(document) => assert_eq!(expected["stdout"]["document"], document),
        None => assert!(expected["stdout"]["document"].is_null()),
    }
}

fn assert_empty_stdout(output: &Output, expected: &Value, context: &str) {
    assert_stdout_contract(expected, "empty", None);
    assert!(output.stdout.is_empty(), "{context} stdout is not empty");
}

fn stdout_json(output: &Output, expected: &Value, context: &str) -> Value {
    assert_eq!(
        expected["stdout"]["kind"], "json",
        "the observed stdout path drifted from the contract kind"
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{context} stdout is not JSON: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
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

fn run_server<S: AsRef<OsStr>>(cwd: &Path, args: &[S], stdin: &[u8]) -> Output {
    let mut command = clean_server_command();
    command.current_dir(cwd).args(args);
    run_bounded(
        command,
        stdin,
        GOLDEN_PATH_LIMITS,
        "agent golden-path MCP command",
    )
    .unwrap_or_else(|error| panic!("{error}"))
}

fn python() -> &'static str {
    if cfg!(windows) {
        "python"
    } else {
        "python3"
    }
}

fn required_python() -> &'static str {
    let interpreter = python();
    let mut command = Command::new(interpreter);
    command.arg("--version");
    let output = run_bounded(
        command,
        b"",
        GOLDEN_PATH_LIMITS,
        "protected-action Python preflight",
    )
    .unwrap_or_else(|error| {
        panic!("the protected-action reference fixture requires {interpreter} on PATH: {error}")
    });
    assert!(
        output.status.success(),
        "the protected-action reference fixture requires a working {interpreter}"
    );
    interpreter
}

fn contract_working_directory(expected: &Value) -> PathBuf {
    match runtime_coverage::classify_working_directory(expected)
        .expect("protected-action contract working directory must be safe")
    {
        runtime_coverage::WorkingDirectory::Invocation => {
            panic!("protected-action contract must pin its working directory")
        }
        runtime_coverage::WorkingDirectory::SourceRepoRelative(components) => components
            .iter()
            .fold(workspace_root().to_path_buf(), |path, component| {
                path.join(component)
            }),
    }
}

#[test]
fn protected_action_contract_cwd_is_source_repo_relative_and_materialized() {
    let expected = expected_outcome("protected-action", "policy-denied");
    let cwd = contract_working_directory(&expected);
    for fixture in [
        "mock_github_mcp.py",
        "policies/no-allowance.yaml",
        "baseline-approved.json",
    ] {
        assert!(
            cwd.join(fixture).is_file(),
            "contract cwd does not contain argv fixture {fixture}"
        );
    }
}

#[test]
fn enforcing_proxy_denial_is_structured_but_startup_failure_is_not() {
    let python = required_python();
    let denied_expected = expected_outcome("protected-action", "policy-denied");
    let example = contract_working_directory(&denied_expected);
    let denied_argv = contract_argv(&denied_expected, &[("<python>", python)]);
    let mut command = clean_server_command();
    let child = command
        .current_dir(&example)
        .args(&denied_argv)
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
    assert!(initialize.get("result").is_some());
    assert!(initialize.get("error").is_none());
    let denied = connection.request(
        "tools/call",
        serde_json::json!({
            "name": "github.add_deploy_key",
            "arguments": {"owner": "acme", "repo": "prod-app"}
        }),
        9,
    );
    assert_stdout_contract(&denied_expected, "json_lines", Some("jsonrpc-2.0"));
    assert_eq!(
        format!(
            "jsonrpc-{}",
            denied["jsonrpc"].as_str().expect("JSON-RPC version string")
        ),
        denied_expected["stdout"]["document"]
    );
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
    runtime_coverage::record_observation(&denied_expected);

    let startup_expected = expected_outcome("protected-action", "startup-failure");
    let startup_argv = contract_argv(&startup_expected, &[("<python>", python)]);
    let startup_failure = run_server(&example, &startup_argv, b"");
    assert_exit(&startup_failure, &startup_expected, "proxy startup failure");
    assert_empty_stdout(&startup_failure, &startup_expected, "proxy startup failure");
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
    let expected_success = expected_outcome("sarif-projection", "valid");
    let success_argv = contract_argv(&expected_success, &[]);
    let success = run_server(dir.path(), &success_argv, valid_input.as_bytes());
    assert_exit(&success, &expected_success, "SARIF projection success");
    let success_json = stdout_json(&success, &expected_success, "valid SARIF");
    assert_eq!(
        format!("sarif-{}", success_json["version"].as_str().unwrap()),
        expected_success["stdout"]["document"]
    );
    assert_eq!(
        success_json["runs"][0]["results"].as_array().unwrap().len(),
        1
    );

    let expected_malformed = expected_outcome("sarif-projection", "malformed");
    let malformed_argv = contract_argv(&expected_malformed, &[]);
    let malformed = run_server(dir.path(), &malformed_argv, b"not-json\n");
    assert_exit(&malformed, &expected_malformed, "SARIF malformed input");
    let malformed_json = stdout_json(&malformed, &expected_malformed, "malformed-input SARIF");
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

#[test]
fn every_mcp_contract_outcome_is_executed_once() {
    runtime_coverage::assert_exact(
        &contract(),
        "assay-mcp-server",
        &[
            enforcing_proxy_denial_is_structured_but_startup_failure_is_not,
            sarif_projection_currently_turns_malformed_input_into_clean_output,
        ],
    );
}
