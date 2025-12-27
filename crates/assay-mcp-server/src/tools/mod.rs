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
pub mod check_sequence;
pub mod policy_decide;

pub fn list_tools() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": "assay_check_args",
            "description": "Validate tool arguments against a policy schema.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool": { "type": "string" },
                    "arguments": { "type": "object" },
                    "policy": { "type": "string" }
                },
                "required": ["tool", "arguments"]
            }
        }),
        serde_json::json!({
            "name": "assay_check_sequence",
            "description": "Validate if a tool call is allowed given the history.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "history": { "type": "array", "items": { "type": "string" } },
                    "next_tool": { "type": "string" },
                    "policy": { "type": "string" }
                },
                "required": ["history", "next_tool"]
            }
        }),
        serde_json::json!({
            "name": "assay_policy_decide",
            "description": "Check if a tool is blocked by policy.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool": { "type": "string" },
                    "policy": { "type": "string" }
                },
                "required": ["tool", "policy"]
            }
        }),
    ]
}

pub async fn handle_call(ctx: &ToolContext, name: &str, args: &Value) -> anyhow::Result<Value> {
    match name {
        "assay_check_args" => check_args::check_args(ctx, args).await,
        "assay_check_sequence" => check_sequence::check_sequence(ctx, args).await,
        "assay_policy_decide" => policy_decide::policy_decide(ctx, args).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    }
}
