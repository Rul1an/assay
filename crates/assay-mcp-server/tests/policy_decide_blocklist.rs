//! Real stdio `tools/call` contract for `assay_policy_decide` blocklist parsing.
//!
//! A present `blocklist` that is not a string sequence must fail closed
//! (`allowed: false`, `E_POLICY_PARSE`, `isError: true`) and must not be
//! cached as an empty list. Absent and `[]` remain allow; a valid string
//! list still denies. Non-mapping roots and canonical/mixed dialects fail
//! closed before cache insertion.

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const ROOT_MUST_BE_MAPPING: &str = "Policy root must be a mapping";
const STRUCTURE_INVALID: &str = "Policy structure is invalid";
const BEGIN_SECRET: &str = "BEGIN_SECRET";
const END_SECRET: &str = "END_SECRET";
const LARGE_ROOT_BYTES: usize = 200_000;

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
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "policy-decide-blocklist", "version": "1.0"}
        }),
        1,
    );
}

fn call_policy_decide(conn: &mut Conn, policy: &str, tool: &str, id: u64) -> (Value, Value) {
    let resp = conn.request(
        "tools/call",
        serde_json::json!({
            "name": "assay_policy_decide",
            "arguments": {
                "tool": tool,
                "policy": policy
            }
        }),
        id,
    );
    let result = resp
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("tools/call {id} missing result: {resp}"));
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call {id} missing text: {result}"));
    let body: Value = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("tools/call {id} body is not JSON ({e}): {text}"));
    (result, body)
}

fn assert_parse_error(label: &str, result: &Value, body: &Value, expected_message: &str) {
    assert_eq!(
        result.get("isError").and_then(Value::as_bool),
        Some(true),
        "{label}: MCP isError must be true; result={result} body={body}"
    );
    assert_eq!(
        body.get("allowed").and_then(Value::as_bool),
        Some(false),
        "{label}: allowed must be false; body={body}"
    );
    assert_eq!(
        body.pointer("/error/code").and_then(Value::as_str),
        Some("E_POLICY_PARSE"),
        "{label}: error.code must be E_POLICY_PARSE; body={body}"
    );
    assert_eq!(
        body.pointer("/error/message").and_then(Value::as_str),
        Some(expected_message),
        "{label}: wrong stable parse summary; body={body}"
    );
}

fn assert_allowed(label: &str, result: &Value, body: &Value) {
    assert_eq!(
        result.get("isError").and_then(Value::as_bool),
        Some(false),
        "{label}: MCP isError must be false; result={result} body={body}"
    );
    assert_eq!(
        body.get("allowed").and_then(Value::as_bool),
        Some(true),
        "{label}: allowed must be true; body={body}"
    );
    assert!(
        body.get("error").is_none(),
        "{label}: allow path must not carry error; body={body}"
    );
}

fn assert_denied(label: &str, result: &Value, body: &Value, tool: &str) {
    assert_eq!(
        result.get("isError").and_then(Value::as_bool),
        Some(true),
        "{label}: MCP isError must be true; result={result} body={body}"
    );
    assert_eq!(
        body.get("allowed").and_then(Value::as_bool),
        Some(false),
        "{label}: allowed must be false; body={body}"
    );
    assert!(
        body.get("error").is_none(),
        "{label}: valid deny must not carry a parse error; body={body}"
    );
    let matches = body["matches"]
        .as_array()
        .unwrap_or_else(|| panic!("{label}: valid deny should return matches; body={body}"));
    assert!(
        matches.iter().any(|m| m
            .as_str()
            .is_some_and(|s| s.contains(tool) && s.contains("blocked"))),
        "{label}: valid deny matches missing blocked-tool text: {body}"
    );
}

fn write_policy(dir: &Path, name: &str, contents: &str) {
    fs::write(dir.join(name), contents).unwrap_or_else(|e| panic!("write {name}: {e}"));
}

