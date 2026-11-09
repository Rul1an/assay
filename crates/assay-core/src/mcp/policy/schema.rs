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
    // `$defs` is the shared-definitions entry, consumed by preparation; it is never a tool. Without
    // this, a call to a tool literally named "$defs" would compile the definitions map as a schema
    // (which asserts nothing and accepts everything) while the load-time compiler produces no
    // validator for it: exactly the cross-path drift this module exists to prevent.
    if tool_name == "$defs" || !policy.schemas.contains_key(tool_name) {
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
        policy
            .schemas
            .insert("$defs".to_string(), json!({"shared": {"type": "string"}}));
        assert_eq!(
            check_tool_args(&policy, "$defs", &json!({})),
            ArgsCheck::NoSchema,
            "$defs is consumed by preparation and is never itself a checkable tool"
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

    /// The runtime reads each tool's schema by the per-tool rule `assay validate` and `assay run`
    /// apply: a root key the evaluator does not read, or a schema that asserts nothing without
    /// saying so, is a malformed declaration, so the load-time surface names it and the tool's
    /// calls are denied with `E_SCHEMA_COMPILE`. Every row is checked against its expected verdict
    /// and against the validation path on the same document, so the two readers can drift neither
    /// apart nor together. Unread keys below the root stay accepted on both: the rule checks the
    /// root.
    #[test]
    fn runtime_reads_tool_schemas_by_the_validation_rule() {
        use crate::mcp::policy::{PolicyDecision, PolicyState};

        let cmd = json!({"type": "string", "pattern": "^ls"});
        let rows = [
            (
                "per-argument map",
                json!({"deny": ["rm"], "schemas": {"exec": {"cmd": cmd}}}),
                true,
            ),
            (
                "type: object with the argument beside it",
                json!({"deny": ["rm"], "schemas": {"exec": {"type": "object", "cmd": cmd}}}),
                true,
            ),
            (
                "per-argument map under unconstrained_tools: deny",
                json!({"enforcement": {"unconstrained_tools": "deny"},
                    "schemas": {"exec": {"cmd": cmd}}}),
                true,
            ),
            (
                "root nullable beside a constraint",
                json!({"deny": ["rm"], "schemas": {"exec": {
                    "type": "object", "nullable": true, "properties": {"cmd": cmd}}}}),
                true,
            ),
            (
                "asserts nothing without saying so",
                json!({"deny": ["rm"], "schemas": {"exec": {"properties": {"cmd": {}}}}}),
                true,
            ),
            (
                "true",
                json!({"deny": ["rm"], "schemas": {"exec": true}}),
                false,
            ),
            (
                "false",
                json!({"deny": ["rm"], "schemas": {"exec": false}}),
                false,
            ),
            (
                "empty",
                json!({"deny": ["rm"], "schemas": {"exec": {}}}),
                false,
            ),
            (
                "annotations only",
                json!({"deny": ["rm"], "schemas": {"exec": {"description": "any command"}}}),
                false,
            ),
            (
                "true under unconstrained_tools: deny",
                json!({"enforcement": {"unconstrained_tools": "deny"}, "schemas": {"exec": true}}),
                false,
            ),
            (
                "x- annotation beside a constraint",
                json!({"deny": ["rm"], "schemas": {"exec": {
                    "type": "object", "x-owner": "ops", "properties": {"cmd": cmd}}}}),
                false,
            ),
            (
                "shared $defs",
                json!({"deny": ["rm"], "schemas": {"$defs": {"c": cmd},
                    "exec": {"type": "object", "properties": {"cmd": {"$ref": "#/$defs/c"}}}}}),
                false,
            ),
            (
                "tool-local $defs",
                json!({"deny": ["rm"], "schemas": {"exec": {"$defs": {"c": cmd},
                    "type": "object", "properties": {"cmd": {"$ref": "#/$defs/c"}}}}}),
                false,
            ),
            (
                "constrained",
                json!({"deny": ["rm"], "schemas": {"exec": {
                    "type": "object", "properties": {"cmd": cmd}, "required": ["cmd"]}}}),
                false,
            ),
            (
                "unread key below the root",
                json!({"deny": ["rm"], "schemas": {"exec": {
                    "type": "object", "properties": {"cmd": {"type": "string", "regex": "^ls"}}}}}),
                false,
            ),
        ];

        let mut wrong = Vec::new();
        for (label, mut document, refused) in rows {
            document["version"] = json!("2.0");
            let validation = crate::model::validate_args_policy_value(&document);
            if validation.is_err() != refused {
                wrong.push(format!("{label}: validation {validation:?}"));
            }

            let policy = McpPolicy::from_slice(document.to_string().as_bytes())
                .unwrap_or_else(|error| panic!("{label}: {error:#}"));
            let runtime = compile_all_schemas(&policy)
                .get("exec")
                .unwrap_or_else(|| panic!("{label}: exec was not compiled"))
                .as_ref()
                .err()
                .cloned();
            if runtime.is_some() != refused {
                wrong.push(format!("{label}: runtime {runtime:?}"));
            }
            if let (Some(runtime), Err(validation)) = (&runtime, &validation) {
                if *runtime != validation.to_string() {
                    wrong.push(format!("{label}: runtime {runtime:?} != {validation}"));
                }
            }
            if policy.try_compile_all_schemas().is_err() != refused {
                wrong.push(format!("{label}: try_compile_all_schemas"));
            }
            let checked = check_tool_args(&policy, "exec", &json!({"cmd": "ls -la"}));
            if (checked == ArgsCheck::Malformed) != refused {
                wrong.push(format!("{label}: check_tool_args {checked:?}"));
            }
            let decision = policy.evaluate(
                "exec",
                &json!({"cmd": "cat /etc/passwd"}),
                &mut PolicyState::default(),
                None,
            );
            let schema_denied = matches!(
                &decision,
                PolicyDecision::Deny { code, .. } if code == "E_SCHEMA_COMPILE"
            );
            if schema_denied != refused {
                wrong.push(format!("{label}: evaluate {decision:?}"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }
}
