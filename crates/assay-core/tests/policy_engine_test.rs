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
    let p: McpPolicy = serde_yaml::from_str(yaml).expect("Refused mixed config");

    let mut state = PolicyState::default();

    let req_read = mock_request("read_file", json!({}));
    assert!(matches!(
        p.check(&req_read, &mut state),
        PolicyDecision::Allow
    ));

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
    let p: McpPolicy = serde_yaml::from_str(yaml).unwrap();
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
    assert!(matches!(p.check(&req, &mut state), PolicyDecision::Allow));
}

#[test]
fn test_constraint_enforcement() {
    let yaml = r#"
constraints:
  - tool: read_file
    params:
      path: { matches: "^/app/.*" }
"#;
    let p: McpPolicy = serde_yaml::from_str(yaml).unwrap();
    let mut state = PolicyState::default();

    // Pass
    let req = mock_request("read_file", json!({ "path": "/app/config.json" }));
    assert!(matches!(p.check(&req, &mut state), PolicyDecision::Allow));

    // Fail mismatch
    let req = mock_request("read_file", json!({ "path": "/etc/passwd" }));
    if let PolicyDecision::Deny { reason, .. } = p.check(&req, &mut state) {
        assert!(reason.contains("failed constraint"));
    } else {
        panic!("Should deny mismatch");
    }

    // Fail missing arg (Fail-Closed)
    let req = mock_request("read_file", json!({}));
    match p.check(&req, &mut state) {
        PolicyDecision::Deny { contract, .. } => {
            assert_eq!(contract["error_code"], "MCP_CONSTRAINT_MISSING");
        }
        _ => panic!("expected deny on missing constrained arg"),
    }
}