fn large_scalar_root() -> String {
    let wrapper = 2; // surrounding YAML double quotes
    let filler = LARGE_ROOT_BYTES
        .checked_sub(wrapper + BEGIN_SECRET.len() + END_SECRET.len())
        .expect("sentinel overhead fits in 200k");
    let mut contents = String::with_capacity(LARGE_ROOT_BYTES);
    contents.push('"');
    contents.push_str(BEGIN_SECRET);
    contents.extend(std::iter::repeat_n('A', filler));
    contents.push_str(END_SECRET);
    contents.push('"');
    assert_eq!(
        contents.len(),
        LARGE_ROOT_BYTES,
        "large root must be a 200,000-byte syntactically valid YAML scalar"
    );
    contents
}

fn assert_no_sentinels(label: &str, result: &Value, body: &Value) {
    let blob = format!("{result}{body}");
    assert!(
        !blob.contains(BEGIN_SECRET),
        "{label}: response reflected {BEGIN_SECRET}; body={body}"
    );
    assert!(
        !blob.contains(END_SECRET),
        "{label}: response reflected {END_SECRET}; body={body}"
    );
}

fn assert_repeat_parse_error(
    conn: &mut Conn,
    policy: &str,
    tool: &str,
    id: &mut u64,
    expected_message: &str,
) {
    let (first_result, first_body) = call_policy_decide(conn, policy, tool, *id);
    *id += 1;
    assert_parse_error(
        &format!("{policy} first call"),
        &first_result,
        &first_body,
        expected_message,
    );

    let (second_result, second_body) = call_policy_decide(conn, policy, tool, *id);
    *id += 1;
    assert_parse_error(
        &format!("{policy} repeat call"),
        &second_result,
        &second_body,
        expected_message,
    );
}

fn write_field_shape_fixtures(root: &Path) {
    write_policy(root, "string.yaml", "blocklist: dangerous_tool\n");
    write_policy(root, "mapping.yaml", "blocklist:\n  dangerous_tool: true\n");
    write_policy(root, "number.yaml", "blocklist: 7\n");
    write_policy(root, "null.yaml", "blocklist: null\n");
    write_policy(root, "bool.yaml", "blocklist: true\n");
    write_policy(
        root,
        "mixed.yaml",
        "blocklist:\n  - dangerous_tool\n  - 7\n",
    );
    write_policy(root, "bare.yaml", "blocklist:\n");
}

fn write_compatibility_fixtures(root: &Path) {
    write_policy(root, "absent.yaml", "version: 1\n");
    write_policy(root, "empty.yaml", "blocklist: []\n");
    write_policy(root, "deny.yaml", "blocklist:\n  - dangerous_tool\n");
    write_policy(root, "metadata.yaml", "name: metadata-only\n");
    write_policy(root, "wildcard.yaml", "blocklist:\n  - \"dangerous_*\"\n");
}

const UNSUPPORTED_DIALECTS: [(&str, &str); 5] = [
    ("root-allow.yaml", "allow:\n  - dangerous_tool\n"),
    ("root-deny.yaml", "deny:\n  - dangerous_tool\n"),
    ("tools-deny.yaml", "tools:\n  deny:\n    - dangerous_tool\n"),
    (
        "mixed-root-deny.yaml",
        "blocklist:\n  - dangerous_tool\ndeny:\n  - other_tool\n",
    ),
    (
        "mixed-tools.yaml",
        "blocklist:\n  - dangerous_tool\ntools:\n  deny:\n    - other_tool\n",
    ),
];

#[test]
fn present_malformed_blocklist_fails_closed_and_is_not_cached() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    write_field_shape_fixtures(root);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let mut id = 2;
    for policy in [
        "string.yaml",
        "mapping.yaml",
        "number.yaml",
        "null.yaml",
        "bool.yaml",
        "mixed.yaml",
        "bare.yaml",
    ] {
        assert_repeat_parse_error(
            &mut conn,
            policy,
            "dangerous_tool",
            &mut id,
            STRUCTURE_INVALID,
        );
    }
    let _ = conn.shutdown();
}

