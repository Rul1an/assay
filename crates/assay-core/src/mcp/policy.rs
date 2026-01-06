use super::jsonrpc::{
    ContentItem, JsonRpcRequest, JsonRpcResponse, ToolCallResult, ToolResultBody,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpPolicy {
    #[serde(default)]
    pub tools: ToolPolicy,
    #[serde(default)]
    pub constraints: HashMap<String, ArgConstraints>,
    #[serde(default)]
    pub limits: Option<GlobalLimits>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgConstraints {
    pub deny_patterns: HashMap<String, String>, // arg_name -> regex
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
            if let Some(_max) = limits.max_requests_total {
                // TODO: Global request limiting via `max_requests_total` is not yet enforced.
                // Currently only `max_tool_calls_total` is used to enforce limits.
                // This field is kept for forward compatibility.
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
            if deny.contains(tool_name) {
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
                            {"op":"remove","path":"/tools/deny","value": tool_name}
                        ]
                    }),
                };
            }
        }

        // 2. Allowlist
        if let Some(allow) = &self.tools.allow {
            if !allow.contains(tool_name) {
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
                            {"op":"add","path":"/tools/allow/-","value": tool_name}
                        ]
                    }),
                };
            }
        }

        // 3. Argument Constraints
        if let Some(constraints) = self.constraints.get(tool_name) {
            if let Value::Object(args_map) = &params.arguments {
                for (arg, pattern) in &constraints.deny_patterns {
                    if let Some(val) = args_map.get(arg).and_then(|v| v.as_str()) {
                        // Check regex
                        // Note: compiling regex every time is inefficient, optimization for later
                        if let Ok(re) = regex::Regex::new(pattern) {
                            if re.is_match(val) {
                                return PolicyDecision::Deny {
                                    tool: tool_name.clone(),
                                    reason: format!(
                                        "Argument '{}' matches deny pattern '{}'",
                                        arg, pattern
                                    ),
                                    contract: serde_json::json!({
                                        "status": "deny",
                                        "error_code": "MCP_ARG_BLOCKED",
                                        "tool": tool_name.clone(),
                                        "argument": arg,
                                        "pattern": pattern,
                                        "value": val
                                    }),
                                };
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
