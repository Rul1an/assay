use super::identity::ToolIdentity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpInputFormat {
    Inspector,
    JsonRpc,
    StreamableHttp,
    HttpSse,
}

impl McpInputFormat {
    pub fn from_cli_label(label: &str) -> Option<Self> {
        match label {
            "inspector" | "mcp-inspector" | "mcp-inspector@v1" => Some(Self::Inspector),
            "jsonrpc" => Some(Self::JsonRpc),
            "streamable-http" => Some(Self::StreamableHttp),
            "http-sse" | "sse-legacy" => Some(Self::HttpSse),
            _ => None,
        }
    }
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
    /// Cryptographic identity (computed at runtime or pinned in policy)
    #[serde(default)]
    pub tool_identity: Option<ToolIdentity>,
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

    /// Bounded MCP authorization-discovery summary observed on this event path.
    pub auth_discovery: McpAuthorizationDiscovery,

    pub payload: McpPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct McpAuthorizationDiscovery {
    pub visible: bool,
    pub source_kind: McpAuthorizationDiscoverySourceKind,
    pub resource_metadata_visible: bool,
    pub authorization_servers_visible: bool,
    pub scope_challenge_visible: bool,
}

impl McpAuthorizationDiscovery {
    pub fn merge_from(&mut self, other: &Self) {
        self.visible |= other.visible;
        self.resource_metadata_visible |= other.resource_metadata_visible;
        self.authorization_servers_visible |= other.authorization_servers_visible;
        self.scope_challenge_visible |= other.scope_challenge_visible;

        if self.source_kind == McpAuthorizationDiscoverySourceKind::Unknown
            && other.source_kind != McpAuthorizationDiscoverySourceKind::Unknown
        {
            self.source_kind = other.source_kind;
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpAuthorizationDiscoverySourceKind {
    #[default]
    Unknown,
    WwwAuthenticate,
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
