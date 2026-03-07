use super::{ConstraintRule, McpPolicy};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

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
    // Option 1: Inline $defs into every schema to support relative #/$defs/... refs
    let root_defs = policy.schemas.get("$defs").cloned();

    let mut compiled = HashMap::new();
    for (tool_name, schema) in &policy.schemas {
        if tool_name.starts_with('$') {
            continue;
        }

        let mut schema_to_compile = schema.clone();
        // Inject $defs if they exist and the schema is an object
        if let Some(defs) = &root_defs {
            if let Value::Object(map) = &mut schema_to_compile {
                // Only insert if not already present to allow overrides (or just overwrite?)
                // For now, insert if missing or overwrite to ensure global defs availability.
                map.insert("$defs".to_string(), defs.clone());
            }
        }

        match jsonschema::validator_for(&schema_to_compile) {
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
