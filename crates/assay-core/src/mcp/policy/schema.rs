use super::{ArgsCheck, ConstraintRule, McpPolicy};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Defensively validate `args` against the declared per-tool schema WITHOUT panicking. Unlike
/// [`compile_all_schemas`] (which fail-closes by panicking at load time), this is for the experimental
/// verdict gate, where a malformed declared schema is a classification input (it maps to `invalid`), not
/// an abort. Preparation is shared with the load-time compiler so `$defs` merge and collision semantics
/// cannot drift between the two paths.
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

pub(super) fn compile_all_schemas(
    policy: &McpPolicy,
) -> HashMap<String, Arc<jsonschema::Validator>> {
    let schemas = Value::Object(policy.schemas.clone().into_iter().collect());
    let mut compiled = HashMap::new();
    for tool_name in policy.schemas.keys() {
        if tool_name.starts_with('$') {
            continue;
        }
        let schema_to_compile = crate::policy_engine::prepare_tool_schema(&schemas, tool_name)
            .unwrap_or_else(|error| {
                panic!("Failed to prepare JSON schema for tool '{tool_name}': {error}")
            });
        match crate::policy_engine::compile_schema(&schema_to_compile) {
            Ok(validator) => {
                compiled.insert(tool_name.clone(), Arc::new(validator));
            }
            Err(e) => {
                tracing::error!("Failed to compile schema for tool {}: {}", tool_name, e);
                // Fail securely: do not allow tools with broken schemas to load.
                panic!(
                    "Failed to compile JSON schema for tool '{}': {}",
                    tool_name, e
                );
            }
        }
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
        assert!(policy.compile_all_schemas().contains_key("lookup"));
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
