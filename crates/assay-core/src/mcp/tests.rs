use super::policy::*;
use serde_json::{json, Value};
use std::collections::HashMap;

fn create_v2_policy(schemas: HashMap<String, Value>) -> McpPolicy {
    McpPolicy {
        version: "2.0".to_string(),
        schemas,
        enforcement: EnforcementSettings::default(),
        ..Default::default()
    }
}

#[test]
fn test_v2_schema_validation_allow() {
    let mut schemas = HashMap::new();
    schemas.insert(
        "read_file".to_string(),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "pattern": "^/safe/.*" }
            },
            "required": ["path"]
        }),
    );
    let policy = create_v2_policy(schemas);
    let mut state = PolicyState::default();

    let args = json!({ "path": "/safe/test.txt" });
    let decision = policy.evaluate("read_file", &args, &mut state);

    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn test_v2_schema_validation_deny() {
    let mut schemas = HashMap::new();
    schemas.insert(
        "read_file".to_string(),
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "pattern": "^/safe/.*" }
            },
            "required": ["path"]
        }),
    );
    let policy = create_v2_policy(schemas);
    let mut state = PolicyState::default();

    // Violation: path does not match pattern
    let args = json!({ "path": "/unsafe/hack.sh" });
    let decision = policy.evaluate("read_file", &args, &mut state);

    if let PolicyDecision::Deny { code, .. } = decision {
        assert_eq!(code, "E_ARG_SCHEMA");
    } else {
        panic!("Expected Deny, got {:?}", decision);
    }

    // Violation: missing property
    let args_missing = json!({});
    let decision_missing = policy.evaluate("read_file", &args_missing, &mut state);
    if let PolicyDecision::Deny { code, .. } = decision_missing {
        assert_eq!(code, "E_ARG_SCHEMA");
    } else {
        panic!("Expected Deny for missing arg, got {:?}", decision_missing);
    }
}

#[test]
fn test_v1_migration_correctness() {
    let yaml = r#"
version: "1.0"
constraints:
  - tool: read_file
    params:
      path:
        matches: "^/safe/.*"
"#;

    let mut policy: McpPolicy = serde_yaml::from_str(yaml).unwrap();
    // This method is now public
    policy.migrate_constraints_to_schemas();

    // Verify schema was created
    assert!(policy.schemas.contains_key("read_file"));
    let schema = policy.schemas.get("read_file").unwrap();

    // Check schema structure: { "type": "object", "properties": { "path": { "pattern": ... } }, "required": ["path"] }
    let path_pattern = schema
        .get("properties")
        .and_then(|p| p.get("path"))
        .and_then(|p| p.get("pattern"))
        .and_then(|v| v.as_str())
        .expect("Missing pattern in migrated schema");

    assert_eq!(path_pattern, "^/safe/.*");

    let required = schema
        .get("required")
        .and_then(|v| v.as_array())
        .expect("Missing required array");

    assert!(required.iter().any(|v| v.as_str() == Some("path")));

    // Test evaluation against migrated policy
    let mut state = PolicyState::default();
    let args_ok = json!({ "path": "/safe/file" });
    assert_eq!(
        policy.evaluate("read_file", &args_ok, &mut state),
        PolicyDecision::Allow
    );

    let args_bad = json!({ "path": "/unsafe/file" });
    match policy.evaluate("read_file", &args_bad, &mut state) {
        PolicyDecision::Deny { code, .. } => assert_eq!(code, "E_ARG_SCHEMA"),
        _ => panic!("Migrated policy failed to deny invalid arg"),
    }
}

#[test]
fn test_enforcement_modes() {
    let mut policy = McpPolicy::default();
    policy.enforcement.unconstrained_tools = UnconstrainedMode::Warn;
    let mut state = PolicyState::default();

    // No schema for "unknown_tool"
    let decision = policy.evaluate("unknown_tool", &json!({}), &mut state);
    if let PolicyDecision::AllowWithWarning { code, .. } = decision {
        assert_eq!(code, "E_TOOL_UNCONSTRAINED");
    } else {
        panic!("Expected AllowWithWarning, got {:?}", decision);
    }

    // Change to Deny
    policy.enforcement.unconstrained_tools = UnconstrainedMode::Deny;
    let decision_deny = policy.evaluate("unknown_tool", &json!({}), &mut state);
    if let PolicyDecision::Deny { code, .. } = decision_deny {
        assert_eq!(code, "E_TOOL_UNCONSTRAINED");
    } else {
        panic!("Expected Deny, got {:?}", decision_deny);
    }

    // Change to Allow
    policy.enforcement.unconstrained_tools = UnconstrainedMode::Allow;
    let decision_allow = policy.evaluate("unknown_tool", &json!({}), &mut state);
    assert_eq!(decision_allow, PolicyDecision::Allow);
}

#[test]
fn test_defs_resolution() {
    // Test that $refs work using inline $defs
    let mut schemas = HashMap::new();

    // Root definitions
    let defs = json!({
        "path_pattern": { "type": "string", "pattern": "^/safe/.*" }
    });
    schemas.insert("$defs".to_string(), defs);

    // Tool schema using ref
    let tool_schema = json!({
        "type": "object",
        "properties": {
            "path": { "$ref": "#/$defs/path_pattern" }
        },
        "required": ["path"]
    });
    schemas.insert("refined_tool".to_string(), tool_schema);

    let policy = create_v2_policy(schemas);
    let mut state = PolicyState::default();

    // Valid
    let args_ok = json!({ "path": "/safe/ok" });
    assert_eq!(
        policy.evaluate("refined_tool", &args_ok, &mut state),
        PolicyDecision::Allow
    );

    // Invalid
    let args_bad = json!({ "path": "/unsafe/bad" });
    if let PolicyDecision::Deny { code, .. } =
        policy.evaluate("refined_tool", &args_bad, &mut state)
    {
        assert_eq!(code, "E_ARG_SCHEMA");
    } else {
        panic!("Expected Deny for ref violation");
    }
}

#[test]
fn test_is_v1_format() {
    // V1 Explicit
    let v1 = McpPolicy {
        version: "1.0".to_string(),
        ..Default::default()
    };
    assert!(v1.is_v1_format());

    // V1 Implied by constraints
    let v1_implied = McpPolicy {
        constraints: vec![ConstraintRule {
            tool: "t".into(),
            params: std::collections::BTreeMap::new(),
        }],
        ..Default::default()
    };
    assert!(v1_implied.is_v1_format());

    // V2
    let v2 = McpPolicy {
        version: "2.0".to_string(),
        ..Default::default()
    };
    assert!(!v2.is_v1_format());

    let empty = McpPolicy::default();
    assert!(!empty.is_v1_format());
}
