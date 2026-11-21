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

#[test]
fn policy_suggestions_use_the_policy_shape_and_contents() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    let ctx = AgenticCtx {
        policy_path: Some(path.clone()),
        config_path: None,
    };
    let diags = ["E_TOOL_NOT_ALLOWED", "E_TOOL_DENIED"].map(|code| {
        let mut diagnostic = Diagnostic::new(code, "tool");
        diagnostic.context = json!({"tool": "read"});
        diagnostic
    });

    for source in [
        None,
        Some("allow: [read]"),
        Some("tools: ["),
        Some("tools: {my_tool: {image: python}}"),
    ] {
        if let Some(text) = source {
            std::fs::write(&path, text).unwrap();
        }
        let (actions, patches) = build_suggestions(&diags, &ctx);
        assert!(patches.is_empty(), "{source:?}");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].command.last().unwrap(), path.to_str().unwrap());
    }
    for (document, pointer) in [
        (
            "tools: {allow: ['*'], deny: [other, read]}",
            "/tools/deny/1",
        ),
        (
            "x-names: &names [other, read]\ntools: {allow: ['*'], deny: *names}",
            "/tools/deny/1",
        ),
    ] {
        std::fs::write(&path, document).unwrap();
        let (_, patches) = build_suggestions(&diags, &ctx);
        assert_eq!(patches.len(), 1);
        assert!(matches!(&patches[0].ops[..], [JsonPatchOp::Remove { path }] if path == pointer));
        let patched = crate::fix::apply_ops_to_text(document, &patches[0].ops, false).unwrap();
        let value: serde_json::Value = serde_yaml::from_str(&patched).unwrap();
        let (parent, _) = pointer.rsplit_once('/').unwrap();
        assert_eq!(value.pointer(parent), Some(&json!(["other"])));
        if document.contains("x-names") {
            assert_eq!(value["x-names"], json!(["other", "read"]));
        }
    }
}

#[test]
fn allow_suggestion_matches_the_current_v2_allowlist() {
    use crate::mcp::policy::{McpPolicy, PolicyDecision, PolicyState};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    let ctx = AgenticCtx {
        policy_path: Some(path.clone()),
        config_path: None,
    };
    let mut diagnostic = Diagnostic::new("E_TOOL_NOT_ALLOWED", "tool");
    diagnostic.context = json!({"tool":"read"});
    for (section, propose) in [
        ("", false),
        ("tools: {}", false),
        ("tools: {deny: [remove]}", false),
        ("tools: {allow_classes: []}", true),
        ("tools: {allow: ['r*']}", false),
        ("tools: {allow: [write], deny: [read]}", false),
        ("tools: {allow: [write]}", true),
    ] {
        let text = format!("version: '2.0'\n{section}");
        let verdict = |source: &str, tool| {
            let policy = McpPolicy::from_slice(source.as_bytes()).unwrap();
            match policy.evaluate(tool, &json!({}), &mut PolicyState::default(), None) {
                PolicyDecision::Deny { code, .. } => Some(code),
                _ => None,
            }
        };
        assert_eq!(
            verdict(&text, "read").as_deref() == Some("E_TOOL_NOT_ALLOWED"),
            propose,
            "{text}"
        );
        std::fs::write(&path, &text).unwrap();
        let (_, patches) = build_suggestions(std::slice::from_ref(&diagnostic), &ctx);
        assert_eq!(patches.len(), usize::from(propose), "{text}");
        if propose {
            let p = &patches[0];
            assert_eq!(p.risk, RiskLevel::High);
            let repaired = crate::fix::apply_ops_to_text(&text, &p.ops, false).unwrap();
            assert_ne!(
                verdict(&repaired, "read").as_deref(),
                Some("E_TOOL_NOT_ALLOWED")
            );
            for other in ["write", "other"] {
                assert_eq!(verdict(&text, other), verdict(&repaired, other));
            }
        }
    }
}

