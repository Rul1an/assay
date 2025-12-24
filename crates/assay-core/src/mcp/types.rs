use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpInputFormat {
    Inspector,
    JsonRpc,
}

/// Minimal MCP tool definition (enough for tools/list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// MCP often uses JSON Schema-like shapes; keep as Value.
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

/// Wrapper for MCP events with metadata needed for deterministic sorting (P0.3).
#[derive(Debug, Clone)]
pub struct McpEvent {
    /// Line number in original file (JSON-RPC) or array index (Inspector).
    /// Used as stable sort fallback.
    pub source_line: u64,

    /// Best-effort timestamp in milliseconds.
    pub timestamp_ms: Option<u64>,

    /// JSON-RPC ID (stringified) for request/response correlation (P0.2).
    pub jsonrpc_id: Option<String>,

    pub payload: McpPayload,
}

#[derive(Debug, Clone)]
pub enum McpPayload {
    SessionStart {
        raw: serde_json::Value,
    },

    ToolsListRequest {
        raw: serde_json::Value,
    },
    ToolsListResponse {
        tools: Vec<McpToolDef>,
        raw: serde_json::Value,
    },

    ToolCallRequest {
        name: String,
        arguments: serde_json::Value,
        raw: serde_json::Value,
    },
    ToolCallResponse {
        result: serde_json::Value,
        is_error: bool,
        raw: serde_json::Value,
    },

    SessionEnd {
        raw: serde_json::Value,
    },

    /// Anything else for forward-compat.
    Other {
        raw: serde_json::Value,
    },
}
