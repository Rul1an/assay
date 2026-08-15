//! Real-stdio hostile-input contract for policy diagnostic safety (#2387).
//!
//! Spawns `assay-mcp-server` over stdio, exercises all four raw policy-parse
//! sinks with malformed inputs, and asserts:
//!
//! 1. Exactly three fixed public parse summaries per failure class.
//! 2. `result.isError == true` on every parse failure.
//! 3. No sentinel from hostile content reaches the client (independent per position).
//! 4. `error.message` byte length ≤ 4096.
//! 5. No temporary path or policy path leaks into the full JSON-RPC response.
//! 6. Positive numeric `details.line`/`details.column` required for syntax errors.
//! 7. Invalid UTF-8 through real `assay_check_args` → `E_POLICY_PARSE` / `Policy YAML is invalid`.

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

mod jsonrpc_conn;
use jsonrpc_conn::Conn;

const YAML_INVALID: &str = "Policy YAML is invalid";
const ROOT_NOT_MAPPING: &str = "Policy root must be a mapping";
const STRUCTURE_INVALID: &str = "Policy structure is invalid";

const BEGIN_SENTINEL: &str = "BEGIN_SECRET_MARKER_2387";
const MIDDLE_SENTINEL: &str = "MIDDLE_SECRET_MARKER_2387";
const END_SENTINEL: &str = "END_SECRET_MARKER_2387";

const MAX_PUBLIC_MESSAGE_BYTES: usize = 4096;

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
            "clientInfo": {"name": "diag-safety", "version": "1.0"}
        }),
        1,
    );
}

/// Call a tool and return (MCP result object, parsed tool body, full serialized JSON-RPC response).
fn call_tool(conn: &mut Conn, tool: &str, arguments: Value, id: u64) -> (Value, Value, String) {
    let resp = conn.request(
        "tools/call",
        serde_json::json!({"name": tool, "arguments": arguments}),
        id,
    );
    let full_response = serde_json::to_string(&resp).unwrap_or_default();
    let result = resp
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("tools/call {id} missing result: {resp}"));
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call {id} missing text: {result}"))
        .to_string();
    let body: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("tools/call {id} body is not JSON ({e}): {text}"));
    (result, body, full_response)
}

fn write_policy(dir: &Path, name: &str, contents: &[u8]) {
    fs::write(dir.join(name), contents).unwrap_or_else(|e| panic!("write {name}: {e}"));
}

/// Shared assertion: isError, allowed, code, exact message, ceiling, sentinels, path leak.
fn assert_parse_error(
    label: &str,
    result: &Value,
    body: &Value,
    full_response: &str,
    expected_message: &str,
    policy_root: &Path,
) {
    assert_parse_error_with_policy(
        label,
        result,
        body,
        full_response,
        expected_message,
        policy_root,
        None,
    );
}