#[test]
fn constraint_diagnostics_do_not_invent_a_v1_rule() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    let ctx = AgenticCtx {
        policy_path: Some(path.clone()),
        config_path: None,
    };
    std::fs::write(&path, "version: '2.0'\nschemas: {read: {type: object}}").unwrap();
    for code in [
        "E_PATH_SCOPE_VIOLATION",
        "E_ARG_PATTERN_BLOCKED",
        "E_CONSTRAINT_MISSING",
    ] {
        for context in [
            json!({}),
            json!({"tool":"read","param":"path","recommended_matches":"^/tmp/"}),
        ] {
            let mut diagnostic = Diagnostic::new(code, "argument");
            diagnostic.context = context;
            let (actions, patches) = build_suggestions(&[diagnostic], &ctx);
            assert!(patches.is_empty(), "{code}: {patches:?}");
            assert_eq!(actions.len(), 1);
            assert!(actions[0].title.contains("schema"));
            assert_eq!(actions[0].command.last().unwrap(), path.to_str().unwrap());
        }
    }
}

#[test]
fn unsafe_or_ineffective_list_edits_are_not_offered() {
    let dir = tempfile::tempdir().unwrap();
    for (name, body, code) in [
        (
            "policy.yaml",
            "!policy\nversion: '2.0'\ntools: {allow: []}",
            "E_TOOL_NOT_ALLOWED",
        ),
        (
            "policy.yaml",
            "version: '2.0'\nx-meta: {1: first, '1': second}\ntools: {deny: [read]}",
            "E_TOOL_DENIED",
        ),
        (
            "policy.json",
            "version: '2.0'\ntools: {allow: []}",
            "E_TOOL_NOT_ALLOWED",
        ),
        (
            "policy.yaml",
            "tools: {deny: [read, 'r*']}",
            "E_TOOL_DENIED",
        ),
        (
            "policy.yaml",
            "tools: {deny: [read, read]}",
            "E_TOOL_DENIED",
        ),
        ("policy.yaml", "tools: {allow: ['*']}", "E_TOOL_DENIED"),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        let mut diagnostic = Diagnostic::new(code, "tool");
        diagnostic.context = json!({"tool":"read"});
        let (actions, patches) = build_suggestions(
            &[diagnostic],
            &AgenticCtx {
                policy_path: Some(path.clone()),
                config_path: None,
            },
        );
        assert!(patches.is_empty(), "{body}: {patches:?}");
        assert!(
            !actions
                .iter()
                .any(|action| action.title.starts_with("Manually remove")),
            "{body}"
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), body);
    }
}

#[test]
fn one_policy_edit_can_be_applied_before_replanning_the_remaining_diagnostics() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.yaml");
    let ctx = AgenticCtx {
        policy_path: Some(path.clone()),
        config_path: None,
    };
    let source = "tools: {deny: [a, b, keep]}";
    std::fs::write(&path, source).unwrap();
    let diags = ["a", "b"].map(|tool| {
        let mut d = Diagnostic::new("E_TOOL_DENIED", tool);
        d.context = json!({"tool":tool});
        d
    });
    for _ in 0..2 {
        let (_, patches) = build_suggestions(&diags, &ctx);
        assert_eq!(patches.len(), 1);
        crate::fix::apply_ops_to_file(&path, &patches[0].ops).unwrap();
    }
    let doc = crate::mcp::policy::McpPolicy::from_file(&path).unwrap();
    assert_eq!(doc.tools.deny.unwrap(), ["keep"]);
}

#[test]
fn json_policy_suggestions_preserve_values_outside_the_tool_list() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.JSON");
    let ctx = AgenticCtx {
        policy_path: Some(path.clone()),
        config_path: None,
    };
    for (extension, ordinary) in [
        (r#"{"label":"keep"}"#, true),
        (r#"{"$serde_json::private::Number":"37"}"#, false),
    ] {
        let source = format!(r#"{{"version":"2.0","tools":{{"allow":[]}},"x-data":{extension}}}"#);
        std::fs::write(&path, &source).unwrap();
        let mut diagnostic = Diagnostic::new("E_TOOL_NOT_ALLOWED", "read");
        diagnostic.context = json!({"tool":"read"});
        let (_, patches) = build_suggestions(&[diagnostic], &ctx);
        if ordinary {
            assert_eq!(patches.len(), 1);
        }
        // Feature unification determines whether the writer treats the number marker as data.
        if let Some(patch) = patches.first() {
            let written = crate::fix::apply_ops_to_file(&path, &patch.ops).unwrap();
            let before: serde_yaml::Value = serde_yaml::from_str(&source).unwrap();
            let after: serde_yaml::Value = serde_yaml::from_str(&written).unwrap();
            assert_eq!(after["x-data"], before["x-data"]);
        }
    }
}
