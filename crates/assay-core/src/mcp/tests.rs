use super::policy::*;
use serde_json::{json, Value};
use std::collections::HashMap;

// ── Full-policy error classification tests ──────────────────────────────────

#[test]
fn policy_error_classification_syntax() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"version: \"\n  bad: [").unwrap();
    let err = McpPolicy::from_file(tmp.path()).unwrap_err();
    let typed = err.downcast_ref::<McpPolicyError>();
    assert!(
        typed.is_some(),
        "syntax error must be McpPolicyError, got: {err}"
    );
    assert!(
        matches!(typed.unwrap().kind, McpPolicyErrorKind::Syntax { .. }),
        "syntax error must be Syntax kind, got: {:?}",
        typed.unwrap().kind
    );
}

#[test]
fn policy_error_classification_root_not_mapping() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"- item1\n- item2\n").unwrap();
    let err = McpPolicy::from_file(tmp.path()).unwrap_err();
    let typed = err.downcast_ref::<McpPolicyError>();
    assert!(
        typed.is_some(),
        "root-not-mapping must be McpPolicyError, got: {err}"
    );
    assert!(
        matches!(typed.unwrap().kind, McpPolicyErrorKind::RootNotMapping),
        "root-not-mapping must be RootNotMapping kind, got: {:?}",
        typed.unwrap().kind
    );
}

#[test]
fn policy_error_classification_structure() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    // A mapping where 'tools' has the wrong shape (number instead of object)
    std::fs::write(tmp.path(), b"version: \"2.0\"\ntools: 42\n").unwrap();
    let err = McpPolicy::from_file(tmp.path()).unwrap_err();
    let typed = err.downcast_ref::<McpPolicyError>();
    assert!(
        typed.is_some(),
        "structure error must be McpPolicyError, got: {err}"
    );
    assert!(
        matches!(typed.unwrap().kind, McpPolicyErrorKind::Structure),
        "structure error must be Structure kind, got: {:?}",
        typed.unwrap().kind
    );
}

#[test]
fn policy_error_classification_invalid_utf8() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), [0xFF, 0xFE, b'v', b':', b' ', b'1']).unwrap();
    let err = McpPolicy::from_file(tmp.path()).unwrap_err();
    let typed = err.downcast_ref::<McpPolicyError>();
    assert!(
        typed.is_some(),
        "invalid UTF-8 must be McpPolicyError, got: {err}"
    );
    assert!(
        matches!(typed.unwrap().kind, McpPolicyErrorKind::Syntax { .. }),
        "invalid UTF-8 must be Syntax kind, got: {:?}",
        typed.unwrap().kind
    );
}

// ── Validation kind (gap 4) ──────────────────────────────────────────────

#[test]
fn policy_error_classification_validation() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    // Valid YAML mapping, valid McpPolicy shape, but bad pin hash → validation error
    std::fs::write(
        tmp.path(),
        b"version: \"2.0\"\ntool_pins:\n  test_tool:\n    schema_hash: \"bad\"\n    meta_hash: \"bad\"\n    server_id: s\n    tool_name: n\n",
    )
    .unwrap();
    let err = McpPolicy::from_file(tmp.path()).unwrap_err();
    let typed = err.downcast_ref::<McpPolicyError>();
    assert!(
        typed.is_some(),
        "validation error must be McpPolicyError, got: {err}"
    );
    assert!(
        matches!(typed.unwrap().kind, McpPolicyErrorKind::Validation),
        "validation error must be Validation kind, got: {:?}",
        typed.unwrap().kind
    );
}

// ── Scalar root classification ──────────────────────────────────────────

#[test]
fn policy_error_classification_scalar_root() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"just_a_string\n").unwrap();
    let err = McpPolicy::from_file(tmp.path()).unwrap_err();
    let typed = err.downcast_ref::<McpPolicyError>();
    assert!(
        typed.is_some(),
        "scalar root must be McpPolicyError, got: {err}"
    );
    assert!(
        matches!(typed.unwrap().kind, McpPolicyErrorKind::RootNotMapping),
        "scalar root must be RootNotMapping kind, got: {:?}",
        typed.unwrap().kind
    );
}

