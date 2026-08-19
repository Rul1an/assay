//! Complete MCP 2026-07-28 stateless server adapter, compiled in and unreachable
//! from public stdio dispatch.
//!
//! Existence is not reachability. [`crate::server::Server::run`] never calls this
//! module. That absence is the closed gate: not a config flag and not an env var.

use crate::server::{ERROR_METHOD_NOT_FOUND, ERROR_UNSUPPORTED_PROTOCOL_VERSION};

#[allow(deprecated)]
use crate::server::MODERN_PROTOCOL_VERSION;
use crate::tools::{self, ToolContext};
use serde_json::{json, Value};

const PROTOCOL_VERSION_META_KEY: &str = "io.modelcontextprotocol/protocolVersion";
const CLIENT_CAPABILITIES_META_KEY: &str = "io.modelcontextprotocol/clientCapabilities";
const TOOL_EXECUTION_FAILED: &str = "Tool execution failed";

/// JSON-RPC invalid params. Used when required per-request metadata is absent.
pub const ERROR_INVALID_PARAMS: i32 = -32602;

/// Conservative initial cache policy. Change only with explicit evidence.
pub struct CachePolicy {
    pub ttl_ms: u64,
    pub scope: &'static str,
}

impl CachePolicy {
    pub const INITIAL: Self = Self {
        ttl_ms: 0,
        scope: "private",
    };
}

pub const CACHE_TTL_MS: u64 = CachePolicy::INITIAL.ttl_ms;
pub const CACHE_SCOPE: &str = CachePolicy::INITIAL.scope;

/// Zero-sized adapter: no session, no initialize, no `Mcp-Session-Id`.
#[derive(Clone, Copy, Default)]
pub struct StatelessAdapter;

impl StatelessAdapter {
    pub fn new() -> Self {
        Self
    }

    pub async fn serve(&self, request: &Value, ctx: Option<&ToolContext>) -> Value {
        serve(request, ctx).await
    }
}

fn modern_version() -> &'static str {
    #[allow(deprecated)]
    {
        MODERN_PROTOCOL_VERSION
    }
}

/// Serve one modern request. Any instance can serve any request.
pub async fn serve(request: &Value, ctx: Option<&ToolContext>) -> Value {
    let id = request.get("id").cloned();
    match validate_request_metadata(request) {
        Ok(()) => {}
        Err(AdapterFault::InvalidParams) => {
            return error_response(id, ERROR_INVALID_PARAMS, "Invalid params", None);
        }
        Err(AdapterFault::UnsupportedVersion(requested)) => {
            return error_response(
                id,
                ERROR_UNSUPPORTED_PROTOCOL_VERSION,
                "unsupported protocol version",
                Some(json!({
                    "requested": requested,
                    "supported": [modern_version()],
                })),
            );
        }
    }

    match request.get("method").and_then(Value::as_str) {
        Some("server/discover") => ok_response(id, discover_result()),
        Some("tools/list") => ok_response(id, list_tools_result()),
        Some("tools/call") => match ctx {
            Some(ctx) => ok_response(id, call_tool(request, ctx).await),
            None => error_response(id, ERROR_INVALID_PARAMS, "Invalid params", None),
        },
        _ => error_response(id, ERROR_METHOD_NOT_FOUND, "Method not found", None),
    }
}

enum AdapterFault {
    InvalidParams,
    UnsupportedVersion(String),
}

fn validate_request_metadata(request: &Value) -> Result<(), AdapterFault> {
    let Some(meta) = request
        .get("params")
        .and_then(Value::as_object)
        .and_then(|params| params.get("_meta"))
        .and_then(Value::as_object)
    else {
        return Err(AdapterFault::InvalidParams);
    };

    match meta.get(PROTOCOL_VERSION_META_KEY).and_then(Value::as_str) {
        None => return Err(AdapterFault::InvalidParams),
        Some(version) if version != modern_version() => {
            return Err(AdapterFault::UnsupportedVersion(version.to_string()));
        }
        Some(_) => {}
    }

    match meta.get(CLIENT_CAPABILITIES_META_KEY) {
        Some(Value::Object(_)) => Ok(()),
        _ => Err(AdapterFault::InvalidParams),
    }
}

fn cacheable_members() -> Value {
    json!({
        "resultType": "complete",
        "ttlMs": CACHE_TTL_MS,
        "cacheScope": CACHE_SCOPE,
    })
}

fn discover_result() -> Value {
    let mut result = cacheable_members();
    result["capabilities"] = json!({ "tools": {} });
    result["supportedVersions"] = json!([modern_version()]);
    result
}

fn list_tools_result() -> Value {
    let mut result = cacheable_members();
    result["tools"] = Value::Array(tools::list_tools());
    result
}

async fn call_tool(request: &Value, ctx: &ToolContext) -> Value {
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let default_args = json!({});
    let args = params.get("arguments").unwrap_or(&default_args);
    let payload = match tools::handle_call(ctx, name, args).await {
        Ok(value) => value,
        Err(_) => json!({
            "allowed": false,
            "error": {
                "code": "E_INTERNAL",
                "message": TOOL_EXECUTION_FAILED
            }
        }),
    };
    let allowed = payload.get("allowed").and_then(Value::as_bool);
    let is_error = payload.get("error").is_some() || allowed == Some(false);
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&payload).unwrap_or_default()
        }],
        "isError": is_error,
        "resultType": "complete"
    })
}

fn ok_response(id: Option<Value>, result: Value) -> Value {
    let mut response = json!({
        "jsonrpc": "2.0",
        "result": result
    });
    if let Some(id) = id {
        response["id"] = id;
    }
    response
}

fn error_response(id: Option<Value>, code: i32, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({
        "code": code,
        "message": message
    });
    if let Some(data) = data {
        error["data"] = data;
    }
    let mut response = json!({
        "jsonrpc": "2.0",
        "error": error
    });
    if let Some(id) = id {
        response["id"] = id;
    }
    response
}
