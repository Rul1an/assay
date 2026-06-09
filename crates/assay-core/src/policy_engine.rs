use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

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
    let schema_val = match policy.get(tool_name) {
        Some(s) => s,
        None => {
            // Check for potential typos
            let mut message = format!("Tool '{}' not defined in policy", tool_name);
            if let Some(obj) = policy.as_object() {
                // Use our similarity helper
                if let Some(match_) =
                    crate::errors::similarity::closest_prompt(tool_name, obj.keys())
                {
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
    };

    // 2. Compile Schema
    // In a real high-perf scenario, we'd cache this (Compilation is expensive).
    // For this core function, we compile on the fly or need a cached compilation context.
    // User Step 1.2: "Compile JSON Schema validators één keer bij policy load".
    // Since this function takes `&Value`, it implies per-call.
    // To support caching, we'd need a `PolicyState` struct.
    // For now, I'll compile on the fly (parity correctness first).

    let compiled = match jsonschema::validator_for(schema_val) {
        Ok(c) => c,
        Err(e) => {
            return Verdict {
                status: VerdictStatus::Blocked,
                reason_code: "E_SCHEMA_COMPILE".to_string(),
                details: serde_json::json!({
                    "message": format!("Invalid schema for tool '{}': {}", tool_name, e)
                }),
            };
        }
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

impl PolicyState {
    /// Compile every tool schema in the policy once. A tool whose schema fails to compile is recorded
    /// as an error and only surfaces (as `E_SCHEMA_COMPILE`) if that tool is later evaluated, matching
    /// the one-shot `evaluate_tool_args` behavior of only compiling the requested tool's schema.
    pub fn compile(policy: &Value) -> Self {
        let mut validators = HashMap::new();
        let mut tool_names = Vec::new();
        if let Some(obj) = policy.as_object() {
            for (tool, schema_val) in obj {
                tool_names.push(tool.clone());
                validators.insert(
                    tool.clone(),
                    jsonschema::validator_for(schema_val).map_err(|e| e.to_string()),
                );
            }
        }
        Self {
            validators,
            tool_names,
        }
    }

    /// Evaluate one tool call against the pre-compiled validators.
    pub fn evaluate(&self, tool_name: &str, tool_args: &Value) -> Verdict {
        match self.validators.get(tool_name) {
            None => {
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
            }
            Some(Err(e)) => Verdict {
                status: VerdictStatus::Blocked,
                reason_code: "E_SCHEMA_COMPILE".to_string(),
                details: serde_json::json!({
                    "message": format!("Invalid schema for tool '{}': {}", tool_name, e)
                }),
            },
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