// ── Full-policy parser contract: successful parse, warnings, V1 migration ──

#[test]
fn policy_file_parser_contract_v2_success() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        b"version: \"2.0\"\nname: test\ntools:\n  allow:\n    - read_file\n",
    )
    .unwrap();
    let policy = McpPolicy::from_file(tmp.path()).unwrap();
    assert_eq!(policy.version, "2.0");
    assert!(policy
        .tools
        .allow
        .as_ref()
        .unwrap()
        .contains(&"read_file".to_string()));
}

#[test]
#[serial_test::serial]
#[allow(unsafe_code)]
fn policy_file_parser_contract_v1_legacy_normalization() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        b"version: \"1.0\"\nallow:\n  - tool_a\ndeny:\n  - tool_b\n",
    )
    .unwrap();
    // Remove strict deprecations for this test
    unsafe { std::env::remove_var("ASSAY_STRICT_DEPRECATIONS") };
    let policy = McpPolicy::from_file(tmp.path()).unwrap();
    // Root allow/deny must be normalized into tools.*
    assert!(policy
        .tools
        .allow
        .as_ref()
        .unwrap()
        .contains(&"tool_a".to_string()));
    assert!(policy
        .tools
        .deny
        .as_ref()
        .unwrap()
        .contains(&"tool_b".to_string()));
    // Root-level fields consumed
    assert!(policy.allow.is_none());
    assert!(policy.deny.is_none());
}

#[test]
fn policy_file_parser_contract_validation_error() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    // Bad pin hash (not 64 hex chars)
    std::fs::write(
        tmp.path(),
        b"version: \"2.0\"\ntool_pins:\n  test_tool:\n    schema_hash: \"bad\"\n    meta_hash: \"bad\"\n    server_id: s\n    tool_name: n\n",
    )
    .unwrap();
    let err = McpPolicy::from_file(tmp.path()).unwrap_err();
    assert!(
        err.to_string().contains("hexadecimal"),
        "validation error should mention hex: {err}"
    );
}

#[test]
#[serial_test::serial]
#[allow(unsafe_code)]
fn policy_file_parser_contract_v1_constraints_migration() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        tmp.path(),
        b"version: \"1.0\"\nconstraints:\n  - tool: read_file\n    params:\n      path:\n        matches: \"^/safe/.*\"\n",
    )
    .unwrap();
    unsafe { std::env::remove_var("ASSAY_STRICT_DEPRECATIONS") };
    let policy = McpPolicy::from_file(tmp.path()).unwrap();
    // Constraints must be auto-migrated to schemas
    assert!(
        policy.schemas.contains_key("read_file"),
        "V1 constraints must auto-migrate to schemas"
    );
    let schema = policy.schemas.get("read_file").unwrap();
    let pattern = schema
        .pointer("/properties/path/pattern")
        .and_then(|v| v.as_str());
    assert_eq!(pattern, Some("^/safe/.*"), "migrated pattern mismatch");
}

