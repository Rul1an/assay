use super::{ArgsCheck, ConstraintRule, McpPolicy};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Defensively validate `args` against the declared per-tool schema. This is for the experimental
/// verdict gate, where a malformed declared schema is a classification input (it maps to `invalid`).
/// Preparation is shared with [`compile_all_schemas`] so `$defs` merge and collision semantics cannot
/// drift between the two paths, and both treat exactly the `"$defs"` key as the shared-definitions
/// entry: `compile_all_schemas` records the same malformed declaration as a per-tool error that the
/// enforcement engine denies with `E_SCHEMA_COMPILE`.
pub(super) fn check_tool_args(policy: &McpPolicy, tool_name: &str, args: &Value) -> ArgsCheck {
    if !policy.schemas.contains_key(tool_name) {
        return ArgsCheck::NoSchema;
    }
    let schemas = Value::Object(policy.schemas.clone().into_iter().collect());
    let schema_to_compile = match crate::policy_engine::prepare_tool_schema(&schemas, tool_name) {
        Ok(schema) => schema,
        Err(_) => return ArgsCheck::Malformed,
    };
    match crate::policy_engine::compile_schema(&schema_to_compile) {
        Ok(validator) => {
            if validator.is_valid(args) {
                ArgsCheck::Valid
            } else {
                ArgsCheck::Invalid
            }
        }
        // A declared schema that does not compile is a malformed declaration, not merely missing evidence.
        Err(_) => ArgsCheck::Malformed,
    }
}

pub(super) fn migrate_constraints_to_schemas(policy: &mut McpPolicy) {
    for constraint in std::mem::take(&mut policy.constraints) {
        let schema = constraint_to_schema(&constraint);
        policy.schemas.insert(constraint.tool.clone(), schema);
    }
    if policy.version.is_empty() || policy.version == "1.0" {
        policy.version = "2.0".to_string();
    }
}

/// Compile every tool schema, recording per-tool preparation and compile errors instead of
/// panicking. Preparation could not fail before #1948; it now fails on a shared/tool-local `$defs`
/// collision or a non-mapping `$defs`, and this map is materialized lazily from the enforcement
/// path (`compiled_schemas()` is a `OnceLock`), so a panic here would abort the proxy on the first
/// enforcement decision over user-authored policy input. A broken declared schema is a per-tool
/// fail-closed condition (`E_SCHEMA_COMPILE` at evaluation, mirroring `PolicyState::compile` and
/// `check_tool_args`), not a server abort.
///
/// Skips exactly the `"$defs"` key, matching `prepare_schema_map`: `$defs` is consumed by the
/// merge and is never itself a tool. Any other `$`-prefixed name is an ordinary (if ill-advised)
/// tool name on every path, so the two compilers cannot disagree about which keys are tools.
pub(super) fn compile_all_schemas(policy: &McpPolicy) -> super::types::CompiledSchemas {
    let schemas = Value::Object(policy.schemas.clone().into_iter().collect());
    let mut compiled = HashMap::new();
    for tool_name in policy.schemas.keys() {
        if tool_name == "$defs" {
            continue;
        }
        let result = crate::policy_engine::prepare_tool_schema(&schemas, tool_name)
            .and_then(|schema| crate::policy_engine::compile_schema(&schema))
            .map(Arc::new);
        if let Err(error) = &result {
            tracing::error!(
                "Schema for tool '{}' failed to prepare or compile; its calls will be denied \
                 (E_SCHEMA_COMPILE): {}",
                tool_name,
                error
            );
        }
        compiled.insert(tool_name.clone(), result);
    }
    compiled
}

