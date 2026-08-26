use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Whether a uniquely-parsed JSON-RPC method is a `tools/call`.
///
/// Protocol-control methods (`initialize`, `tools/list`, notifications, `ping`)
/// are not. One function so wrap and proxy-enforce cannot drift.
pub fn is_tools_call_method(method: &str) -> bool {
    method == "tools/call"
}

impl JsonRpcRequest {
    pub fn is_tool_call(&self) -> bool {
        is_tools_call_method(&self.method)
    }

    pub fn tool_params(&self) -> Option<CallToolParams> {
        if !self.is_tool_call() {
            return None;
        }
        serde_json::from_value(self.params.clone()).ok()
    }
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(flatten)]
    pub payload: T,
}

// Specific payloads
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    pub result: ToolResultBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultBody {
    pub content: Vec<ContentItem>,
    #[serde(rename = "isError")]
    pub is_error: bool,
    #[serde(rename = "structuredContent", skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ContentItem {
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Debug, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
}