#[test]
#[serial_test::serial]
#[allow(unsafe_code)]
fn policy_file_parser_contract_strict_deprecations_rejects_v1() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"version: \"1.0\"\nconstraints: []\n").unwrap();
    unsafe { std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1") };
    let err = McpPolicy::from_file(tmp.path());
    assert!(err.is_err(), "strict deprecations must reject v1");
    assert!(
        err.unwrap_err().to_string().contains("Strict mode"),
        "error must mention Strict mode"
    );
    unsafe { std::env::remove_var("ASSAY_STRICT_DEPRECATIONS") };
}

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
    let decision = policy.evaluate("read_file", &args, &mut state, None);

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
    let decision = policy.evaluate("read_file", &args, &mut state, None);

    if let PolicyDecision::Deny { code, .. } = decision {
        assert_eq!(code, "E_ARG_SCHEMA");
    } else {
        panic!("Expected Deny, got {:?}", decision);
    }

    // Violation: missing property
    let args_missing = json!({});
    let decision_missing = policy.evaluate("read_file", &args_missing, &mut state, None);
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
        policy.evaluate("read_file", &args_ok, &mut state, None),
        PolicyDecision::Allow
    );

    let args_bad = json!({ "path": "/unsafe/file" });
    match policy.evaluate("read_file", &args_bad, &mut state, None) {
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
    let decision = policy.evaluate("unknown_tool", &json!({}), &mut state, None);
    if let PolicyDecision::AllowWithWarning { code, .. } = decision {
        assert_eq!(code, "E_TOOL_UNCONSTRAINED");
    } else {
        panic!("Expected AllowWithWarning, got {:?}", decision);
    }

    // Change to Deny
    policy.enforcement.unconstrained_tools = UnconstrainedMode::Deny;
    let decision_deny = policy.evaluate("unknown_tool", &json!({}), &mut state, None);
    if let PolicyDecision::Deny { code, .. } = decision_deny {
        assert_eq!(code, "E_TOOL_UNCONSTRAINED");
    } else {
        panic!("Expected Deny, got {:?}", decision_deny);
    }

    // Change to Allow
    policy.enforcement.unconstrained_tools = UnconstrainedMode::Allow;
    let decision_allow = policy.evaluate("unknown_tool", &json!({}), &mut state, None);
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
        policy.evaluate("refined_tool", &args_ok, &mut state, None),
        PolicyDecision::Allow
    );

    // Invalid
    let args_bad = json!({ "path": "/unsafe/bad" });
    if let PolicyDecision::Deny { code, .. } =
        policy.evaluate("refined_tool", &args_bad, &mut state, None)
    {
        assert_eq!(code, "E_ARG_SCHEMA");
    } else {
        panic!("Expected Deny for ref violation");
    }
}
#[test]
fn test_tool_integrity_drift() {
    use crate::mcp::identity::ToolIdentity;
    let mut policy = McpPolicy::default();
    let tool_name = "test_tool";
    let pinned_id = ToolIdentity::new("srv1", tool_name, &None, &Some("old desc".into()));
    let runtime_id = ToolIdentity::new("srv1", tool_name, &None, &Some("new desc".into()));

    policy
        .tool_pins
        .insert(tool_name.to_string(), pinned_id.clone());
    let mut state = PolicyState::default();

    // Case 1: Match -> Allow (assuming no schema)
    let tool_args = &json!({});
    let decision = policy.evaluate(tool_name, tool_args, &mut state, None);
    assert!(matches!(decision, PolicyDecision::AllowWithWarning { .. }));

    // Case 2: Mismatch -> Deny
    let decision_fail = policy.evaluate(tool_name, &json!({}), &mut state, Some(&runtime_id));
    if let PolicyDecision::Deny { code, .. } = decision_fail {
        assert_eq!(code, "E_TOOL_DRIFT");
    } else {
        panic!("Expected E_TOOL_DRIFT, got {:?}", decision_fail);
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

#[test]
#[serial_test::serial]
#[allow(unsafe_code)]
fn test_strict_deprecation_env_var() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.path();
    std::fs::write(path, "version: '1.0'\nconstraints: []").unwrap();

    // Case 1: No env var -> OK (but warns)
    unsafe {
        std::env::remove_var("ASSAY_STRICT_DEPRECATIONS");
    }
    let res = McpPolicy::from_file(path);
    assert!(res.is_ok());

    // Case 2: Env var set -> Error
    unsafe {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }
    let res_strict = McpPolicy::from_file(path);
    assert!(res_strict.is_err());
    assert!(res_strict.unwrap_err().to_string().contains("Strict mode"));

    // Cleanup
    unsafe {
        std::env::remove_var("ASSAY_STRICT_DEPRECATIONS");
    }
}
