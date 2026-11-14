//! `assay_check_args` names what in a policy file it does not weigh, so an answer never reads as
//! a full check of a file it only partly read.
//!
//! The notes use Assay's own key and tool names only. A key that no Assay reader knows is counted,
//! not quoted: the answer goes to an MCP client, and no text from the policy file may reach it
//! (the same rule the parse summaries follow).

use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const ALLOW: &str = "version: '2.0'\nname: notes\ntools:\n  allow: [deploy]\n";
const UNKNOWN_KEY: &str = "zz_unread_marker";

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

/// The raw response text and the parsed answer for `deploy` under a policy file with `contents`.
fn check(contents: &str) -> (String, Value) {
    let dir = tempfile::tempdir().expect("policy-root");
    fs::write(dir.path().join("policy.yaml"), contents).expect("write policy");
    let mut conn = spawn_server(dir.path());
    conn.request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "check-args-notes", "version": "1.0"}
        }),
        1,
    );
    let resp = conn.request(
        "tools/call",
        json!({
            "name": "assay_check_args",
            "arguments": {"tool": "deploy", "arguments": {"resource": "x"}, "policy": "policy.yaml"}
        }),
        2,
    );
    let _ = conn.shutdown();
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call missing text: {resp}"))
        .to_string();
    let body = serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"));
    (resp.to_string(), body)
}

fn notes(label: &str, body: &Value) -> Vec<String> {
    body["notes"]
        .as_array()
        .unwrap_or_else(|| panic!("{label}: the answer names nothing; body={body}"))
        .iter()
        .map(|note| note.as_str().expect("note is a string").to_string())
        .collect()
}

fn assert_note(label: &str, body: &Value, needles: &[&str]) {
    let notes = notes(label, body);
    assert!(
        notes
            .iter()
            .any(|note| needles.iter().all(|needle| note.contains(needle))),
        "{label}: no note mentions {needles:?}; notes={notes:?}"
    );
}

/// `assay mcp wrap` refuses a policy with a key no Assay reader acts on, so this check, which
/// decides a call as wrap would, refuses it too instead of answering for a policy wrap would not
/// run. The answer is a fixed summary: no text from the policy file reaches the client.
#[test]
fn a_policy_wrap_refuses_is_refused_here_and_not_quoted() {
    for (label, contents) in [
        ("root", format!("{ALLOW}{UNKNOWN_KEY}: [deploy]\n")),
        ("nested", format!("{ALLOW}  {UNKNOWN_KEY}: [deploy]\n")),
        (
            "signatures",
            format!("{ALLOW}signatures:\n  check_descriptions: true\n"),
        ),
    ] {
        let (raw, body) = check(&contents);
        assert_eq!(body["allowed"], json!(false), "{label}: body={body}");
        assert_eq!(
            body["error"]["code"],
            json!("E_POLICY_PARSE"),
            "{label}: body={body}"
        );
        let message = body["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("no Assay reader acts on")
                && message.contains("assay policy validate"),
            "{label}: body={body}"
        );
        assert!(
            !raw.contains(UNKNOWN_KEY),
            "{label}: the policy file's text reached the client: {raw}"
        );
    }
}

#[test]
fn a_key_another_tool_reads_names_that_tool() {
    let (_, body) = check(&format!(
        "{ALLOW}sequences:\n  - {{ type: require, tool: audit }}\n"
    ));
    assert_note("sequences", &body, &["sequences", "assay_check_sequence"]);
    let (_, body) = check(&format!("{ALLOW}blocklist: [deploy]\n"));
    assert_note("blocklist", &body, &["blocklist", "assay_policy_decide"]);
}

#[test]
fn a_key_wrap_leaves_to_another_command_names_it() {
    let (_, body) = check(&format!("{ALLOW}discovery:\n  enabled: true\n"));
    assert_note("discovery", &body, &["discovery", "assay discover"]);
}

#[test]
fn a_pin_this_check_cannot_compare_is_named() {
    let hash = "a".repeat(64);
    let pin = format!(
        "tool_pins:\n  deploy:\n    server_id: s\n    tool_name: deploy\n    schema_hash: \"{hash}\"\n    meta_hash: \"{hash}\"\n"
    );
    let (_, body) = check(&format!("{ALLOW}{pin}"));
    assert_note("pinned", &body, &["pin", "assay mcp wrap"]);

    let other = pin
        .replace("  deploy:\n", "  other:\n")
        .replace("tool_name: deploy", "tool_name: other");
    let (_, body) = check(&format!("{ALLOW}{other}"));
    assert!(
        body.get("notes").is_none(),
        "a pin on another tool says nothing about this call; body={body}"
    );
}

#[test]
fn limits_this_check_cannot_count_are_named() {
    let (_, body) = check(&format!("{ALLOW}limits:\n  max_tool_calls_total: 1\n"));
    assert_note("limits", &body, &["limits", "assay mcp wrap"]);
}

/// A file every key of which this check weighs gets no notes.
#[test]
fn a_file_it_reads_whole_has_no_notes() {
    let (_, body) = check(ALLOW);
    assert!(body.get("notes").is_none(), "body={body}");
    let (_, body) = check(&format!("{ALLOW}schemas:\n  deploy: {{type: object}}\n"));
    assert!(body.get("notes").is_none(), "body={body}");
}
