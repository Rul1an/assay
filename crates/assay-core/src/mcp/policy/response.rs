use super::super::jsonrpc::{ContentItem, JsonRpcResponse, ToolCallResult, ToolResultBody};
use serde::Serialize;
use serde_json::Value;

pub fn make_deny_response(id: Value, msg: &str, contract: Value) -> String {
    let body = deny_body(msg, contract);
    let resp = JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        payload: ToolCallResult { result: body },
    };
    serde_json::to_string(&resp).unwrap_or_default() + "\n"
}

/// [`make_deny_response`] for a request whose declared revision requires `resultType`.
///
/// A separate shape because `ToolResultBody` is a public struct that cannot gain a field on 6.x.
pub(crate) fn make_typed_deny_response(id: Value, msg: &str, contract: Value) -> String {
    #[derive(Serialize)]
    struct Typed {
        #[serde(rename = "resultType")]
        result_type: &'static str,
        #[serde(flatten)]
        body: ToolResultBody,
    }
    #[derive(Serialize)]
    struct Payload {
        result: Typed,
    }
    let resp = JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        payload: Payload {
            result: Typed {
                result_type: "complete",
                body: deny_body(msg, contract),
            },
        },
    };
    serde_json::to_string(&resp).unwrap_or_default() + "\n"
}

fn deny_body(msg: &str, contract: Value) -> ToolResultBody {
    ToolResultBody {
        content: vec![ContentItem::Text {
            text: msg.to_string(),
        }],
        is_error: true,
        structured_content: Some(contract),
    }
}
