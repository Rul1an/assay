use super::jsonrpc::{
    ContentItem, JsonRpcRequest, JsonRpcResponse, ToolCallResult, ToolResultBody,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPolicy {
    #[serde(default)]
    pub tools: ToolPolicy,

    #[serde(default)]
    pub allow: Option<Vec<String>>,
    #[serde(default)]
    pub deny: Option<Vec<String>>,

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
}

// Dual-Shape Deserializer Helper
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ConstraintsCompat {
    List(Vec<ConstraintRule>),
    // Legacy: Map<ToolName, Map<ArgName, RegexString>>
    Map(BTreeMap<String, BTreeMap<String, InputParamConstraint>>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum InputParamConstraint {
    Direct(String),
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

fn matches_tool_pattern(tool_name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return tool_name == pattern;
    }

    let starts_star = pattern.starts_with('*');
    let ends_star = pattern.ends_with('*');

    match (starts_star, ends_star) {
        (true, true) => {
            // *abc* => contains("abc")
            let inner = pattern.trim_matches('*');
            if inner.is_empty() {
                true // pattern == "***" => match all
            } else {
                tool_name.contains(inner)
            }
        }
        (false, true) => {
            // abc* => starts_with("abc")
            let prefix = pattern.trim_end_matches('*');
            !prefix.is_empty() && tool_name.starts_with(prefix)
        }
        (true, false) => {
            // *abc => ends_with("abc")
            let suffix = pattern.trim_start_matches('*');
            !suffix.is_empty() && tool_name.ends_with(suffix)
        }
        (false, false) => tool_name == pattern, // unreachable due to contains('*') check above
    }
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

        // 1. Denylist Checks (Union of root.deny + tools.deny)
        let root_deny = self.deny.as_ref();
        let tools_deny = self.tools.deny.as_ref();

        let blocked = root_deny.iter().flat_map(|v| v.iter())
            .chain(tools_deny.iter().flat_map(|v| v.iter()))
            .any(|pattern| matches_tool_pattern(tool_name, pattern));

        if blocked {
            return PolicyDecision::Deny {
                tool: tool_name.clone(),
                reason: "Tool is explicitly denylisted".to_string(),
                contract: serde_json::json!({
                    "status": "deny",
                    "error_code": "MCP_TOOL_DENIED",
                    "tool": tool_name.clone(),
                    "reason": "Tool is denylisted",
                    "did_you_mean": [],
                    "suggested_patches": [
                        {"op":"remove","path":"/deny","value": tool_name}
                    ]
                }),
            };
        }

        // 2. Allowlist Checks (Union of root.allow + tools.allow)
        let root_allow = self.allow.as_ref();
        let tools_allow = self.tools.allow.as_ref();

        if root_allow.is_some() || tools_allow.is_some() {
             let explicitly_allowed = root_allow.iter().flat_map(|v| v.iter())
                .chain(tools_allow.iter().flat_map(|v| v.iter()))
                .any(|pattern| matches_tool_pattern(tool_name, pattern));

            if !explicitly_allowed {
                return PolicyDecision::Deny {
                    tool: tool_name.clone(),
                    reason: "Tool is not in the allowlist".to_string(),
                    contract: serde_json::json!({
                        "status": "deny",
                        "error_code": "MCP_TOOL_NOT_ALLOWED",
                        "tool": tool_name.clone(),
                        "reason": "Tool is not in allowlist",
                        "allowed_tools": root_allow.or(tools_allow),
                        "suggested_patches": [
                            {"op":"add","path":"/allow/-","value": tool_name}
                        ]
                    }),
                };
            }
        }

        // 3. Argument Constraints
        if let Value::Object(args_map) = &params.arguments {
            for rule in &self.constraints {
                if rule.tool != *tool_name {
                    continue;
                }

                for (arg_name, constraint) in &rule.params {
                    let Some(pattern) = &constraint.matches else {
                        continue;
                    };

                    let arg_val = args_map.get(arg_name);

                    let val_str = match arg_val.and_then(|v| v.as_str()) {
                         Some(s) => s,
                         None => {
                             return PolicyDecision::Deny {
                                 tool: tool_name.clone(),
                                 reason: format!(
                                     "Argument '{}' missing or not a string (required to match '{}')",
                                     arg_name, pattern
                                 ),
                                 contract: serde_json::json!({
                                     "status": "deny",
                                     "error_code": "MCP_CONSTRAINT_MISSING",
                                     "tool": tool_name.clone(),
                                     "argument": arg_name,
                                     "pattern": pattern,
                                     "violation": "missing_or_non_string"
                                 }),
                             };
                         }
                    };

                    match regex::Regex::new(pattern) {
                         Ok(re) => {
                             if !re.is_match(val_str) {
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
                                         "value": val_str,
                                         "violation": "must_match"
                                     }),
                                 };
                             }
                         }
                         Err(_) => {
                            // Invalid regex in policy -> Deny safely
                            return PolicyDecision::Deny {
                                tool: tool_name.clone(),
                                reason: format!(
                                    "Invalid regex constraint for argument '{}' (pattern '{}')",
                                    arg_name, pattern
                                ),
                                contract: serde_json::json!({
                                    "status": "deny",
                                    "error_code": "MCP_POLICY_INVALID_REGEX",
                                    "tool": tool_name.clone(),
                                    "argument": arg_name,
                                    "pattern": pattern,
                                    "violation": "invalid_regex"
                                }),
                            };
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
