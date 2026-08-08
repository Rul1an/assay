//! MCP-owned rows of the #1975 stdout-and-exit-code journey contract.

use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn contract_guide() -> String {
    let path = workspace_root().join("docs/guides/agent-golden-path.md");
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "read agent golden-path contract {}: {error}",
            path.display()
        )
    })
}

fn assert_documented(needles: &[&str]) {
    let guide = contract_guide();
    for needle in needles {
        assert!(
            guide.contains(needle),
            "agent golden-path contract does not pin {needle:?}"
        );
    }
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
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn assay-mcp-server");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(stdin)
        .expect("write child stdin");
    child.wait_with_output().expect("wait for server")
}

fn json_lines(bytes: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("stdout line is not JSON: {error}: {line}"))
        })
        .collect()
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
    let requests = [
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "golden-path-contract", "version": "1"}
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {
                "name": "github.add_deploy_key",
                "arguments": {"owner": "acme", "repo": "prod-app"}
            }
        }),
    ];
    let mut input = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    input.push('\n');

    let success = run_server(
        &example,
        &[
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
        ],
        input.as_bytes(),
    );
    assert_eq!(
        success.status.code(),
        Some(0),
        "proxy failed: {}",
        String::from_utf8_lossy(&success.stderr)
    );
    let responses = json_lines(&success.stdout);
    let denied = responses
        .iter()
        .find(|response| response["id"] == 9)
        .expect("tools/call response");
    assert_eq!(denied["error"]["code"], -32042);
    assert_eq!(denied["error"]["data"]["origin"], "assay-proxy");
    assert_eq!(denied["error"]["data"]["reason"], "no_declared_allowance");

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
        input.as_bytes(),
    );
    assert_eq!(startup_failure.status.code(), Some(1));
    assert!(startup_failure.stdout.is_empty());

    assert_documented(&[
        "| 5. Protected action |",
        "`assay-mcp-server proxy-enforce <args>`",
        "no_declared_allowance",
        "#2163",
    ]);
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
    assert_eq!(success.status.code(), Some(0));
    let success_json: Value = serde_json::from_slice(&success.stdout).expect("valid SARIF stdout");
    assert_eq!(success_json["version"], "2.1.0");
    assert_eq!(
        success_json["runs"][0]["results"].as_array().unwrap().len(),
        1
    );

    let malformed = run_server(
        dir.path(),
        &["enforcement-sarif", "--input", "-", "--output", "-"],
        b"not-json\n",
    );
    assert_eq!(malformed.status.code(), Some(0));
    let malformed_json: Value =
        serde_json::from_slice(&malformed.stdout).expect("malformed-input SARIF stdout");
    assert_eq!(malformed_json["version"], "2.1.0");
    assert!(malformed_json["runs"][0]["results"]
        .as_array()
        .unwrap()
        .is_empty());

    assert_documented(&[
        "| 8. SARIF projection |",
        "`assay-mcp-server enforcement-sarif --input <decisions.ndjson> --output -`",
        "#2166",
    ]);
}
