use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

struct LocalOnlyRetriever;

impl jsonschema::Retrieve for LocalOnlyRetriever {
    fn retrieve(
        &self,
        _uri: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("external JSON Schema retrieval is disabled".into())
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    Allowed,
    Blocked,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct Verdict {
    pub status: VerdictStatus,
    pub reason_code: String, // e.g., "OK", "E_ARG_SCHEMA", "E_TOOL_NOT_ALLOWED"
    pub details: Value,      // JSON details, violations, etc.
}

/// Evaluates tool arguments against a policy (JSON/YAML Value).
/// The policy is expected to be a map of tool_name -> schema.
pub fn evaluate_tool_args(policy: &Value, tool_name: &str, tool_args: &Value) -> Verdict {
    // 1. Check if tool exists in policy
    if policy
        .as_object()
        .and_then(|schemas| schemas.get(tool_name))
        .filter(|_| tool_name != "$defs")
        .is_none()
    {
        // Check for potential typos
        let mut message = format!("Tool '{}' not defined in policy", tool_name);
        if let Some(obj) = policy.as_object() {
            // Use our similarity helper
            if let Some(match_) = crate::errors::similarity::closest_prompt(
                tool_name,
                obj.keys().filter(|name| name.as_str() != "$defs"),
            ) {
                message.push_str(&format!(". Did you mean '{}'?", match_.prompt));
            }
        }
        return Verdict {
            status: VerdictStatus::Blocked,
            reason_code: "E_POLICY_MISSING_TOOL".to_string(),
            details: serde_json::json!({
                "message": message
            }),
        };
    }

    // 2. Compile Schema
    // In a real high-perf scenario, we'd cache this (Compilation is expensive).
    // For this core function, we compile on the fly or need a cached compilation context.
    // User Step 1.2: "Compile JSON Schema validators één keer bij policy load".
    // Since this function takes `&Value`, it implies per-call.
    // To support caching, we'd need a `PolicyState` struct.
    // For now, I'll compile on the fly (parity correctness first).

    let schema_val = match prepare_tool_schema(policy, tool_name) {
        Ok(schema) => schema,
        Err(error) => return schema_compile_error(tool_name, &error),
    };
    let compiled = match compile_schema(&schema_val) {
        Ok(c) => c,
        Err(e) => return schema_compile_error(tool_name, &e),
    };

    // 3. Validate
    evaluate_schema(&compiled, tool_args)
}

/// Evaluates tool arguments against a compiled schema.
pub fn evaluate_schema(compiled: &jsonschema::Validator, tool_args: &Value) -> Verdict {
    if compiled.is_valid(tool_args) {
        return Verdict {
            status: VerdictStatus::Allowed,
            reason_code: "OK".to_string(),
            details: serde_json::json!({}),
        };
    }
    let violations: Vec<Value> = compiled
        .iter_errors(tool_args)
        .map(|e| {
            serde_json::json!({
                "path": e.instance_path().to_string(),
                "constraint": e.to_string(),
                "message": e.to_string()
            })
        })
        .collect();
    Verdict {
        status: VerdictStatus::Blocked,
        reason_code: "E_ARG_SCHEMA".to_string(),
        details: serde_json::json!({
            "violations": violations
        }),
    }
}

/// A policy whose per-tool JSON Schema validators are compiled ONCE, so a caller evaluating many tool
/// calls against the same policy does not recompile per call (`jsonschema::validator_for` is the
/// expensive step). `evaluate_tool_args` stays the one-shot convenience that compiles on the fly; this
/// is the compile-once path for hot loops, matching how the MCP proxy compiles all schemas at policy
/// load. Verdicts are identical to `evaluate_tool_args` for the same policy and call.
pub struct PolicyState {
    validators: HashMap<String, Result<jsonschema::Validator, String>>,
    tool_names: Vec<String>,
}

/// Prepare a tool-schema map for compilation without permitting external retrieval.
///
/// Root `$defs` are merged into each object-valued tool schema and therefore compile under that
/// schema's declared dialect. A shared definition may not replace a tool-local definition with the
/// same name. With only boolean tools, the otherwise-unscoped definitions use the default dialect.
///
/// This is the ONE preparation semantics for `$defs` in a tool-schema map. Every consumer that
/// compiles or reasons about such a map (the MCP proxy's load-time compiler, `check_tool_args`,
/// the `tool_output_valid` metric, and config-validation vacuity checks) must go through it, or a
/// `$defs` entry means different things on different paths. The returned map contains only tool
/// entries: `$defs` is consumed by the merge and is never itself a tool schema.
pub fn prepare_schema_map(policy: &Value) -> Result<Value, String> {
    let Some(schemas) = policy.as_object() else {
        return Ok(policy.clone());
    };
    let root_defs = shared_defs(schemas)?;
    let has_object_tool = schemas
        .iter()
        .any(|(tool, schema)| tool != "$defs" && schema.is_object());
    if let Some(root_defs) = root_defs.filter(|_| !has_object_tool) {
        validate_unscoped_shared_defs(root_defs)?;
    }
    let mut prepared = serde_json::Map::new();
    for tool in schemas.keys().filter(|tool| tool.as_str() != "$defs") {
        prepared.insert(tool.clone(), prepare_tool_schema(policy, tool)?);
    }
    Ok(Value::Object(prepared))
}

fn shared_defs(
    schemas: &serde_json::Map<String, Value>,
) -> Result<Option<&serde_json::Map<String, Value>>, String> {
    Ok(match schemas.get("$defs") {
        Some(Value::Object(defs)) => Some(defs),
        Some(_) => return Err("shared $defs must be a mapping".to_string()),
        None => None,
    })
}

pub fn prepare_tool_schema(policy: &Value, tool: &str) -> Result<Value, String> {
    let schemas = policy
        .as_object()
        .ok_or_else(|| "policy must be a tool-name-to-schema mapping".to_string())?;
    let root_defs = shared_defs(schemas)?;
    let mut schema = schemas
        .get(tool)
        .cloned()
        .ok_or_else(|| format!("tool '{tool}' is not present"))?;
    if let Some(root_defs) = root_defs {
        match &mut schema {
            Value::Object(schema_object) => {
                let local_defs = match schema_object.get_mut("$defs") {
                    Some(Value::Object(defs)) => defs,
                    Some(_) => return Err("tool-local $defs must be a mapping".to_string()),
                    None => {
                        schema_object.insert("$defs".to_string(), Value::Object(root_defs.clone()));
                        return Ok(schema);
                    }
                };
                for (name, definition) in root_defs {
                    if local_defs.contains_key(name) {
                        return Err(
                            "shared and tool-local $defs entries must not overlap".to_string()
                        );
                    }
                    local_defs.insert(name.clone(), definition.clone());
                }
            }
            Value::Bool(_) => validate_unscoped_shared_defs(root_defs)?,
            _ => {}
        }
    }
    Ok(schema)
}

fn validate_unscoped_shared_defs(root_defs: &serde_json::Map<String, Value>) -> Result<(), String> {
    let definitions_schema = serde_json::json!({"$defs": root_defs});
    compile_schema(&definitions_schema)
        .map(|_| ())
        .map_err(|error| format!("shared $defs failed to compile: {error}"))
}

pub(crate) fn compile_schema(schema: &Value) -> Result<jsonschema::Validator, String> {
    jsonschema::options()
        .with_retriever(LocalOnlyRetriever)
        .build(schema)
        .map_err(|error| error.to_string())
}

fn schema_compile_error(tool_name: &str, error: &str) -> Verdict {
    Verdict {
        status: VerdictStatus::Blocked,
        reason_code: "E_SCHEMA_COMPILE".to_string(),
        details: serde_json::json!({
            "message": format!("Invalid schema for tool '{}': {}", tool_name, error)
        }),
    }
}

impl PolicyState {
    /// Compile every tool schema in the policy once. A tool whose schema fails to compile is recorded
    /// as an error and only surfaces (as `E_SCHEMA_COMPILE`) if that tool is later evaluated, matching
    /// the one-shot `evaluate_tool_args` behavior of only compiling the requested tool's schema.
    pub fn compile(policy: &Value) -> Self {
        let mut validators = HashMap::new();
        let tool_names: Vec<_> = policy
            .as_object()
            .into_iter()
            .flat_map(|schemas| schemas.keys())
            .filter(|tool| tool.as_str() != "$defs")
            .cloned()
            .collect();
        for tool in &tool_names {
            let compiled =
                prepare_tool_schema(policy, tool).and_then(|schema| compile_schema(&schema));
            validators.insert(tool.clone(), compiled);
        }
        Self {
            validators,
            tool_names,
        }
    }

    /// Evaluate one tool call against the pre-compiled validators.
    pub fn evaluate(&self, tool_name: &str, tool_args: &Value) -> Verdict {
        if !self.tool_names.iter().any(|tool| tool == tool_name) {
            return {
                let mut message = format!("Tool '{}' not defined in policy", tool_name);
                if let Some(match_) =
                    crate::errors::similarity::closest_prompt(tool_name, self.tool_names.iter())
                {
                    message.push_str(&format!(". Did you mean '{}'?", match_.prompt));
                }
                Verdict {
                    status: VerdictStatus::Blocked,
                    reason_code: "E_POLICY_MISSING_TOOL".to_string(),
                    details: serde_json::json!({ "message": message }),
                }
            };
        }
        match self.validators.get(tool_name) {
            None => schema_compile_error(tool_name, "schema preparation produced no validator"),
            Some(Err(e)) => schema_compile_error(tool_name, e),
            Some(Ok(compiled)) => evaluate_schema(compiled, tool_args),
        }
    }
}

/// Evaluates a sequence of tool calls against a sequence policy (regex-like).
/// For v0.9, simplified: the policy is just a string (regex) of tool names.
/// E.g. "^search (analyze )*report$"
/// The input is a list of tool names invoked in order.
pub fn evaluate_sequence(policy_regex: &str, tool_names: &[String]) -> Verdict {
    // 1. Construct the sequence string
    // We join tool names with space. Note: tool names should not contain spaces ideally.
    // If they do, this simple approach might be ambiguous, but standard tools usually don't.
    let trace_str = tool_names.join(" ");

    // 2. Compile Regex
    // Again, efficiency concern: compile once.
    let re = match regex::Regex::new(policy_regex) {
        Ok(r) => r,
        Err(e) => {
            return Verdict {
                status: VerdictStatus::Blocked,
                reason_code: "E_POLICY_REGEX_INVALID".to_string(),
                details: serde_json::json!({
                    "message": format!("Invalid regex policy '{}': {}", policy_regex, e)
                }),
            };
        }
    };

    // 3. Match
    if re.is_match(&trace_str) {
        Verdict {
            status: VerdictStatus::Allowed,
            reason_code: "OK".to_string(),
            details: serde_json::json!({}),
        }
    } else {
        Verdict {
            status: VerdictStatus::Blocked,
            reason_code: "E_SEQUENCE_VIOLATION".to_string(),
            details: serde_json::json!({
                "expected": policy_regex,
                "found": trace_str
            }),
        }
    }
}
