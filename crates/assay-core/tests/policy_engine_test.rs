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

#[test]
fn test_dual_shape_constraints() {
    // Canonical List
    let yaml_list = r#"
constraints:
  - tool: read_file
    params:
      path: { matches: "^/app/.*" }
"#;
    let p_list: McpPolicy = serde_yaml::from_str(yaml_list).unwrap();
    assert_eq!(p_list.constraints.len(), 1);
    assert_eq!(p_list.constraints[0].tool, "read_file");

    // Legacy Map
    let yaml_map = r#"
constraints:
  read_file:
    path: "^/app/.*"
"#;
    let p_map: McpPolicy = serde_yaml::from_str(yaml_map).unwrap();
    assert_eq!(p_map.constraints.len(), 1);
    assert_eq!(p_map.constraints[0].tool, "read_file");

    // Check normalization
    let rule = &p_map.constraints[0];
    let param = rule.params.get("path").expect("param missing");
    assert_eq!(param.matches.as_deref(), Some("^/app/.*"));
}

#[test]
fn test_mixed_tools_config() {
    // Root allow: ["*"], Nested deny: ["exec"]
    let yaml = r#"
allow: ["*"]
tools:
  deny: ["exec"]
"#;
    let mut p: McpPolicy = serde_yaml::from_str(yaml).expect("Refused mixed config");
    p.normalize_legacy_shapes();

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
    let mut p: McpPolicy = serde_yaml::from_str(yaml).unwrap();
    p.normalize_legacy_shapes();

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
    let mut p: McpPolicy = serde_yaml::from_str(yaml).unwrap();
    p.migrate_constraints_to_schemas();

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
