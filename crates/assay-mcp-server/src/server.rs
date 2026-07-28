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

#[derive(Clone, Copy)]
enum LegacyProtocolVersion {
    V2024_11_05,
    V2025_11_25,
}

impl LegacyProtocolVersion {
    const LATEST: Self = Self::V2025_11_25;

    fn negotiate(params: Option<&Value>) -> Option<Self> {
        let requested = params
            .and_then(Value::as_object)
            .and_then(|params| params.get("protocolVersion"))
            .and_then(Value::as_str)?;

        Some(match requested {
            "2024-11-05" => Self::V2024_11_05,
            "2025-11-25" => Self::V2025_11_25,
            _ => Self::LATEST,
        })
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::V2024_11_05 => "2024-11-05",
            Self::V2025_11_25 => "2025-11-25",
        }
    }
}

/// The `initialize` result this server returns on a successful legacy handshake.
///
/// Extracted so the claims boundary is testable rather than remembered. ADR-042 refuses
/// compliance and partnership claims; ADR-043 §2 extends that refusal from prose to what the
/// binary puts on the wire. This response previously carried a `meta` object asserting
/// `certified: true` and a `partner`, unconditionally, on every successful handshake including
/// sessions that had just failed authentication under the default permissive mode. Neither
/// literal carried a basis a reviewer could check, so neither is emitted.
///
/// Server-specific metadata, if it is ever needed, belongs under the protocol's reserved
/// `_meta` key with a non-reserved prefix such as `dev.assay/`; a bare `meta` object was never
/// part of the protocol.
fn initialize_result(protocol_version: LegacyProtocolVersion) -> Value {
    serde_json::json!({
        // MCP 2026-07-28 removed this handshake. Modern support requires server/discover and
        // per-request metadata; this legacy path deliberately advertises neither.
        "protocolVersion": protocol_version.as_str(),
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "assay-mcp-server",
            // Derived from the crate, not written by hand. The previous literal "0.4.0" was
            // an identity claim no build could substantiate, which is the same defect as the
            // status claims this function was cleaned up to remove.
            "version": env!("CARGO_PKG_VERSION")
        }
    })
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
        crate::config::reject_unsupported_stdio_auth_env()?;

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
                "initialize" => match LegacyProtocolVersion::negotiate(req.params.as_ref()) {
                    Some(version) => {
                        JsonRpcResponse::ok(req.id.clone(), initialize_result(version))
                    }
                    None => JsonRpcResponse::error(
                        req.id.clone(),
                        -32602,
                        "Invalid initialize params: protocolVersion must be a string".to_string(),
                    ),
                },
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

                        // Metered billing telemetry
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

                        // P57b: emit the observed tool decision (assay.tool_decision_surface.v0) as
                        // its own structured event. Redaction and the asserted-vs-verified rule are
                        // enforced inside build_decision; this site never has SaaS-verified evidence.
                        {
                            use crate::tool_decision::{build_decision, Effect, ObservedCall};
                            let (effect, status) = match &result {
                                Ok(val) => {
                                    let allowed = val
                                        .get("allowed")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false);
                                    if let Some(code) = val
                                        .get("error")
                                        .and_then(|e| e.get("code"))
                                        .and_then(|v| v.as_str())
                                    {
                                        (Effect::Error, code.to_string())
                                    } else if allowed {
                                        (Effect::Allow, "success".to_string())
                                    } else {
                                        (Effect::Deny, "blocked".to_string())
                                    }
                                }
                                Err(_) => (Effect::Error, "crash".to_string()),
                            };
                            let decision = build_decision(&ObservedCall {
                                server_id: "mcp",
                                tool_name: name,
                                // Inspected transiently by the classifier to project named target
                                // fields (hashed); never copied into the record verbatim.
                                args,
                                effect,
                                status: &status,
                                rule_id: None,
                            });
                            tracing::info!(
                                event = "tool_decision",
                                rid = %rid,
                                rpc_id = ?req.id,
                                decision = %serde_json::to_string(&decision).unwrap_or_default(),
                            );
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

                                // Agent Awareness
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

#[cfg(test)]
mod claims_boundary_tests {
    use super::{initialize_result, LegacyProtocolVersion};

    /// ADR-042 stop list, ADR-043 §2. The handshake must not assert a status the server
    /// cannot substantiate. A denylist over the serialized response catches a claim
    /// reintroduced anywhere in the object under any nesting, but it only knows the words
    /// it was given, so it is the backstop here and not the primary control. The primary
    /// control is `initialize_result_pins_every_value`, which pins the leaves.
    #[test]
    fn initialize_result_asserts_no_unearned_status() {
        let wire = serde_json::to_string(&initialize_result(LegacyProtocolVersion::LATEST))
            .expect("serializable");
        for forbidden in [
            "certified",
            "certification",
            "partner",
            "compliant",
            "compliance",
            "approved",
            "endorsed",
            "accredited",
        ] {
            assert!(
                !wire.to_ascii_lowercase().contains(forbidden),
                "initialize result asserts `{forbidden}` without a checkable basis: {wire}"
            );
        }
    }