fn constraint_to_schema(constraint: &ConstraintRule) -> Value {
    let mut properties = json!({});
    let mut required = vec![];

    for (param_name, param_constraint) in &constraint.params {
        if let Some(pattern) = &param_constraint.matches {
            properties[param_name] = json!({
                "type": "string",
                "pattern": pattern,
                "minLength": 1
                // No maxLength restriction for V1 backward compatibility
            });
            required.push(param_name.clone());
        }
    }

    json!({
        "type": "object",
        // Allow additional properties for V1 backward compatibility
        "additionalProperties": true,
        "properties": properties,
        "required": required,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_compilation_merges_shared_and_local_definitions() {
        let mut policy = McpPolicy::default();
        policy
            .schemas
            .insert("$defs".to_string(), json!({"shared": {"type": "string"}}));
        policy.schemas.insert(
            "lookup".to_string(),
            json!({
                "$defs": {"local": {"type": "integer"}},
                "type": "object",
                "properties": {
                    "name": {"$ref": "#/$defs/shared"},
                    "count": {"$ref": "#/$defs/local"}
                },
                "required": ["name", "count"]
            }),
        );

        assert_eq!(
            check_tool_args(&policy, "lookup", &json!({"name": "item", "count": 1})),
            ArgsCheck::Valid
        );
        assert!(policy
            .try_compile_all_schemas()
            .expect("merged schemas compile")
            .contains_key("lookup"));
    }

    /// #1952: a `$defs` collision is a per-tool compile error and a load-time `Err`, never a
    /// panic. The proxy reaches `compile_all_schemas` lazily from the enforcement path, so a
    /// panic here would abort the server on the first enforcement decision.
    #[test]
    fn mcp_compilation_records_collision_as_error_instead_of_panicking() {
        let mut policy = McpPolicy::default();
        policy
            .schemas
            .insert("$defs".to_string(), json!({"id": {"type": "string"}}));
        policy.schemas.insert(
            "lookup".to_string(),
            json!({"$defs": {"id": {"type": "integer"}}, "$ref": "#/$defs/id"}),
        );
        policy
            .schemas
            .insert("healthy".to_string(), json!({"type": "object"}));

        let compiled = compile_all_schemas(&policy);
        assert!(compiled["lookup"].is_err(), "collision is a per-tool error");
        assert!(compiled["healthy"].is_ok(), "healthy tools still compile");
        assert!(!compiled.contains_key("$defs"), "$defs is never a tool");

        let error = policy
            .try_compile_all_schemas()
            .expect_err("load-time surface names the broken tool");
        assert!(error.contains("lookup"), "{error}");
        assert!(error.contains("overlap"), "{error}");
    }

    /// The two compile paths must agree about which keys are tools. `compile_all_schemas` used
    /// to skip every `$`-prefixed key while `check_tool_args` special-cased only `"$defs"`, so a
    /// tool named `$weird` was checkable on one path and invisible on the other (#1952).
    #[test]
    fn dollar_prefixed_tool_names_are_tools_on_both_paths() {
        let mut policy = McpPolicy::default();
        policy
            .schemas
            .insert("$weird".to_string(), json!({"type": "object"}));

        assert_eq!(
            check_tool_args(&policy, "$weird", &json!({})),
            ArgsCheck::Valid
        );
        let compiled = compile_all_schemas(&policy);
        assert!(
            compiled.contains_key("$weird"),
            "load-time compiler sees the same tool set as check_tool_args"
        );
        assert!(compiled["$weird"].is_ok());
    }

    #[test]
    fn mcp_compilation_rejects_shared_local_definition_collisions() {
        let mut policy = McpPolicy::default();
        policy
            .schemas
            .insert("$defs".to_string(), json!({"id": {"type": "string"}}));
        policy.schemas.insert(
            "lookup".to_string(),
            json!({"$defs": {"id": {"type": "integer"}}, "$ref": "#/$defs/id"}),
        );

        assert_eq!(
            check_tool_args(&policy, "lookup", &json!(1)),
            ArgsCheck::Malformed
        );
    }

    #[test]
    fn mcp_compilation_never_retrieves_external_references() {
        let mut policy = McpPolicy::default();
        policy.schemas.insert(
            "lookup".to_string(),
            json!({"$ref": "https://example.invalid/external-schema"}),
        );

        assert_eq!(
            check_tool_args(&policy, "lookup", &json!({})),
            ArgsCheck::Malformed
        );
    }
}