/// Extended version that also checks the relative policy filename is absent from the response.
fn assert_parse_error_with_policy(
    label: &str,
    result: &Value,
    body: &Value,
    full_response: &str,
    expected_message: &str,
    policy_root: &Path,
    policy_filename: Option<&str>,
) {
    assert_eq!(
        result.get("isError").and_then(Value::as_bool),
        Some(true),
        "{label}: result.isError must be true; result={result}"
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
    let msg = body
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap();
    assert!(
        msg.len() <= MAX_PUBLIC_MESSAGE_BYTES,
        "{label}: error.message exceeds {MAX_PUBLIC_MESSAGE_BYTES} bytes ({} bytes)",
        msg.len()
    );
    assert!(
        !full_response.contains(BEGIN_SENTINEL),
        "{label}: response reflected {BEGIN_SENTINEL}"
    );
    assert!(
        !full_response.contains(MIDDLE_SENTINEL),
        "{label}: response reflected {MIDDLE_SENTINEL}"
    );
    assert!(
        !full_response.contains(END_SENTINEL),
        "{label}: response reflected {END_SENTINEL}"
    );
    // (A) Exclude the raw temp root path
    let root_str = policy_root.to_str().unwrap_or("");
    if !root_str.is_empty() {
        assert!(
            !full_response.contains(root_str),
            "{label}: response contains policy root path '{root_str}'"
        );
    }
    // (A) Exclude the canonical temp root (macOS /var → /private/var alias)
    if let Ok(canon) = policy_root.canonicalize() {
        let canon_str = canon.to_str().unwrap_or("");
        if !canon_str.is_empty() && canon_str != root_str {
            assert!(
                !full_response.contains(canon_str),
                "{label}: response contains canonical policy root path '{canon_str}'"
            );
        }
    }
    // (A) Exclude the relative policy filename
    if let Some(filename) = policy_filename {
        assert!(
            !full_response.contains(filename),
            "{label}: response contains relative policy filename '{filename}'"
        );
    }
    // (B) Exclude baseline UTF-8 error phrase
    assert!(
        !full_response.contains("stream did not contain valid UTF-8"),
        "{label}: response contains raw UTF-8 baseline phrase"
    );
}

/// Build a 200k-byte filler with a single sentinel at its actual beginning.
fn payload_sentinel_at_begin(sentinel: &str) -> Vec<u8> {
    let total = 200_000;
    let mut buf = Vec::with_capacity(total);
    buf.extend_from_slice(sentinel.as_bytes());
    buf.extend(std::iter::repeat_n(b'X', total - sentinel.len()));
    assert_eq!(buf.len(), total);
    buf
}

/// Build a 200k-byte filler with a single sentinel at its actual midpoint.
fn payload_sentinel_at_middle(sentinel: &str) -> Vec<u8> {
    let total = 200_000;
    let half = (total - sentinel.len()) / 2;
    let mut buf = Vec::with_capacity(total);
    buf.extend(std::iter::repeat_n(b'X', half));
    buf.extend_from_slice(sentinel.as_bytes());
    buf.extend(std::iter::repeat_n(b'X', total - buf.len()));
    assert_eq!(buf.len(), total);
    buf
}

/// Build a 200k-byte filler with a single sentinel at its actual end.
fn payload_sentinel_at_end(sentinel: &str) -> Vec<u8> {
    let total = 200_000;
    let mut buf = Vec::with_capacity(total);
    buf.extend(std::iter::repeat_n(b'X', total - sentinel.len()));
    buf.extend_from_slice(sentinel.as_bytes());
    assert_eq!(buf.len(), total);
    buf
}

// ── Tool argument builders ─────────────────────────────────────────────

fn policy_decide_args(policy: &str) -> Value {
    serde_json::json!({"tool": "any_tool", "policy": policy})
}
fn check_args_args(policy: &str) -> Value {
    serde_json::json!({"tool": "any_tool", "arguments": {}, "policy": policy})
}
fn check_coverage_args(policy: &str) -> Value {
    serde_json::json!({"policy": policy, "traces": [{"tools": ["any_tool"]}]})
}
fn explain_trace_args(policy: &str) -> Value {
    serde_json::json!({"policy": policy, "trace": [{"tool": "any_tool"}]})
}

type ArgsFn = fn(&str) -> Value;

const ALL_TOOLS: &[(&str, ArgsFn)] = &[
    ("assay_policy_decide", policy_decide_args as ArgsFn),
    ("assay_check_args", check_args_args as ArgsFn),
    ("assay_check_coverage", check_coverage_args as ArgsFn),
    ("assay_explain_trace", explain_trace_args as ArgsFn),
];

// ── Independent sentinel positions ──────────────────────────────────────
// Three separate 200k-byte malformed YAML fixtures, each with exactly ONE
// sentinel at its actual beginning, midpoint, or end.  All three use
// syntax-error inputs (broken quoted scalar) so position is isolated from
// failure class.  Each sentinel test exercises all four tools.

#[test]
fn begin_sentinel_absent_from_syntax_error_all_tools() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    // Sentinel at the physical beginning of the hostile payload
    let mut content = b"version: \"\n  ".to_vec();
    content.extend(&payload_sentinel_at_begin(BEGIN_SENTINEL));
    write_policy(root, "begin.yaml", &content);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    for (i, (tool, args_fn)) in ALL_TOOLS.iter().enumerate() {
        let (result, body, full) = call_tool(&mut conn, tool, args_fn("begin.yaml"), i as u64 + 2);
        assert_parse_error(
            &format!("{tool}/begin"),
            &result,
            &body,
            &full,
            YAML_INVALID,
            root,
        );
    }
    let _ = conn.shutdown();
}

