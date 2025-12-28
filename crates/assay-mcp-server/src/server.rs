use crate::config::ServerConfig;
use crate::tools::{self};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::timeout;

static RID: AtomicU64 = AtomicU64::new(1);

fn next_rid() -> String {
    let n = RID.fetch_add(1, Ordering::Relaxed);
    format!("r-{n:06}")
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    params: Option<Value>,
    id: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    fn error(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
            id,
        }
    }
}

pub struct Server;

use crate::cache::PolicyCaches;

impl Server {
    pub async fn run(policy_root: std::path::PathBuf, cfg: ServerConfig) -> Result<()> {
        let caches = PolicyCaches::new(cfg.cache_entries);

        // Canonicalize root once
        let policy_root_canon = std::fs::canonicalize(&policy_root)
            .map_err(|e| anyhow::anyhow!("invalid --policy-root: {e}"))?;

        let ctx = tools::ToolContext {
            policy_root,
            policy_root_canon,
            cfg: cfg.clone(),
            caches,
        };
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        for line in stdin.lock().lines() {
            let line = line?;
            let rid = next_rid();

            if line.len() > cfg.max_msg_bytes {
                tracing::warn!(
                    target: "assay_mcp_server",
                    event="limit_exceeded",
                    rid=%rid,
                    bytes_in=line.len(),
                    max=cfg.max_msg_bytes
                );

                let resp = JsonRpcResponse::ok(
                    None,
                    serde_json::json!({
                        "allowed": false,
                        "error": {
                            "code": "E_LIMIT_EXCEEDED",
                            "message": format!("message bytes={} > max={}", line.len(), cfg.max_msg_bytes)
                        }
                    }),
                );
                let resp_json = serde_json::to_string(&resp)?;
                writeln!(stdout, "{}", resp_json)?;
                stdout.flush()?;
                continue;
            }

            if line.trim().is_empty() {
                continue;
            }

            // Parse Request
            let req: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(
                        event="json_parse_error",
                        rid=%rid,
                        error=%e
                    );
                    continue; // Ignore invalid JSON lines (stdio transport robustness)
                }
            };

