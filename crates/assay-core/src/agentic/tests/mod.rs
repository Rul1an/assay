use super::*;
use serde_json::json;

#[test]
fn test_deduplication() {
    let diags = vec![
        Diagnostic::new("E_CFG_PARSE", "Error 1"),
        Diagnostic::new("E_CFG_PARSE", "Error 2"),
    ];
    let ctx = AgenticCtx {
        policy_path: None,
        config_path: None,
    };
    let (actions, patches) = build_suggestions(&diags, &ctx);

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id, "regen_config");
    assert!(patches.is_empty());
}

#[test]
fn test_unknown_tool_action_only() {
    let mut d = Diagnostic::new("UNKNOWN_TOOL", "Unknown tool");
    d.context = json!({ "tool": "weird-tool" });

    let diags = vec![d];
    let ctx = AgenticCtx {
        policy_path: None,
        config_path: None,
    };
    let (actions, patches) = build_suggestions(&diags, &ctx);

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].id, "fix_unknown_tool:weird-tool");
    assert!(
        patches.is_empty(),
        "UNKNOWN_TOOL should not generate patches"
    );
}

#[test]
fn test_rename_field_patch() {
    let mut d = Diagnostic::new("E_CFG_SCHEMA_UNKNOWN_FIELD", "Unknown field");
    d.context = json!({
        "file": "assay.yaml",
        "json_pointer_parent": "/config",
        "unknown_field": "policcy",
        "suggested_field": "policy"
    });

    let diags = vec![d];
    let ctx = AgenticCtx {
        policy_path: None,
        config_path: None,
    };
    let (_, patches) = build_suggestions(&diags, &ctx);

    assert_eq!(patches.len(), 1);
    let p = &patches[0];
    assert_eq!(p.id, "rename_field:policcy->policy");

    match &p.ops[0] {
        JsonPatchOp::Move { from, path } => {
            assert_eq!(from, "/config/policcy");
            assert_eq!(path, "/config/policy");
        }
        _ => panic!("Expected Move op"),
    }
}

#[test]
fn test_detect_policy_shape() {
    // Top Level
    let doc1: serde_yaml::Value = serde_yaml::from_str("allow: []\ndeny: []").unwrap();
    match super::policy_helpers::detect_policy_shape(&doc1) {
        super::policy_helpers::PolicyShape::TopLevel => {}
        _ => panic!("Expected TopLevel"),
    }

    // Tools Map (Legacy/Standard)
    let doc2: serde_yaml::Value = serde_yaml::from_str(
        r#"
tools:
  allow: ["read_file"]
  deny: []
"#,
    )
    .unwrap();
    match super::policy_helpers::detect_policy_shape(&doc2) {
        super::policy_helpers::PolicyShape::ToolsMap => {}
        _ => panic!("Expected ToolsMap"),
    }

    // Tools as explicit map (Bug regression check)
    // If tools is just a map of definitions, it should NOT be detected as ToolsMap
    // unless it has allow/deny sequences.
    let doc3: serde_yaml::Value = serde_yaml::from_str(
        r#"
tools:
  my-tool:
    image: python:3.9
"#,
    )
    .unwrap();
    match super::policy_helpers::detect_policy_shape(&doc3) {
        super::policy_helpers::PolicyShape::TopLevel => {}
        _ => panic!("Expected TopLevel for tools definition map"),
    }
}

#[test]
fn test_tool_poisoning_action_uses_assay_config_not_policy() {
    let mut d = Diagnostic::new("E_TOOL_DESC_SUSPICIOUS", "Suspicious tool description");
    d.context = json!({
        "policy_file": "policy.yaml",
        "config_file": "assay.yaml"
    });

    let diags = vec![d];
    let ctx = AgenticCtx {
        policy_path: None,
        config_path: None,
    };
    let (actions, _patches) = build_suggestions(&diags, &ctx);

    let a = actions
        .iter()
        .find(|a| a.id == "enable_tool_poisoning_checks")
        .expect("expected enable_tool_poisoning_checks action");

    assert_eq!(a.command[0], "assay");
    assert_eq!(a.command[1], "fix");
    assert_eq!(a.command[2], "--config");
    assert_eq!(a.command[3], "assay.yaml");
}
