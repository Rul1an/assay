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
            "description": "Pre-flight review for one proposed MCP tool call. Use this before executing a tool when you have the exact tool name, the JSON arguments that would be sent, and an Assay policy file. It evaluates allow/deny rules plus the matching per-tool JSON schema and returns allowed=true/false, warnings, violations, and a suggested-fix slot. It never invokes the target tool and never proves the provider executed anything.",
            "inputSchema": {
                "type": "object",
                "title": "Tool argument policy check request",
                "description": "Request body for validating one MCP tool call against a local Assay policy.",
                "properties": {
                    "tool": {
                        "type": "string",
                        "description": "Exact MCP tool name to evaluate, using the same name that would appear in the client tool call.",
                        "minLength": 1,
                        "examples": ["github.add_deploy_key", "filesystem.write_file"]
                    },
                    "arguments": {
                        "type": "object",
                        "description": "JSON object that would be sent to the target tool. Assay validates this object against the policy schema for the named tool.",
                        "examples": [
                            { "repository": "owner/repo", "key": "ssh-ed25519 AAAA..." },
                            { "path": "docs/report.md", "content": "draft" }
                        ]
                    },
                    "policy": {
                        "type": "string",
                        "description": "Assay policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml", "policies/mcp-production.yaml"]
                    }
                },
                "required": ["tool", "arguments", "policy"],
                "examples": [
                    {
                        "tool": "github.add_deploy_key",
                        "arguments": { "repository": "owner/repo", "key": "ssh-ed25519 AAAA..." },
                        "policy": "policy.yaml"
                    }
                ],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_check_sequence",
            "description": "Pre-flight review for tool-call order. Use this when an agent is about to make another MCP tool call and you need to check the proposed next tool against policy sequence rules such as required predecessors, forbidden orderings, or deadline windows. It returns allowed=true/false and sequence violations for the trace-so-far plus next_tool. It does not execute tools and does not assert that the workflow is complete.",
            "inputSchema": {
                "type": "object",
                "title": "Tool sequence policy check request",
                "description": "Request body for validating whether next_tool is allowed after the observed history.",
                "properties": {
                    "history": {
                        "type": "array",
                        "description": "Tool names already observed in chronological order before the proposed next call.",
                        "items": { "type": "string", "minLength": 1 },
                        "examples": [["github.get_repository", "github.list_branches"]]
                    },
                    "next_tool": {
                        "type": "string",
                        "description": "Exact MCP tool name being considered as the next call.",
                        "minLength": 1,
                        "examples": ["github.create_pull_request"]
                    },
                    "policy": {
                        "type": "string",
                        "description": "Assay sequence or full policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml", "policies/release-flow.yaml"]
                    }
                },
                "required": ["history", "next_tool", "policy"],
                "examples": [
                    {
                        "history": ["github.get_repository", "github.list_branches"],
                        "next_tool": "github.create_pull_request",
                        "policy": "policy.yaml"
                    }
                ],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_policy_decide",
            "description": "Fast name-only policy decision for an MCP tool. Use this for inexpensive routing or UI gating when only the tool name is known. It checks the policy blocklist and returns allowed=true/false plus a short reason or match. It intentionally does not validate arguments, schemas, sequence rules, runtime delivery, or provider behavior; use assay_check_args for argument-aware review.",
            "inputSchema": {
                "type": "object",
                "title": "Tool name policy decision request",
                "description": "Request body for checking a tool name against the local policy blocklist.",
                "properties": {
                    "tool": {
                        "type": "string",
                        "description": "Exact MCP tool name to check against the policy blocklist.",
                        "minLength": 1,
                        "examples": ["shell.exec", "github.delete_repository"]
                    },
                    "policy": {
                        "type": "string",
                        "description": "Policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml"]
                    }
                },
                "required": ["tool", "policy"],
                "examples": [
                    {
                        "tool": "shell.exec",
                        "policy": "policy.yaml"
                    }
                ],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_check_coverage",
            "description": "Coverage report for policy test traces. Use this after collecting one or more tool-call traces to see which policy tools or rules were exercised, what remains unseen, and whether the requested coverage threshold was met. It returns JSON by default or a Markdown/GitHub annotation summary. This is evidence about trace coverage only, not runtime safety, provider truth, or compliance.",
            "inputSchema": {
                "type": "object",
                "title": "Policy trace coverage request",
                "description": "Request body for comparing recorded tool-call traces with policy coverage expectations.",
                "properties": {
                    "policy": {
                        "type": "string",
                        "description": "Full Assay policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml", "policies/mcp-production.yaml"]
                    },
                    "traces": {
                        "type": "array",
                        "description": "Recorded traces to compare against the policy. Each trace should list the tools observed in execution order and may list policy rules observed as triggered.",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "title": "Coverage trace record",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "Optional stable trace identifier used in reports.",
                                    "examples": ["trace-pr-42-happy-path"]
                                },
                                "tools": {
                                    "type": "array",
                                    "description": "Tool names observed in this trace.",
                                    "minItems": 1,
                                    "items": { "type": "string", "minLength": 1 },
                                    "examples": [["github.get_repository", "github.create_pull_request"]]
                                },
                                "rules_triggered": {
                                    "type": "array",
                                    "description": "Optional policy rule identifiers observed as triggered in this trace.",
                                    "items": { "type": "string", "minLength": 1 },
                                    "examples": [["require_review_before_merge"]]
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
                        "default": 80,
                        "examples": [80, 95]
                    },
                    "format": {
                        "type": "string",
                        "description": "Response format for the coverage report.",
                        "enum": ["json", "markdown", "github"],
                        "default": "json",
                        "examples": ["json", "markdown"]
                    }
                },
                "required": ["policy", "traces"],
                "examples": [
                    {
                        "policy": "policy.yaml",
                        "traces": [
                            {
                                "id": "trace-pr-42-happy-path",
                                "tools": ["github.get_repository", "github.create_pull_request"],
                                "rules_triggered": ["require_review_before_merge"]
                            }
                        ],
                        "threshold": 80,
                        "format": "json"
                    }
                ],
                "additionalProperties": false
            }
        }),
        serde_json::json!({
            "name": "assay_explain_trace",
            "description": "Human-readable explanation of a recorded MCP tool-call trace. Use this when you need to debug or review why a sequence of tool calls was allowed, warned, or blocked by an Assay policy. It evaluates the supplied ordered trace and returns step-by-step rule reasoning, blocked-step counts, and formatted output. It is an offline explanation of supplied evidence, not a live telemetry exporter.",
            "inputSchema": {
                "type": "object",
                "title": "Trace explanation request",
                "description": "Request body for explaining how a recorded tool-call trace evaluates against an Assay policy.",
                "properties": {
                    "policy": {
                        "type": "string",
                        "description": "Assay policy file path relative to the server policy root.",
                        "minLength": 1,
                        "examples": ["policy.yaml"]
                    },
                    "trace": {
                        "type": "array",
                        "description": "Ordered tool-call trace to explain. Each item is one observed MCP tool call.",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "title": "Trace step",
                            "properties": {
                                "tool": {
                                    "type": "string",
                                    "description": "Tool name for this trace step.",
                                    "minLength": 1,
                                    "examples": ["github.create_pull_request"]
                                },
                                "args": {
                                    "type": "object",
                                    "description": "Optional JSON arguments observed for this trace step. This is used only for explanation and policy evaluation; the tool is not invoked.",
                                    "examples": [{ "repository": "owner/repo", "title": "Update policy docs" }]
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
                        "default": "json",
                        "examples": ["markdown", "json"]
                    }
                },
                "required": ["policy", "trace"],
                "examples": [
                    {
                        "policy": "policy.yaml",
                        "trace": [
                            {
                                "tool": "github.create_pull_request",
                                "args": { "repository": "owner/repo", "title": "Update policy docs" }
                            }
                        ],
                        "format": "markdown"
                    }
                ],
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
