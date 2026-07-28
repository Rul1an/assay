use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const SECRET_VALUE: &str = "must-not-appear-in-diagnostics";

fn clean_command() -> Command {
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

fn run_with_auth_env(args: &[&str], name: &str, stdin: &[u8]) -> Output {
    let mut command = clean_command();
    command
        .args(args)
        .env(name, SECRET_VALUE)
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

fn assert_value_free_auth_refusal(output: &Output, variable: &str) {
    assert!(!output.status.success(), "legacy auth config must fail");
    assert!(
        output.stdout.is_empty(),
        "refusal must happen before protocol output: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported") && stderr.contains(variable),
        "diagnostic must name the unsupported boundary and variable: {stderr}"
    );
    assert!(
        !stderr.contains(SECRET_VALUE),
        "diagnostic leaked the configured auth value: {stderr}"
    );
}

fn policy_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/mcp")
}

#[test]
fn standalone_rejects_known_auth_configuration_before_protocol_io() {
    let output = run_with_auth_env(
        &["--policy-root", "does-not-need-to-exist"],
        "ASSAY_AUTH_MODE",
        br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    );
    assert_value_free_auth_refusal(&output, "ASSAY_AUTH_MODE");
}

#[test]
fn proxy_rejects_unknown_auth_configuration_before_spawning_upstream() {
    let output = run_with_auth_env(
        &[
            "proxy",
            "--upstream-command",
            "this-command-must-never-be-spawned",
        ],
        "ASSAY_AUTH_FUTURE_OPTION",
        b"",
    );
    assert_value_free_auth_refusal(&output, "ASSAY_AUTH_FUTURE_OPTION");
}

#[test]
fn enforcing_proxy_rejects_auth_configuration_before_loading_policy() {
    let output = run_with_auth_env(
        &[
            "proxy-enforce",
            "--upstream-command",
            "this-command-must-never-be-spawned",
            "--enforce-policy",
            "does-not-need-to-exist.yaml",
            "--declared-mcp-manifest",
            "does-not-need-to-exist.json",
        ],
        "ASSAY_AUTH_JWKS_URI",
        b"",
    );
    assert_value_free_auth_refusal(&output, "ASSAY_AUTH_JWKS_URI");
}

#[test]
fn offline_sarif_projection_ignores_server_auth_environment() {
    let output = run_with_auth_env(
        &["enforcement-sarif", "--input", "-", "--output", "-"],
        "ASSAY_AUTH_MODE",
        b"",
    );
    assert!(
        output.status.success(),
        "offline projection must remain independent of server auth configuration: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let sarif: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid SARIF JSON output");
    assert_eq!(sarif["version"], "2.1.0");
    assert!(!String::from_utf8_lossy(&output.stderr).contains(SECRET_VALUE));
}

#[test]
fn refusal_names_all_configured_variables_in_stable_order_without_values() {
    let mut command = clean_command();
    command
        .args(["--policy-root", "does-not-need-to-exist"])
        .env("ASSAY_AUTH_Z_FUTURE", "second-secret")
        .env("ASSAY_AUTH_A_FUTURE", "first-secret")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = command.output().expect("run assay-mcp-server");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let first = stderr.find("ASSAY_AUTH_A_FUTURE").expect("first name");
    let second = stderr.find("ASSAY_AUTH_Z_FUTURE").expect("second name");
    assert!(first < second, "variable names must be sorted: {stderr}");
    assert!(!stderr.contains("first-secret") && !stderr.contains("second-secret"));
}

#[test]
fn lowercase_auth_namespace_is_rejected_for_cross_platform_consistency() {
    let output = run_with_auth_env(
        &["--policy-root", "does-not-need-to-exist"],
        "assay_auth_mode",
        b"",
    );
    assert_value_free_auth_refusal(&output, "assay_auth_mode");
}

#[test]
fn initialize_credential_fields_are_ignored_as_authority_and_never_logged() {
    let secret = "Bearer initialize-secret-must-not-be-logged";
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {
                "name": "auth-boundary-test",
                "version": "1.0"
            },
            "authorization": secret,
            "initializationOptions": {
                "authorization": secret
            }
        }
    });

    let mut command = clean_command();
    command
        .arg("--policy-root")
        .arg(policy_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn assay-mcp-server");
    writeln!(
        child.stdin.take().expect("child stdin"),
        "{}",
        serde_json::to_string(&request).expect("serialize request")
    )
    .expect("write request");
    let output = child.wait_with_output().expect("wait for server");

    assert!(output.status.success(), "{:?}", output.status);
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("initialize response");
    assert!(response.get("result").is_some(), "{response}");
    assert!(response.get("error").is_none(), "{response}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains(secret),
        "initialize value leaked: {stderr}"
    );
    assert!(
        !stderr.contains("auth_success")
            && !stderr.contains("auth_failure")
            && !stderr.contains("auth_missing"),
        "initialize was interpreted as authentication: {stderr}"
    );
}
