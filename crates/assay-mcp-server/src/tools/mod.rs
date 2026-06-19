use serde_json::Value;
use std::path::PathBuf;

use crate::cache::PolicyCaches;
use crate::config::ServerConfig;

pub struct ToolContext {
    pub policy_root: PathBuf,
    pub policy_root_canon: PathBuf,
    pub cfg: ServerConfig,
    pub caches: PolicyCaches,
}

impl ToolContext {
    /// Securely resolves a user-provided path against the policy root.
    pub async fn resolve_policy_path(
        &self,
        user_path: &str,
    ) -> std::result::Result<PathBuf, ToolError> {
        // Delegate to pure function
        crate::security::resolve_policy_path(&self.policy_root_canon, user_path)
    }
}

#[derive(serde::Serialize)]
pub struct ToolError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ToolError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        }
    }
    pub fn result(self) -> anyhow::Result<Value> {
        Ok(serde_json::to_value(serde_json::json!({
             "allowed": false,
             "error": self
        }))?)
    }
}

pub mod check_args;
pub mod check_coverage;
pub mod check_sequence;
pub mod explain_trace;
pub mod policy_decide;

#[cfg(feature = "test-outbound")]
pub mod test_outbound;

pub fn list_tools() -> Vec<Value> {
    #[allow(unused_mut)] // mut needed when feature "test-outbound" is enabled
    let mut list: Vec<Value> = vec![
        serde_json::json!({
            "name": "assay_check_args",
            "description": "Validate one proposed MCP tool call against an Assay policy file. Checks the tool name and JSON arguments against allow/deny rules and per-tool schemas, then returns an allow/deny decision with violations. This tool does not execute the target tool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool": {
                        "type": "string",
                        "description": "Exact MCP tool name to evaluate, for example `github.add_deploy_key`."
                    },
                    "arguments": {
                        "type": "object",
                        "description": "JSON object that would be sent to the tool. Assay validates this object against the matching policy schema."
                    },
                    "policy": {
                        "type": "string",
                        "description": "Policy file path relative to the configured policy root, for example `policy.yaml`."
                    }
                },
                "required": ["tool", "arguments", "policy"],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_check_sequence",
            "description": "Check whether a proposed next tool call is allowed by the sequence rules in an Assay policy. Provide the prior tool-name history and the next tool name; Assay returns sequence violations without executing any tools.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "history": {
                        "type": "array",
                        "description": "Tool names already observed in order before the proposed next call.",
                        "items": { "type": "string" }
                    },
                    "next_tool": {
                        "type": "string",
                        "description": "Tool name being considered as the next call."
                    },
                    "policy": {
                        "type": "string",
                        "description": "Policy file path relative to the configured policy root."
                    }
                },
                "required": ["history", "next_tool", "policy"],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_policy_decide",
            "description": "Return a lightweight allow/deny decision for a tool name using the policy blocklist. Use this for quick tool-name gating; use `assay_check_args` when argument/schema validation is needed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool": {
                        "type": "string",
                        "description": "Exact MCP tool name to check against the policy blocklist."
                    },
                    "policy": {
                        "type": "string",
                        "description": "Policy file path relative to the configured policy root."
                    }
                },
                "required": ["tool", "policy"],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_check_coverage",
            "description": "Measure how well one or more recorded tool-call traces cover an Assay policy. Returns coverage percentages, unseen tools or rules, and whether the supplied threshold is met; it does not claim runtime safety or provider truth.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "policy": {
                        "type": "string",
                        "description": "Policy file path relative to the configured policy root."
                    },
                    "traces": {
                        "type": "array",
                        "description": "Trace records to compare against the policy.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Optional trace identifier used in reports."
                                },
                                "tools": {
                                    "type": "array",
                                    "description": "Tool names observed in this trace.",
                                    "items": { "type": "string" }
                                },
                                "rules_triggered": {
                                    "type": "array",
                                    "description": "Optional policy rule identifiers observed as triggered in this trace.",
                                    "items": { "type": "string" }
                                }
                            },
                            "required": ["tools"],
                            "additionalProperties": false
                        }
                    },
                    "threshold": {
                        "type": "number",
                        "description": "Minimum acceptable coverage percentage from 0 to 100. Defaults to 80.",
                        "minimum": 0,
                        "maximum": 100,
                        "default": 80
                    },
                    "format": {
                        "type": "string",
                        "description": "Response format for the coverage report.",
                        "enum": ["json", "markdown", "github"],
                        "default": "json"
                    }
                },
                "required": ["policy", "traces"],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_explain_trace",
            "description": "Explain how an ordered trace of tool calls evaluates against an Assay policy. Produces step-by-step rule evaluation and blocked-step counts in JSON, Markdown, terminal text, or HTML.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "policy": {
                        "type": "string",
                        "description": "Policy file path relative to the configured policy root."
                    },
                    "trace": {
                        "type": "array",
                        "description": "Ordered tool-call trace to explain.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "tool": {
                                    "type": "string",
                                    "description": "Tool name for this trace step."
                                },
                                "args": {
                                    "type": "object",
                                    "description": "Optional JSON arguments observed for this trace step."
                                }
                            },
                            "required": ["tool"],
                            "additionalProperties": false
                        }
                    },
                    "format": {
                        "type": "string",
                        "description": "Response format for the explanation.",
                        "enum": ["json", "markdown", "terminal", "html"],
                        "default": "json"
                    }
                },
                "required": ["policy", "trace"],
                "additionalProperties": false
            }
        }),
    ];
    #[cfg(feature = "test-outbound")]
    list.push(serde_json::json!({
        "name": "assay_test_outbound",
        "description": "Test-only: E6a.3 no-pass-through E2E. GET ASSAY_TEST_OUTBOUND_URL with allowlist headers only.",
        "inputSchema": { "type": "object", "properties": {}, "required": [] }
    }));
    list
}

pub async fn handle_call(ctx: &ToolContext, name: &str, args: &Value) -> anyhow::Result<Value> {
    match name {
        "assay_check_args" => check_args::check_args(ctx, args).await,
        "assay_check_sequence" => check_sequence::check_sequence(ctx, args).await,
        "assay_policy_decide" => policy_decide::policy_decide(ctx, args).await,
        "assay_check_coverage" => check_coverage::check_coverage(ctx, args).await,
        "assay_explain_trace" => explain_trace::explain_trace(ctx, args).await,
        #[cfg(feature = "test-outbound")]
        "assay_test_outbound" => test_outbound::test_outbound(args).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}
