//! `assay_check_args` answers a call the way `assay mcp wrap` decides it before forwarding.
//!
//! Wrap refuses a call that an `approval_required`, `restrict_scope` or `redact_args` control
//! cannot let through, and its cases live in `crates/assay-cli/tests/mcp_wrap_obligations.rs`. The
//! same policy files and arguments are used here: a pre-flight answer of `allowed: true` for a call
//! wrap would refuse is the answer this test exists to rule out, and an answer of `allowed: false`
//! for a call wrap forwards is the opposite mistake, so every refusal case has a forwarded twin.

use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const SCOPE: &str = "  restrict_scope: [deploy]\n  restrict_scope_contract:\n    scope_type: resource\n    scope_value: /workspace\n    scope_match_mode: prefix\n";
const APPROVAL: &str = "  approval_required: [deploy]\n";
const REDACTION: &str = "  redact_args: [deploy]\n  redact_args_contract:\n    redaction_target: body\n    redaction_mode: mask\n    redaction_scope: args\n";
const FAR_FUTURE: &str = "2999-01-01T00:00:00Z";

fn spawn_server(policy_root: &Path) -> Conn {
    let child = Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args([
            "--policy-root",
            policy_root.to_str().expect("utf-8 policy-root"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn assay-mcp-server");
    Conn::attach(child)
}

fn initialize(conn: &mut Conn) {
    conn.request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "check-args-obligations", "version": "1.0"}
        }),
        1,
    );
}

fn approval(tool: &str, resource: &str, expires_at: &str) -> Value {
    json!({
        "approval_id": "approval-1",
        "approver": "reviewer",
        "issued_at": "2020-01-01T00:00:00Z",
        "expires_at": expires_at,
        "scope": "deploy",
        "bound_tool": tool,
        "bound_resource": resource,
    })
}

/// The answer `assay_check_args` gives for `deploy` with these arguments under a policy whose
/// `tools:` block is `controls`, as wrap's policy file is written.
fn check(controls: &str, arguments: Value) -> Value {
    let dir = tempfile::tempdir().expect("policy-root");
    fs::write(
        dir.path().join("policy.yaml"),
        format!("version: '2.0'\nname: obligations\ntools:\n{controls}"),
    )
    .expect("write policy");
    let mut conn = spawn_server(dir.path());
    initialize(&mut conn);
    let resp = conn.request(
        "tools/call",
        json!({
            "name": "assay_check_args",
            "arguments": {"tool": "deploy", "arguments": arguments, "policy": "policy.yaml"}
        }),
        2,
    );
    let _ = conn.shutdown();
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call missing text: {resp}"))
        .to_string();
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("body is not JSON ({e}): {text}"))
}

fn assert_refused(label: &str, body: &Value, code: &str) {
    assert_eq!(
        body["allowed"], false,
        "{label}: wrap refuses this call; body={body}"
    );
    assert_eq!(
        body["code"], code,
        "{label}: wrong refusal code; body={body}"
    );
}

fn assert_allowed(label: &str, body: &Value) {
    assert_eq!(
        body["allowed"], true,
        "{label}: wrap forwards this call; body={body}"
    );
}

#[test]
fn approval_required_without_a_valid_artifact_is_refused() {
    let controls = format!("  allow: [deploy]\n{APPROVAL}");
    let cases = [
        ("missing", json!({"resource": "prod"})),
        (
            "expired",
            json!({"resource": "prod", "_meta": {"approval": approval("deploy", "prod", "2021-01-01T00:00:00Z")}}),
        ),
        (
            "other tool",
            json!({"resource": "prod", "_meta": {"approval": approval("other", "prod", FAR_FUTURE)}}),
        ),
        (
            "other resource",
            json!({"resource": "prod", "_meta": {"approval": approval("deploy", "staging", FAR_FUTURE)}}),
        ),
    ];
    for (name, arguments) in cases {
        assert_refused(name, &check(&controls, arguments), "P_APPROVAL_REQUIRED");
    }
}

#[test]
fn a_valid_approval_is_allowed() {
    let controls = format!("  allow: [deploy]\n{APPROVAL}");
    let arguments =
        json!({"resource": "prod", "_meta": {"approval": approval("deploy", "prod", FAR_FUTURE)}});
    assert_allowed("valid approval", &check(&controls, arguments));
}

#[test]
fn restrict_scope_refuses_what_is_not_inside_the_scope() {
    let controls = format!("  allow: [deploy]\n{SCOPE}");
    let cases = [
        ("outside", json!({"resource": "/etc/passwd"})),
        ("absent", json!({"path": "/workspace/a"})),
        ("sibling", json!({"resource": "/workspace-other/a"})),
        ("parent", json!({"resource": "/workspace/../etc/passwd"})),
    ];
    for (name, arguments) in cases {
        assert_refused(name, &check(&controls, arguments), "P_RESTRICT_SCOPE");
    }
}

#[test]
fn restrict_scope_allows_what_is_inside_the_scope() {
    let controls = format!("  allow: [deploy]\n{SCOPE}");
    for resource in ["/workspace", "/workspace/a/b"] {
        assert_allowed(resource, &check(&controls, json!({"resource": resource})));
    }
}

#[test]
fn redaction_that_cannot_apply_is_refused_and_one_that_can_is_allowed() {
    let controls = format!("  allow: [deploy]\n{REDACTION}");
    assert_refused(
        "no body to redact",
        &check(&controls, json!({"token": "t"})),
        "P_REDACT_ARGS",
    );
    assert_allowed(
        "body redacted",
        &check(&controls, json!({"body": "b", "resource": "x"})),
    );
}

/// Without a control the answer is the policy decision, unchanged.
#[test]
fn controls_without_obligations_keep_their_answer() {
    assert_allowed(
        "allow",
        &check("  allow: [deploy]\n", json!({"resource": "x"})),
    );
    assert_refused(
        "deny",
        &check("  deny: [deploy]\n", json!({"resource": "x"})),
        "E_TOOL_DENIED",
    );
}