    /// Pinning paths is not enough, because a claim can live in a *value* on a permitted
    /// path: `serverInfo.version: "0.4.0 (SOC 2 Type II attested)"` satisfies both the
    /// allowlist and the denylist, since "attested" is not one of the eight words. The
    /// response has four leaves, so pin all four. Anything asserted on the wire then has to
    /// pass through this test, and the denylist is left as a backstop for the case where
    /// someone updates a pinned value without thinking about what it claims.
    #[test]
    fn initialize_result_pins_every_value() {
        for (version, expected) in [
            (LegacyProtocolVersion::V2024_11_05, "2024-11-05"),
            (LegacyProtocolVersion::V2025_11_25, "2025-11-25"),
        ] {
            let value = initialize_result(version);
            assert_eq!(
                value.get("protocolVersion").and_then(|v| v.as_str()),
                Some(expected)
            );
            assert_eq!(
                value.get("capabilities"),
                Some(&serde_json::json!({"tools": {}}))
            );
            assert_eq!(
                value
                    .get("serverInfo")
                    .and_then(|s| s.get("name"))
                    .and_then(|v| v.as_str()),
                Some("assay-mcp-server")
            );
            // Derived, not asserted. The literal "0.4.0" was itself an unverifiable identity
            // claim: the crate has been on 3.x for a long time, so the wire was stating a
            // version no build ever produced.
            assert_eq!(
                value
                    .get("serverInfo")
                    .and_then(|s| s.get("version"))
                    .and_then(|v| v.as_str()),
                Some(env!("CARGO_PKG_VERSION"))
            );
        }
    }

    /// The bare `meta` key was never part of the protocol. If server metadata returns it
    /// belongs under the reserved `_meta` key with a non-reserved prefix.
    #[test]
    fn initialize_result_has_no_bare_meta_object() {
        let value = initialize_result(LegacyProtocolVersion::LATEST);
        let obj = value.as_object().expect("result is an object");
        assert!(!obj.contains_key("meta"), "bare `meta` key reintroduced");
    }

    /// A denylist alone only catches the words we thought of. The response surface is small
    /// and closed, so pin it: anything new on the wire has to be added here deliberately,
    /// which is the point at which someone has to justify its basis.
    ///
    /// The walk is recursive on purpose. Pinning only the top level would leave
    /// `serverInfo.vendorStatus` free to appear, which is the same defect one level down.
    #[test]
    fn initialize_result_surface_is_a_closed_set() {
        const PERMITTED: &[&str] = &[
            "protocolVersion",
            "capabilities",
            "capabilities.tools",
            "serverInfo",
            "serverInfo.name",
            "serverInfo.version",
        ];

        // Descends through arrays as well as objects. An array element is addressed as
        // `path[]` rather than `path[0]`, so the allowlist pins shape and not length: a
        // future `capabilities.tools: [{"newField": true}]` surfaces as
        // `capabilities.tools[].newField` and fails, instead of hiding inside a node the
        // walk never entered.
        fn walk(value: &serde_json::Value, prefix: &str, found: &mut Vec<String>) {
            match value {
                serde_json::Value::Object(obj) => {
                    for (key, child) in obj {
                        let path = if prefix.is_empty() {
                            key.clone()
                        } else {
                            format!("{prefix}.{key}")
                        };
                        found.push(path.clone());
                        walk(child, &path, found);
                    }
                }
                serde_json::Value::Array(items) => {
                    let path = format!("{prefix}[]");
                    for child in items {
                        walk(child, &path, found);
                    }
                }
                _ => {}
            }
        }

        let value = initialize_result(LegacyProtocolVersion::LATEST);
        let mut found = Vec::new();
        walk(&value, "", &mut found);

        for path in &found {
            assert!(
                PERMITTED.contains(&path.as_str()),
                "`{path}` was added to the initialize result without being declared here; \
                 add it to PERMITTED only once its basis is checkable (ADR-042, ADR-043 §2)"
            );
        }
        // Guard the guard: if the response shrinks, the allowlist must shrink with it, or it
        // stops describing what actually goes on the wire.
        for permitted in PERMITTED {
            assert!(
                found.iter().any(|p| p == permitted),
                "`{permitted}` is declared permitted but no longer emitted; prune it"
            );
        }
    }

    #[test]
    fn initialize_result_still_carries_the_fields_a_client_needs() {
        let value = initialize_result(LegacyProtocolVersion::LATEST);
        assert!(value.get("protocolVersion").is_some());
        assert!(value.get("capabilities").is_some());
        assert_eq!(
            value
                .get("serverInfo")
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str()),
            Some("assay-mcp-server")
        );
    }
}
