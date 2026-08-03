//! Empirical pins for the jsonschema-crate semantics both #1951 and #1952 rest on.
use serde_json::json;

#[test]
fn unresolvable_local_ref_fails_at_build_time() {
    let schema = json!({"$ref": "#/$defs/Missing"});
    let result = jsonschema::validator_for(&schema);
    assert!(result.is_err(), "expected build-time error, got Ok");
}

#[test]
fn bare_defs_object_accepts_everything() {
    let schema = json!({"$defs": {"X": {"type": "string"}}});
    let validator = jsonschema::validator_for(&schema).expect("compiles");
    assert!(validator.is_valid(&json!(42)));
    assert!(validator.is_valid(&json!({"anything": true})));
}

#[test]
fn ref_with_merged_defs_resolves_and_validates() {
    let schema = json!({"$defs": {"NonEmpty": {"type": "string", "minLength": 1}}, "$ref": "#/$defs/NonEmpty"});
    let validator = jsonschema::validator_for(&schema).expect("compiles");
    assert!(validator.is_valid(&json!("ok")));
    assert!(!validator.is_valid(&json!("")));
    assert!(!validator.is_valid(&json!(7)));
}

/// #1952 end to end: a policy carrying a shared/tool-local `$defs` collision loads, and the first
/// enforcement decision on the broken tool is a fail-closed deny, not a process abort. Healthy
/// tools in the same policy keep evaluating normally.
mod collision_policy_enforcement {
    use assay_core::mcp::policy::{McpPolicy, PolicyDecision, PolicyState};
    use serde_json::json;

    fn collision_policy() -> McpPolicy {
        let mut policy = McpPolicy::default();
        policy
            .schemas
            .insert("$defs".to_string(), json!({"id": {"type": "string"}}));
        policy.schemas.insert(
            "lookup".to_string(),
            json!({"$defs": {"id": {"type": "integer"}}, "$ref": "#/$defs/id"}),
        );
        policy.schemas.insert(
            "healthy".to_string(),
            json!({"type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}),
        );
        policy
    }

    #[test]
    fn broken_tool_is_denied_fail_closed_instead_of_panicking() {
        let policy = collision_policy();
        let mut state = PolicyState::default();

        let decision = policy.evaluate("lookup", &json!({"anything": 1}), &mut state, None);
        match decision {
            PolicyDecision::Deny { code, tool, .. } => {
                assert_eq!(code, "E_SCHEMA_COMPILE");
                assert_eq!(tool, "lookup");
            }
            other => panic!("expected fail-closed deny, got {other:?}"),
        }
    }

    #[test]
    fn healthy_tools_keep_evaluating_beside_a_broken_one() {
        let policy = collision_policy();
        let mut state = PolicyState::default();

        let allowed = policy.evaluate("healthy", &json!({"name": "x"}), &mut state, None);
        assert!(matches!(allowed, PolicyDecision::Allow));

        let rejected = policy.evaluate("healthy", &json!({}), &mut state, None);
        assert!(
            matches!(rejected, PolicyDecision::Deny { ref code, .. } if code == "E_ARG_SCHEMA"),
            "schema violations still classify normally: {rejected:?}"
        );
    }
}