#[test]
fn middle_sentinel_absent_from_syntax_error_all_tools() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    // Sentinel at the physical midpoint of the hostile payload
    let mut content = b"version: \"\n  ".to_vec();
    content.extend(&payload_sentinel_at_middle(MIDDLE_SENTINEL));
    write_policy(root, "middle.yaml", &content);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    for (i, (tool, args_fn)) in ALL_TOOLS.iter().enumerate() {
        let (result, body, full) = call_tool(&mut conn, tool, args_fn("middle.yaml"), i as u64 + 2);
        assert_parse_error(
            &format!("{tool}/middle"),
            &result,
            &body,
            &full,
            YAML_INVALID,
            root,
        );
    }
    let _ = conn.shutdown();
}

#[test]
fn end_sentinel_absent_from_syntax_error_all_tools() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    // Sentinel at the physical end of the hostile payload
    let mut content = b"version: \"\n  ".to_vec();
    content.extend(&payload_sentinel_at_end(END_SENTINEL));
    write_policy(root, "end.yaml", &content);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    for (i, (tool, args_fn)) in ALL_TOOLS.iter().enumerate() {
        let (result, body, full) = call_tool(&mut conn, tool, args_fn("end.yaml"), i as u64 + 2);
        assert_parse_error(
            &format!("{tool}/end"),
            &result,
            &body,
            &full,
            YAML_INVALID,
            root,
        );
    }
    let _ = conn.shutdown();
}

// ── Non-mapping root sentinels (separate from syntax) ──────────────────

#[test]
fn root_sentinel_absent_from_non_mapping_error_all_tools() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    // Large quoted scalar root with sentinel at midpoint
    let mut content = b"\"".to_vec();
    content.extend(&payload_sentinel_at_middle(MIDDLE_SENTINEL));
    content.push(b'"');
    write_policy(root, "root-scalar.yaml", &content);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    for (i, (tool, args_fn)) in ALL_TOOLS.iter().enumerate() {
        let (result, body, full) =
            call_tool(&mut conn, tool, args_fn("root-scalar.yaml"), i as u64 + 2);
        assert_parse_error(
            &format!("{tool}/root-scalar"),
            &result,
            &body,
            &full,
            ROOT_NOT_MAPPING,
            root,
        );
    }
    let _ = conn.shutdown();
}

// ── Structure sentinel (policy_decide only, typed field shape) ──────────

#[test]
fn structure_sentinel_absent_from_policy_decide() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    // Mapping with blocklist as string → structure error; sentinel at end
    let mut content = b"blocklist: \"".to_vec();
    content.extend(&payload_sentinel_at_end(END_SENTINEL));
    content.extend(b"\"\n");
    write_policy(root, "struct-sentinel.yaml", &content);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let (result, body, full) = call_tool(
        &mut conn,
        "assay_policy_decide",
        policy_decide_args("struct-sentinel.yaml"),
        2,
    );
    assert_parse_error(
        "policy_decide/struct-sentinel",
        &result,
        &body,
        &full,
        STRUCTURE_INVALID,
        root,
    );
    let _ = conn.shutdown();
}

// ── Structure class for all four sinks (gap 5) ─────────────────────────

#[test]
fn structure_error_exact_for_check_coverage_and_explain_trace() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    // 'tools: 42' is a well-formed mapping with a typed field-shape error
    write_policy(root, "bad-struct.yaml", b"version: \"2.0\"\ntools: 42\n");

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    for (i, (tool, args_fn)) in [
        ("assay_check_coverage", check_coverage_args as ArgsFn),
        ("assay_explain_trace", explain_trace_args as ArgsFn),
    ]
    .iter()
    .enumerate()
    {
        let (result, body, full) =
            call_tool(&mut conn, tool, args_fn("bad-struct.yaml"), i as u64 + 2);
        assert_parse_error(
            &format!("{tool}/struct"),
            &result,
            &body,
            &full,
            STRUCTURE_INVALID,
            root,
        );
    }
    let _ = conn.shutdown();
}

#[test]
fn structure_error_exact_for_check_args() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    write_policy(root, "bad-struct.yaml", b"version: \"2.0\"\ntools: 42\n");

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let (result, body, full) = call_tool(
        &mut conn,
        "assay_check_args",
        check_args_args("bad-struct.yaml"),
        2,
    );
    assert_parse_error(
        "check_args/struct",
        &result,
        &body,
        &full,
        STRUCTURE_INVALID,
        root,
    );
    let _ = conn.shutdown();
}

// ── Syntax error requires positive numeric location (gap 1) ────────────

