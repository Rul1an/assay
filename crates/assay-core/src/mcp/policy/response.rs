use super::super::jsonrpc::{ContentItem, JsonRpcResponse, ToolCallResult, ToolResultBody};
use serde_json::Value;

pub fn make_deny_response(id: Value, msg: &str, contract: Value) -> String {
    let body = ToolResultBody {
        content: vec![ContentItem::Text {
            text: msg.to_string(),
        }],
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
