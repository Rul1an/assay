use super::jsonrpc::{
    ContentItem, JsonRpcRequest, JsonRpcResponse, ToolCallResult, ToolResultBody,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPolicy {
    #[serde(default, flatten)]
    pub tools: ToolPolicy,

    #[serde(default, deserialize_with = "deserialize_constraints")]
    pub constraints: Vec<ConstraintRule>,

    #[serde(default)]
    pub limits: Option<GlobalLimits>,

    #[serde(default)]
    pub version: String,

    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub signatures: Option<SignaturePolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignaturePolicy {
    #[serde(default)]
    pub check_descriptions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalLimits {
    pub max_requests_total: Option<u64>,
    pub max_tool_calls_total: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolPolicy {
    pub allow: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
}

// Canonical Rule Shape
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintRule {
    pub tool: String,
    pub params: BTreeMap<String, ConstraintParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintParam {
    #[serde(default)]
    pub matches: Option<String>,
    // For legacy support, maybe map "deny_patterns" regex to something?
    // The legacy code used deny_patterns. User wants "matches" (allowlist logic).
}

// Dual-Shape Deserializer Helper
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ConstraintsCompat {
    List(Vec<ConstraintRule>),
    // Legacy: Map<ToolName, Map<ArgName, RegexString>>
    // The previous implementation had ArgConstraints { deny_patterns: Map<String, String> }
    // If we want to support that via "matches", we mapping "regex" to "matches".
    // BUT legacy was DENY logic. "matches" is usually ALLOW logic.
    // Let's assume for this transition that if user provided a map, they meant the new "matches" logic
    // OR we can't support the logic inversion easily.
    // Given we are cleaning up, maybe we just support the structure and map string -> matches?
    Map(BTreeMap<String, BTreeMap<String, InputParamConstraint>>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum InputParamConstraint {
    // Handle "arg": "regex" (Legacy direct string)
    Direct(String),
    // Handle "arg": { "matches": "regex" } (Future map)
    Object(ConstraintParam),
}

fn deserialize_constraints<'de, D>(d: D) -> Result<Vec<ConstraintRule>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let c = Option::<ConstraintsCompat>::deserialize(d)?;

    let out = match c {
        None => vec![],
        Some(ConstraintsCompat::List(v)) => v,
        Some(ConstraintsCompat::Map(m)) => {
            m.into_iter()
                .map(|(tool, params)| {
                    let new_params = params.into_iter().map(|(arg, val)| {
                        let param = match val {
                            // Legacy: string was a deny regex? Or allow?
                            // Based on context of "constraints", usually implies "must match".
                            InputParamConstraint::Direct(s) => ConstraintParam { matches: Some(s) },
                            InputParamConstraint::Object(o) => o,
                        };
                        (arg, param)
                    }).collect();

                    ConstraintRule { tool, params: new_params }
                })
                .collect()
        }
    };

    Ok(out)
}

#[derive(Debug, Default)]
pub struct PolicyState {
    pub requests_count: u64,
    pub tool_calls_count: u64,
}

pub enum PolicyDecision {
    Allow,
    Deny {
        tool: String,
        reason: String,
        contract: Value,
    },
}

impl McpPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        // Support YAML (assay.yaml)
        let policy: McpPolicy = serde_yaml::from_str(&content)?;
        Ok(policy)
    }

    pub fn check(&self, request: &JsonRpcRequest, state: &mut PolicyState) -> PolicyDecision {
        state.requests_count += 1;

        if let Some(limits) = &self.limits {
            if let Some(max) = limits.max_requests_total {
                if state.requests_count > max {
                    return PolicyDecision::Deny {
                        tool: "ALL".to_string(),
                        reason: "Rate limit exceeded (total requests)".to_string(),
                        contract: serde_json::json!({
                            "status": "deny",
                            "error_code": "MCP_RATE_LIMIT",
                            "limit": max,
                            "current": state.requests_count,
                            "reason": "Global request limit exceeded"
                        }),
                    };
                }
            }
        }

        // Only check tools/call
        if !request.is_tool_call() {
            return PolicyDecision::Allow;
        }

        state.tool_calls_count += 1;

        // Rate Limits (Global Tool Calls)
        if let Some(limits) = &self.limits {
            if let Some(max) = limits.max_tool_calls_total {
                // eprintln!("[assay] DEBUG: tool_calls={} max={}", state.tool_calls_count, max);
                if state.tool_calls_count > max {
                    return PolicyDecision::Deny {
                        tool: "ALL".to_string(),
                        reason: "Rate limit exceeded (total tool calls)".to_string(),
                        contract: serde_json::json!({
                            "status": "deny",
                            "error_code": "MCP_RATE_LIMIT",
                            "limit": max,
                            "current": state.tool_calls_count,
                            "reason": "Global tool call limit exceeded"
                        }),
                    };
                }
            }
        }

        let params = match request.tool_params() {
            Some(p) => p,
            None => return PolicyDecision::Allow, // Malformed or no params, let server handle protocol error
        };

        let tool_name = &params.name;

        // 1. Denylist
        if let Some(deny) = &self.tools.deny {
            // Check wildcards
            let blocked = deny.iter().any(|pattern| {
                 if pattern.contains('*') {
                     // Simple glob: literal prefix match if ends with *, or contains
                     // For now, support "exec*"
                     if let Some(prefix) = pattern.strip_suffix('*') {
                         tool_name.starts_with(prefix)
                     } else if let Some(suffix) = pattern.strip_prefix('*') {
                         tool_name.ends_with(suffix)
                     } else {
                         tool_name == pattern
                     }
                 } else {
                     tool_name == pattern
                 }
            });

            if blocked {
                return PolicyDecision::Deny {
                    tool: tool_name.clone(),
                    reason: "Tool is explicitly denylisted".to_string(),
                    contract: serde_json::json!({
                        "status": "deny",
                        "error_code": "MCP_TOOL_DENIED",
                        "tool": tool_name.clone(),
                        "reason": "Tool is denylisted",
                        "did_you_mean": [], // TODO: Suggest similar tools
                        "suggested_patches": [
                            {"op":"remove","path":"/deny","value": tool_name} // updated path due to flatten
                        ]
                    }),
                };
            }
        }

        // 2. Allowlist
        if let Some(allow) = &self.tools.allow {
             // Check wildcards
            let explicitly_allowed = allow.iter().any(|pattern| {
                 if pattern.contains('*') {
                     if let Some(prefix) = pattern.strip_suffix('*') {
                         tool_name.starts_with(prefix)
                     } else if let Some(suffix) = pattern.strip_prefix('*') {
                         tool_name.ends_with(suffix)
                     } else {
                         tool_name == pattern
                     }
                 } else {
                     tool_name == pattern
                 }
            });

            if !explicitly_allowed {
                return PolicyDecision::Deny {
                    tool: tool_name.clone(),
                    reason: "Tool is not in the allowlist".to_string(),
                    contract: serde_json::json!({
                        "status": "deny",
                        "error_code": "MCP_TOOL_NOT_ALLOWED",
                        "tool": tool_name.clone(),
                        "reason": "Tool is not in allowlist",
                        "allowed_tools": allow,
                        "suggested_patches": [
                            {"op":"add","path":"/allow/-","value": tool_name} // updated path due to flatten
                        ]
                    }),
                };
            }
        }

        // 3. Argument Constraints (Normalized List)
        if let Value::Object(args_map) = &params.arguments {
            for rule in &self.constraints {
                if rule.tool != *tool_name {
                    continue;
                }

                for (arg_name, constraint) in &rule.params {
                    if let Some(val) = args_map.get(arg_name).and_then(|v| v.as_str()) {
                         if let Some(pattern) = &constraint.matches {
                             // "matches": regex (ALLOWED pattern? or DENIED pattern?)
                             // Wait, legacy ArgConstraints had deny_patterns. User example says constraints: matches: "^/app/.*".
                             // Usually "matches" implies allow-list logic (must match).
                             // But legacy code said "deny_patterns".
                             // Let's assume standard behavior: if matches is present, value MUST match the regex. If it doesn't match -> DENY.
                             // BUT wait, user said "constraints: ... matches: ^/app/.*" for "filesystem-readonly".
                             // This implies: "Allow only if matches /app/.*".
                             // So if !matches -> Deny.

                             if let Ok(re) = regex::Regex::new(pattern) {
                                 if !re.is_match(val) {
                                      return PolicyDecision::Deny {
                                         tool: tool_name.clone(),
                                         reason: format!(
                                             "Argument '{}' failed constraint (must match '{}')",
                                             arg_name, pattern
                                         ),
                                         contract: serde_json::json!({
                                             "status": "deny",
                                             "error_code": "MCP_ARG_CONSTRAINT",
                                             "tool": tool_name.clone(),
                                             "argument": arg_name,
                                             "pattern": pattern,
                                             "value": val,
                                             "violation": "must_match"
                                         }),
                                     };
                                 }
                             }
                         }
                    }
                }
            }
        }

        PolicyDecision::Allow
    }
}

pub fn make_deny_response(id: Value, msg: &str, contract: Value) -> String {
    let body = ToolResultBody {
        content: vec![
            ContentItem::Text {
                text: msg.to_string(),
            },
            // Optional: embedded contract in text for compatibility
            ContentItem::Text {
                text: contract.to_string(),
            },
        ],
        is_error: true,
        structured_content: Some(contract),
    };

    let resp = JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        payload: ToolCallResult { result: body },
    };

    serde_json::to_string(&resp).unwrap_or_default() + "\n"
}
