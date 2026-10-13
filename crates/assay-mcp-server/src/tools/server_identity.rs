//! `assay_server_identity`: the `serverInfo` this server reports from `initialize`, readable as a
//! tool result by a client that only sees tool results.
//!
//! It is the server's self-report of the name and version it was compiled as. It does not identify
//! the binary file or its path, prove which bytes are loaded, say where the client configuration
//! that started the server came from, or attest the build. It reads no policy, configuration,
//! credential or network input.

use super::ToolError;
use anyhow::Result;
use serde_json::Value;

/// The server name reported in `serverInfo`.
pub const SERVER_NAME: &str = "assay-mcp-server";

/// The tool's advertised name, used by the catalogue, the dispatch and the outer loop's decision
/// and metering guard, so all three name the same tool.
pub const TOOL_NAME: &str = "assay_server_identity";

/// The single source of `serverInfo`. `initialize` and `assay_server_identity` both call this, so
/// the two answers cannot drift apart.
pub fn server_info() -> Value {
    serde_json::json!({
        "name": SERVER_NAME,
        // Derived from the crate, not written by hand. The earlier literal "0.4.0" was an
        // identity claim no build could substantiate.
        "version": env!("CARGO_PKG_VERSION")
    })
}

/// Returns [`server_info`] for an empty argument object and refuses anything else, without
/// echoing what was supplied. The refusal carries only `error`: unlike the policy tools'
/// [`ToolError::result`], it has no `allowed` member, because no policy verdict was reached.
pub async fn server_identity(args: &Value) -> Result<Value> {
    match args.as_object() {
        Some(arguments) if arguments.is_empty() => Ok(server_info()),
        _ => Ok(serde_json::json!({
            "error": ToolError::new("E_INVALID_ARGS", "assay_server_identity takes no arguments")
        })),
    }
}