            // Dispatch
            let resp = match req.method.as_str() {
                "initialize" => {
                    let caps = serde_json::json!({
                        "protocolVersion": "2024-11-05",
                        "capabilities": {
                            "tools": {}
                        },
                        "serverInfo": {
                            "name": "assay-mcp-server",
                            "version": "0.4.0"
                        },
                        "meta": {
                            "certified": true,
                            "partner": "celonis_agentc"
                        }
                    });
                    JsonRpcResponse::ok(req.id.clone(), caps)
                }
                "notifications/initialized" => {
                    // Notification, no response needed usually, but good to ack log
                    tracing::info!(event="initialized", rid=%rid);
                    continue;
                }
                "tools/list" => {
                    let tool_list = tools::list_tools();
                    JsonRpcResponse::ok(
                        req.id.clone(),
                        serde_json::json!({
                            "tools": tool_list
                        }),
                    )
                }
                "tools/call" => {
                    if let Some(params) = req.params {
                        let name = params.get("name").and_then(|s| s.as_str()).unwrap_or("");
                        let default_args = serde_json::json!({});
                        let args = params.get("arguments").unwrap_or(&default_args);

                        let on_error_str = args
                            .get("on_error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("block");
                        let allow_on_error = on_error_str.eq_ignore_ascii_case("allow");

                        let policy = args.get("policy").and_then(|v| v.as_str()).unwrap_or("");
                        let bytes_in = line.len();
                        let args_bytes = serde_json::to_vec(args).map(|b| b.len()).unwrap_or(0);

                        let start = std::time::Instant::now();

                        tracing::info!(
                           event="tool_call_start",
                           rid=%rid,
                           rpc_id=?req.id,
                           tool=name,
                           policy=policy,
                           on_error=on_error_str,
                           bytes_in=bytes_in,
                           args_bytes=args_bytes,
                        );

                        // Metered Billing Telemetry (Celonis Requirement)
                        assay_metrics::usage::log_usage_event("policy_check", 1);

                        // Execute with timeout
                        let fut = tools::handle_call(&ctx, name, args);
                        let result = match timeout(Duration::from_millis(cfg.timeout_ms), fut).await
                        {
                            Ok(res) => res, // Tool finished
                            Err(_) => {
                                let dur = start.elapsed().as_millis() as u64;
                                tracing::warn!(
                                   event="tool_call_timeout",
                                   rid=%rid,
                                   rpc_id=?req.id,
                                   tool=name,
                                   policy=policy,
                                   duration_ms=dur,
                                   code="E_TIMEOUT",
                                   fallback=on_error_str
                                );
                                // Timed out
                                Ok(serde_json::json!({
                                    "allowed": allow_on_error,
                                    "error": {
                                        "code": "E_TIMEOUT",
                                        "message": format!("Request exceeded {}ms", cfg.timeout_ms)
                                    }
                                }))
                            }
                        };

                        let dur = start.elapsed().as_millis() as u64;
                        // Log outcome
                        match &result {
                            Ok(val) => {
                                let allowed = val
                                    .get("allowed")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                if let Some(err) = val.get("error") {
                                    let code =
                                        err.get("code").and_then(|v| v.as_str()).unwrap_or("");
                                    tracing::info!(
                                      event="tool_call_done",
                                      rid=%rid,
                                      rpc_id=?req.id,
                                      tool=name,
                                      policy=policy,
                                      duration_ms=dur,
                                      outcome="app_error",
                                      allowed=allowed,
                                      code=code
                                    );
                                } else {
                                    tracing::info!(
                                      event="tool_call_done",
                                      rid=%rid,
                                      rpc_id=?req.id,
                                      tool=name,
                                      policy=policy,
                                      duration_ms=dur,
                                      outcome="ok",
                                      allowed=allowed
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                  event="tool_call_crash",
                                  rid=%rid,
                                  rpc_id=?req.id,
                                  tool=name,
                                  policy=policy,
                                  duration_ms=dur,
                                  error=%e
                                );
                            }
                        }

                        match result {
                            Ok(res) => {
                                // MCP Compliance: Wrap result in CallToolResult structure
                                // Spec: { content: [{ type: "text", text: "..." }], isError: bool }
                                let is_error =
                                    !res.get("allowed").and_then(|v| v.as_bool()).unwrap_or(true);
                                let json_text =
                                    serde_json::to_string_pretty(&res).unwrap_or_default();

                                let mcp_result = serde_json::json!({
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": json_text
                                        }
                                    ],
                                    "isError": is_error
                                });
                                JsonRpcResponse::ok(req.id.clone(), mcp_result)
                            }
                            Err(e) => {
                                // Fail-safe handling for internal errors
                                tracing::error!(
                                    event="tool_execution_error",
                                    rid=%rid,
                                    error=%e,
                                    fallback=on_error_str
                                );
                                let mut safe_resp = serde_json::json!({
                                    "allowed": allow_on_error,
                                    "error": {
                                        "code": "E_INTERNAL",
                                        "message": e.to_string()
                                    }
                                });

                                // Celonis Feature: Agent Awareness
                                // If we fail open, warn the agent so it can self-regulate (e.g. switch to Safe Mode).
                                if allow_on_error {
                                    safe_resp["warning"] = serde_json::json!("FAIL-SAFE ACTIVE: Policy engine offline. Proceed with caution (Safe Mode).");
                                }
                                // Keep consistent wrapping even for internal fail-safe responses
                                let json_text =
                                    serde_json::to_string_pretty(&safe_resp).unwrap_or_default();
                                let mcp_result = serde_json::json!({
                                    "content": [
                                        {
                                            "type": "text",
                                            "text": json_text
                                        }
                                    ],
                                    "isError": !allow_on_error
                                });
                                JsonRpcResponse::ok(req.id.clone(), mcp_result)
                            }
                        }
                    } else {
                        JsonRpcResponse::error(req.id.clone(), -32602, "Missing params".to_string())
                    }
                }
                _ => JsonRpcResponse::error(
                    req.id.clone(),
                    -32601,
                    format!("Method not found: {}", req.method),
                ),
            };

            // Send Response
            let resp_json = serde_json::to_string(&resp)?;
            writeln!(stdout, "{}", resp_json)?;
            stdout.flush()?;
        }

        Ok(())
    }
}
