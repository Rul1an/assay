//! `assay_check_sequence` refuses a policy file whose sequence rule has a key the rule does not
//! take, instead of checking the call against the rule without it.
//!
//! The rule types used to drop such a key. `strict` misspelt on a `sequence` rule left the rule
//! non-strict, so a call with another call between the rule's two tools was allowed. The refusal is
//! the tool's fixed parse summary: a policy's content never reaches the client.

use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

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

/// The answer for calling `b` after `a` and `c` under a policy file with `contents`.
fn check(contents: &str) -> Value {
    let dir = tempfile::tempdir().expect("policy-root");
    fs::write(dir.path().join("policy.yaml"), contents).expect("write policy");
    let mut conn = spawn_server(dir.path());
    conn.request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "check-sequence-unknown-keys", "version": "1.0"}
        }),
        1,
    );
    let resp = conn.request(
        "tools/call",
        json!({
            "name": "assay_check_sequence",
            "arguments": {"history": ["a", "c"], "next_tool": "b", "policy": "policy.yaml"}
        }),
        2,
    );
    let _ = conn.shutdown();
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call missing text: {resp}"))
        .to_string();
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
}

/// A list of rules, and a policy's `sequences`, each with `{key}` where the rule's `strict` goes.
const POLICIES: &[(&str, &str)] = &[
    (
        "a list of rules",
        "- type: sequence\n  tools: [a, b]\n  {key}: true\n",
    ),
    (
        "a policy's sequences",
        "version: \"1.0\"\nsequences:\n  - type: sequence\n    tools: [a, b]\n    {key}: true\n",
    ),
];

/// The control: spelt right, the rule is strict and the call is not allowed.
#[test]
fn a_strict_sequence_rule_denies_a_call_with_another_between() {
    for (form, policy) in POLICIES {
        let body = check(&policy.replace("{key}", "strict"));
        assert_eq!(body["allowed"], json!(false), "{form}: {body}");
        assert!(
            body["error"].is_null(),
            "{form}: a violation, not an error: {body}"
        );
    }
}

/// Misspelt, the policy is refused with the fixed parse summary, and the key is not echoed.
#[test]
fn a_key_a_sequence_rule_does_not_take_refuses_the_policy() {
    for (form, policy) in POLICIES {
        let body = check(&policy.replace("{key}", "strictly"));
        assert_eq!(body["allowed"], json!(false), "{form}: {body}");
        assert_eq!(
            body["error"]["code"],
            json!("E_POLICY_PARSE"),
            "{form}: {body}"
        );
        assert_eq!(
            body["error"]["message"],
            json!("Invalid sequence policy format"),
            "{form}: {body}"
        );
        assert!(
            !body.to_string().contains("strictly"),
            "{form}: the policy's content must not reach the client: {body}"
        );
    }
}
