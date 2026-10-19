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
fn enforcing_proxy_denial_and_startup_failure_are_structured_on_their_own_channels() {
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
    assert_eq!(
        startup_expected["reason_code"],
        "proxy_enforce_policy_invalid"
    );
    assert_eq!(
        startup_expected["next_step"],
        "Check --enforce-policy and retry proxy-enforce."
    );
    assert!(startup_expected["gap_issue"].is_null());

    let stderr = String::from_utf8(startup_failure.stderr).expect("startup stderr is UTF-8");
    let events: Vec<Value> = stderr
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|event: &Value| event["event"] == "startup_failure")
        .collect();
    assert_eq!(events.len(), 1, "startup stderr lacked one event: {stderr}");
    assert_eq!(events[0]["reason_code"], startup_expected["reason_code"]);
    assert_eq!(events[0]["next_step"], startup_expected["next_step"]);
}

#[test]
fn sarif_projection_rejects_malformed_input_without_clean_output() {
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
    assert_empty_stdout(&malformed, &expected_malformed, "SARIF malformed input");
    let stderr = String::from_utf8_lossy(&malformed.stderr);
    assert!(
        stderr.contains("input line 1") && stderr.contains("expected one JSON value"),
        "malformed-input diagnosis is not actionable: {stderr}"
    );
    assert!(
        !stderr.contains("not-json"),
        "stderr leaked input: {stderr}"
    );
}

#[test]
fn every_mcp_contract_outcome_is_executed_once() {
    runtime_coverage::assert_exact(
        &contract(),
        "assay-mcp-server",
        &[
            enforcing_proxy_denial_and_startup_failure_are_structured_on_their_own_channels,
            sarif_projection_rejects_malformed_input_without_clean_output,
        ],
    );
}

// Execute the documented inputs against the shipped stdio endpoint. These are
// workspace-only docs contracts, like the generated journey rows above.
fn doc_fence_body<'a>(section: &'a str, language: &str) -> &'a str {
    let body = section
        .split_once(&format!("```{language}"))
        .expect("documented executable block")
        .1;
    body.strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
        .expect("documented fence line ending")
}

fn doc_fence<'a>(section: &'a str, language: &str) -> &'a str {
    doc_fence_body(section, language)
        .split_once("```")
        .expect("closed executable block")
        .0
}

fn doc_json(section: &str) -> Value {
    let source = doc_fence(section, "json")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    serde_json::Deserializer::from_str(&source)
        .into_iter::<Value>()
        .next()
        .expect("documented JSON request")
        .expect("valid documented JSON")
}

fn assert_documented_decision(section: &str, decision: &Value, page: &str) {
    let after_request = doc_fence_body(section, "json").split_once("```").unwrap().1;
    let documented = doc_json(after_request);
    let fields = documented.as_object().unwrap();
    assert!(!fields.is_empty(), "{page}: empty documented response");
    for (key, value) in fields {
        assert_eq!(
            decision.get(key),
            Some(value),
            "{page}: documented response field {key}"
        );
    }
}

