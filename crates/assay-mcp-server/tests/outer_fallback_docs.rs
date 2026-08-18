//! Docs must describe the shipped outer-fallback contracts, not the former ones.

#[test]
fn mcp_api_states_invalid_json_lines_are_ignored() {
    let doc = include_str!("../../../docs/reference/mcp-api.md");
    assert!(
        !doc.contains("Protocol-level errors (invalid JSON) return a JSON-RPC `error`."),
        "mcp-api.md still claims a JSON-RPC error for invalid JSON"
    );
    assert!(
        doc.contains("invalid JSON lines are ignored") && doc.contains("no JSON-RPC response"),
        "mcp-api.md must state the ignore-and-continue contract"
    );
}

#[test]
fn mcp_api_states_malformed_arguments_are_fixed_e_internal() {
    let doc = include_str!("../../../docs/reference/mcp-api.md");
    assert!(doc.contains("E_INTERNAL"));
    assert!(doc.contains("missing or malformed tool arguments"));
}

#[test]
fn fail_safe_does_not_imply_a_configured_mcp_failure_policy() {
    let doc = include_str!("../../../docs/concepts/fail-safe.md");
    assert!(
        !doc.contains("Verify that the configured failure policy was applied"),
        "audit-trail still points at a removed MCP failure policy"
    );
    let mcp = doc
        .split("### In the stdio MCP server")
        .nth(1)
        .expect("MCP subsection");
    assert!(mcp.contains("Caller-supplied `arguments.on_error` has no authority"));
}

#[test]
fn changelog_names_the_tool_execution_error_rename() {
    let log = include_str!("../../../CHANGELOG.md");
    let unreleased = log.split("## [5.3.0]").next().expect("Unreleased preface");
    assert!(unreleased.contains("tool_call_crash"));
    assert!(unreleased.contains("tool_execution_error"));
}