#[test]
fn non_mapping_roots_fail_closed_without_source_reflection() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    write_policy(root, "root-scalar.yaml", "dangerous_tool\n");
    write_policy(root, "root-sequence.yaml", "- dangerous_tool\n");
    write_policy(root, "root-null-document.yaml", "null\n");
    write_policy(root, "root-empty-document.yaml", "");
    write_policy(root, "root-large-scalar.yaml", &large_scalar_root());

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let mut id = 2;
    for policy in [
        "root-scalar.yaml",
        "root-sequence.yaml",
        "root-null-document.yaml",
        "root-empty-document.yaml",
        "root-large-scalar.yaml",
    ] {
        let (first_result, first_body) =
            call_policy_decide(&mut conn, policy, "dangerous_tool", id);
        id += 1;
        assert_parse_error(
            &format!("{policy} first call"),
            &first_result,
            &first_body,
            ROOT_MUST_BE_MAPPING,
        );
        assert_no_sentinels(&format!("{policy} first call"), &first_result, &first_body);

        let (second_result, second_body) =
            call_policy_decide(&mut conn, policy, "dangerous_tool", id);
        id += 1;
        assert_parse_error(
            &format!("{policy} repeat call"),
            &second_result,
            &second_body,
            ROOT_MUST_BE_MAPPING,
        );
        assert_no_sentinels(
            &format!("{policy} repeat call"),
            &second_result,
            &second_body,
        );
    }
    let _ = conn.shutdown();
}

#[test]
fn canonical_and_mixed_dialects_fail_closed() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    for (name, contents) in UNSUPPORTED_DIALECTS {
        write_policy(root, name, contents);
    }

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let mut id = 2;
    for (policy, _) in UNSUPPORTED_DIALECTS {
        assert_repeat_parse_error(
            &mut conn,
            policy,
            "dangerous_tool",
            &mut id,
            STRUCTURE_INVALID,
        );
    }
    let _ = conn.shutdown();
}

#[test]
fn exact_name_blocklist_compatibility_controls() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    write_compatibility_fixtures(root);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let mut id = 2;

    let (absent_result, absent_body) =
        call_policy_decide(&mut conn, "absent.yaml", "dangerous_tool", id);
    id += 1;
    assert_allowed("absent blocklist", &absent_result, &absent_body);

    let (metadata_result, metadata_body) =
        call_policy_decide(&mut conn, "metadata.yaml", "dangerous_tool", id);
    id += 1;
    assert_allowed("metadata-only mapping", &metadata_result, &metadata_body);

    let (empty_result, empty_body) =
        call_policy_decide(&mut conn, "empty.yaml", "dangerous_tool", id);
    id += 1;
    assert_allowed("empty blocklist []", &empty_result, &empty_body);

    let (deny_result, deny_body) = call_policy_decide(&mut conn, "deny.yaml", "dangerous_tool", id);
    id += 1;
    assert_denied(
        "valid [dangerous_tool]",
        &deny_result,
        &deny_body,
        "dangerous_tool",
    );

    let (wildcard_allow_result, wildcard_allow_body) =
        call_policy_decide(&mut conn, "wildcard.yaml", "dangerous_tool", id);
    id += 1;
    assert_allowed(
        "literal wildcard does not match dangerous_tool",
        &wildcard_allow_result,
        &wildcard_allow_body,
    );

    let (wildcard_deny_result, wildcard_deny_body) =
        call_policy_decide(&mut conn, "wildcard.yaml", "dangerous_*", id);
    assert_denied(
        "literal wildcard denies exact dangerous_*",
        &wildcard_deny_result,
        &wildcard_deny_body,
        "dangerous_*",
    );

    let _ = conn.shutdown();
}
