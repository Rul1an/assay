use assay_core::mcp::jsonrpc::JsonRpcRequest;
use assay_core::mcp::policy::{McpPolicy, PolicyDecision, PolicyState};
use serde_json::json;

fn mock_request(tool: &str, args: serde_json::Value) -> JsonRpcRequest {
    JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "tools/call".to_string(),
        params: json!({
            "name": tool,
            "arguments": args
        }),
        id: Some(json!(1)),
    }
}

fn migrate(text: &str) -> McpPolicy {
    let converted = McpPolicy::migrate_yaml(text.as_bytes()).unwrap();
    McpPolicy::from_slice(converted.as_bytes()).unwrap()
}

#[test]
fn test_dual_shape_constraints() {
    // Canonical List
    let yaml_list = r#"
constraints:
  - tool: read_file
    params:
      path: { matches: "^/app/.*" }
"#;
    let p_list = migrate(yaml_list);

    // Legacy Map
    let yaml_map = r#"
constraints:
  read_file:
    path: "^/app/.*"
"#;
    let p_map = migrate(yaml_map);
    assert_eq!(p_list.schemas, p_map.schemas);
    assert_eq!(
        p_map.schemas["read_file"]["properties"]["path"]["pattern"],
        "^/app/.*"
    );
}

#[test]
fn test_mixed_tools_config() {
    // Root allow: ["*"], Nested deny: ["exec"]
    let yaml = r#"
allow: ["*"]
tools:
  deny: ["exec"]
"#;
    let p = migrate(yaml);

    let mut state = PolicyState::default();

    let req_read = mock_request("read_file", json!({}));
    // Should be AllowWithWarning because read_file has no schema
    match p.check(&req_read, &mut state) {
        PolicyDecision::Allow | PolicyDecision::AllowWithWarning { .. } => {}
        d => panic!("Expected Allow/Warning, got {:?}", d),
    }

    let req_exec = mock_request("exec", json!({}));
    if let PolicyDecision::Deny { reason, .. } = p.check(&req_exec, &mut state) {
        assert!(reason.to_lowercase().contains("denylisted"));
    } else {
        panic!("Checking exec should result in Deny");
    }
}

#[test]
fn test_wildcard_semantics() {
    let yaml = r#"
deny:
  - "exec*"
  - "*sh"
  - "*kill*"
"#;
    let p = migrate(yaml);

    let mut state = PolicyState::default();

    // exec*
    let req = mock_request("execute_command", json!({}));
    assert!(matches!(
        p.check(&req, &mut state),
        PolicyDecision::Deny { .. }
    ));

    // *sh
    let req = mock_request("zsh", json!({}));
    assert!(matches!(
        p.check(&req, &mut state),
        PolicyDecision::Deny { .. }
    ));

    // *kill* (contains)
    let req = mock_request("skill_check", json!({}));
    assert!(matches!(
        p.check(&req, &mut state),
        PolicyDecision::Deny { .. }
    ));

    // No match
    let req = mock_request("read_file", json!({}));
    match p.check(&req, &mut state) {
        PolicyDecision::Allow | PolicyDecision::AllowWithWarning { .. } => {}
        d => panic!("Expected Allow/Warning, got {:?}", d),
    }
}

#[test]
fn test_constraint_enforcement() {
    let yaml = r#"
constraints:
  - tool: read_file
    params:
      path: { matches: "^/app/.*" }
"#;
    let p = migrate(yaml);

    let mut state = PolicyState::default();

    // Pass
    let req = mock_request("read_file", json!({ "path": "/app/config.json" }));
    match p.check(&req, &mut state) {
        PolicyDecision::Allow => {} // Exact allow because schema exists and validates!
        d => panic!("Expected Allow, got {:?}", d),
    }

    // Fail mismatch
    let req = mock_request("read_file", json!({ "path": "/etc/passwd" }));
    if let PolicyDecision::Deny {
        reason, contract, ..
    } = p.check(&req, &mut state)
    {
        assert!(reason.contains("JSON Schema validation failed"));
        assert_eq!(contract["error_code"], "E_ARG_SCHEMA");
    } else {
        panic!("Should deny mismatch");
    }

    // Fail missing arg (Fail-Closed)
    // V2 auto-migrates constraints to "required" properties in generic migration logic
    let req = mock_request("read_file", json!({}));
    match p.check(&req, &mut state) {
        PolicyDecision::Deny { contract, .. } => {
            // New V2 Error Code
            assert_eq!(contract["error_code"], "E_ARG_SCHEMA");
        }
        _ => panic!("expected deny on missing constrained arg"),
    }
}

#[test]
fn migrated_policy_reloads_with_the_effective_legacy_schemas_and_tools() {
    use assay_core::mcp::policy::McpPolicy;
    for fragment in [
        "tools: !example {}\nallow: [Read]\n",
        "tools: !example null\nallow: [Read]\n",
        "tools: !example\nallow: [Read]\n",
        "schemas: !example null\nconstraints: [{tool: Search, params: {q: {matches: x}}}]\n",
        "schemas: {Search: {type: string}, !label Search: {type: integer}}\nconstraints: [{tool: Search, params: {q: {matches: x}}}]\n",
    ] {
        let bytes = fragment.as_bytes();
        let yaml = McpPolicy::migrate_yaml(bytes).unwrap_or_else(|e| panic!("{fragment}: {e:#}"));
        let migrated = McpPolicy::from_slice(yaml.as_bytes()).unwrap();
        if fragment.starts_with("tools:") {
            assert_eq!(migrated.tools.allow.as_deref(), Some(&["Read".to_owned()][..]));
        } else {
            assert_eq!(migrated.schemas["Search"].pointer("/properties/q/pattern"), Some(&json!("x")));
        }
    }
}

#[test]
fn legacy_mcp_policy_inputs_are_exclusive_to_the_converter() {
    use assay_core::mcp::policy::McpPolicy;
    for (key, text) in [
        ("allow", "allow: []"),
        ("deny", "deny: null"),
        ("constraints", "constraints: []"),
        ("version", "version: '1.0'"),
    ] {
        let current = McpPolicy::migrate_yaml(text.as_bytes()).unwrap();
        for (route, result) in [
            (
                "enforce",
                McpPolicy::from_slice(text.as_bytes()).map(|_| ()),
            ),
            (
                "with source",
                McpPolicy::from_slice_with_source(text.as_bytes()).map(|_| ()),
            ),
            ("coverage", McpPolicy::load_for_coverage(text).map(|_| ())),
        ] {
            let error = result.expect_err(&format!("{route} accepted legacy {key}"));
            assert!(error.to_string().contains(key), "{route}: {error:#}");
        }
        McpPolicy::from_slice(current.as_bytes()).unwrap();
        McpPolicy::from_slice_with_source(current.as_bytes()).unwrap();
        McpPolicy::load_for_coverage(&current).unwrap();
    }
}