fn self_correction_args_example(page: &str, policy_heading: &str) {
    let doc = std::fs::read_to_string(workspace_root().join(page)).unwrap();
    let args_section = doc.split_once("### assay_check_args").unwrap().1;
    let args = doc_json(args_section);
    let sequence_section = doc.split_once("### assay_check_sequence").unwrap().1;
    let sequence = doc_json(sequence_section);
    let dir = tempfile::tempdir().unwrap();
    for (request, heading) in [
        (&args, policy_heading),
        (&sequence, "### Sequence Prerequisites"),
    ] {
        let policy = doc_fence(doc.split_once(heading).unwrap().1, "yaml");
        // The documented filename is independent of the request, so a mismatch fails.
        let filename = policy.lines().next().unwrap().strip_prefix("# ").unwrap();
        assert!(matches!(filename, "discounts.yaml" | "sequence.yaml"));
        assert_eq!(
            request["policy"], filename,
            "{page}: policy filename mismatch"
        );
        std::fs::write(dir.path().join(filename), policy).unwrap();
    }
    let mut argv = doc_fence(&doc, "bash").split_whitespace();
    assert_eq!(argv.next(), Some("assay-mcp-server"));
    let mut command = clean_server_command();
    let child = command
        .current_dir(dir.path())
        .args(argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let mut conn = Conn::attach(child);
    conn.request(
        "initialize",
        serde_json::json!({
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "self-correction-docs", "version": "1"}
        }),
        1,
    );
    let mut rows = Vec::new();
    for (index, (request, expected)) in [
        (args.clone(), false),
        (
            {
                let mut retry = args.clone();
                retry["arguments"]["percent"] = 40.into();
                retry
            },
            false,
        ),
        (
            {
                let mut retry = args.clone();
                retry["arguments"]["percent"] = 30.into();
                retry
            },
            true,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let reply = conn.request(
            "tools/call",
            serde_json::json!({
                "name": "assay_check_args", "arguments": request
            }),
            index as u64 + 2,
        );
        assert_eq!(
            reply["result"]["isError"], !expected,
            "{page}: policy denial is an MCP tool error: {reply}"
        );
        let decision: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(
            decision["allowed"], expected,
            "{page}: discount proposal {request}"
        );
        assert_eq!(decision.get("suggested_fix"), Some(&Value::Null));
        if !expected {
            assert_eq!(
                decision["code"], "E_ARG_SCHEMA",
                "{page}: must reach argument policy evaluation"
            );
        }
        if index == 0 {
            assert_documented_decision(args_section, &decision, page);
        }
        rows.push(serde_json::json!({"request": request, "reply": reply, "allowed": expected}));
    }
    for (index, expected) in [false, true].into_iter().enumerate() {
        let mut request = sequence.clone();
        if expected {
            request["history"] = serde_json::json!(["verify_identity"]);
        }
        let reply = conn.request(
            "tools/call",
            serde_json::json!({
                "name": "assay_check_sequence", "arguments": request
            }),
            index as u64 + 5,
        );
        assert_eq!(
            reply["result"]["isError"], !expected,
            "{page}: sequence example: {reply}"
        );
        let decision: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(decision["allowed"], expected, "{page}: sequence result");
        if index == 0 {
            assert_documented_decision(sequence_section, &decision, page);
        }
        assert_eq!(decision.get("suggested_fix"), Some(&Value::Null));
        assert_eq!(
            decision["violations"].as_array().unwrap().is_empty(),
            expected
        );
    }
    std::fs::remove_file(dir.path().join("discounts.yaml")).unwrap();
    let reply = conn.request(
        "tools/call",
        serde_json::json!({
            "name": "assay_check_args", "arguments": args
        }),
        7,
    );
    assert_eq!(
        reply["result"]["isError"], true,
        "missing policy is a tool error"
    );
    rows.push(serde_json::json!({"request": args, "reply": reply, "allowed": false}));
    let reply = conn.request("unknown_method", serde_json::json!({}), 8);
    assert!(reply.get("error").is_some());
    rows.push(serde_json::json!({"request": args, "reply": reply, "allowed": false}));
    // A truthy non-boolean is not an explicit allow; this is a malformed-response control.
    let mut malformed = rows[2].clone();
    malformed["reply"]["result"]["content"][0]["text"] = "{\"allowed\":\"false\"}".into();
    malformed["allowed"] = false.into();
    rows.push(malformed);
    let integration =
        std::fs::read_to_string(workspace_root().join("docs/mcp/self-correction.md")).unwrap();
    let snippet = doc_fence(
        integration.split_once("### Python Agent").unwrap().1,
        "python",
    );
    // Feed the documented consumer actual responses from the live server above.
    // The executor is only a side-effect witness; it cannot fabricate an allow.
    let program = format!(
        "{snippet}\n{}",
        r#"
import sys
rows = json.load(sys.stdin)
executed = []
for row in rows:
    request = row['request']
    def call_check(name, arguments):
        assert name == 'assay_check_args'
        assert arguments == request, (arguments, request)
        return row['reply']
    def execute_tool(name, arguments):
        executed.append((name, arguments))
    count = len(executed)
    try:
        checked_call(call_check, execute_tool, request['tool'], request['arguments'])
    except ValueError:
        assert not row['allowed'], row
    assert len(executed) - count == int(row['allowed']), row
assert len(executed) == 1, executed
assert executed[0] == ('apply_discount', {'percent': 30}), executed
"#
    );
    let mut python = Command::new(required_python());
    python.args(["-I", "-c", &program]);
    let output = run_bounded(
        python,
        &serde_json::to_vec(&rows).unwrap(),
        GOLDEN_PATH_LIMITS,
        "documented self-correction consumer",
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{page}: documented consumer failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn mcp_self_correction_argument_example_runs() {
    self_correction_args_example(
        "docs/mcp/self-correction.md",
        "## Policies for Self-Correction",
    );
}

#[test]
fn use_case_self_correction_argument_example_runs() {
    self_correction_args_example("docs/use-cases/self-correction.md", "### Policy");
}

#[test]
fn self_correction_fences_accept_lf_and_crlf() {
    for newline in ["\n", "\r\n"] {
        let section = format!(
            "Request:{newline}```json{newline}{{\"tool\":\"apply_discount\"}}{newline}```{newline}Response:{newline}```json{newline}{{\"allowed\":false}}{newline}```{newline}"
        );
        assert_eq!(
            doc_json(&section),
            serde_json::json!({"tool": "apply_discount"})
        );
        assert_documented_decision(
            &section,
            &serde_json::json!({"allowed": false}),
            "newline fixture",
        );
    }
}