#[test]
fn syntax_error_requires_positive_numeric_location() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    write_policy(
        root,
        "bad-syntax.yaml",
        b"version: \"2.0\"\ntools:\n  allow:\n    - ok\nbad_line: [\n",
    );

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let (result, body, full) = call_tool(
        &mut conn,
        "assay_check_args",
        check_args_args("bad-syntax.yaml"),
        2,
    );

    assert_parse_error("syntax-location", &result, &body, &full, YAML_INVALID, root);

    // Require details.line and details.column as positive integers
    let details = body.pointer("/error/details");
    assert!(
        details.is_some(),
        "syntax error must provide details with line/column; body={body}"
    );
    let details = details.unwrap();
    let line = details.get("line");
    assert!(
        line.is_some(),
        "details must include 'line'; details={details}"
    );
    assert!(
        line.unwrap().as_u64().is_some_and(|v| v > 0),
        "details.line must be a positive integer, got {:?}",
        line.unwrap()
    );
    let col = details.get("column");
    assert!(
        col.is_some(),
        "details must include 'column'; details={details}"
    );
    assert!(
        col.unwrap().as_u64().is_some_and(|v| v > 0),
        "details.column must be a positive integer, got {:?}",
        col.unwrap()
    );

    let _ = conn.shutdown();
}

// ── Invalid UTF-8 (gap 7 — discriminating, not vacuous) ────────────────

#[test]
fn invalid_utf8_classifies_as_yaml_syntax_error() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    write_policy(root, "bad-utf8.yaml", &[0xFF, 0xFE, b'v', b':', b' ', b'1']);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let (result, body, full) = call_tool(
        &mut conn,
        "assay_check_args",
        check_args_args("bad-utf8.yaml"),
        2,
    );

    assert_parse_error("invalid-utf8", &result, &body, &full, YAML_INVALID, root);

    // Must not contain raw UTF-8 error diagnostic
    assert!(
        !full.contains("invalid utf-8") && !full.contains("Utf8Error"),
        "response must not contain raw UTF-8 diagnostic"
    );

    let _ = conn.shutdown();
}

// ── (C) Real stdio malformed multibyte policy ──────────────────────────
// Not a synthetic ToolError: exercises the full server path with a policy
// containing multibyte sequences that must not appear in the response.

const MULTIBYTE_SENTINEL: &str = "\u{1F512}MULTIBYTE_SECRET_2387\u{1F513}";

#[test]
fn malformed_multibyte_policy_returns_valid_utf8_with_fixed_summary() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    // Build a policy file containing multibyte sentinels inside a broken YAML scalar.
    // The server must return valid UTF-8 JSON with the fixed summary and no sentinel.
    let mut content = b"version: \"\n  ".to_vec();
    content.extend(MULTIBYTE_SENTINEL.as_bytes());
    // Pad to a decent size so truncation is exercised
    content.extend(std::iter::repeat_n(b'Y', 5000));
    write_policy(root, "multibyte.yaml", &content);

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let (result, body, full) = call_tool(
        &mut conn,
        "assay_check_args",
        check_args_args("multibyte.yaml"),
        2,
    );

    // The full response must parse as valid UTF-8 (it's a JSON string)
    assert!(
        std::str::from_utf8(full.as_bytes()).is_ok(),
        "full response must be valid UTF-8"
    );

    // Fixed summary
    assert_parse_error_with_policy(
        "multibyte",
        &result,
        &body,
        &full,
        YAML_INVALID,
        root,
        Some("multibyte.yaml"),
    );

    // The multibyte sentinel must not appear anywhere
    assert!(
        !full.contains(MULTIBYTE_SENTINEL),
        "response reflected multibyte sentinel"
    );

    let _ = conn.shutdown();
}

// ── (A) Path non-reflection with canonical and filename ────────────────

#[test]
fn path_non_reflection_canonical_and_filename() {
    let dir = tempfile::tempdir().expect("policy-root");
    let root = dir.path();
    write_policy(root, "audit-a.yaml", b"version: \"\nbad: [");

    let mut conn = spawn_server(root);
    initialize(&mut conn);
    let (result, body, full) = call_tool(
        &mut conn,
        "assay_check_args",
        check_args_args("audit-a.yaml"),
        2,
    );

    assert_parse_error_with_policy(
        "path-canonical",
        &result,
        &body,
        &full,
        YAML_INVALID,
        root,
        Some("audit-a.yaml"),
    );

    let _ = conn.shutdown();
}
