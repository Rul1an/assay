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
const INVALID_INITIALIZE_PARAMS: &str =
    "Invalid initialize params: expected the required legacy MCP fields";

/// The messages the fail-closed tool path puts on the wire.
///
/// Named so the claims-boundary guard reads the value that ships. Inline literals at the call
/// sites would have left the guard asserting over its own copies, which is a neighbouring
/// property: the shipped message could gain a status claim while the test stayed green.
const TOOL_EXECUTION_FAILED: &str = "Tool execution failed";
const TOOL_EXECUTION_TIMED_OUT: &str = "Tool execution timed out";

fn fail_closed_tool_result(code: &'static str, message: &'static str) -> Result<Value> {
    tools::ToolError::new(code, message).result()
}

fn classify_tool_result(result: &Value) -> (bool, bool) {
    let has_error = result.get("error").is_some();
    let explicit_allowed = result.get("allowed").and_then(Value::as_bool);
    let allowed = explicit_allowed.unwrap_or(false);
    let is_error = has_error || explicit_allowed == Some(false);
    (allowed, is_error)
}

fn next_rid() -> String {
    let n = RID.fetch_add(1, Ordering::Relaxed);
    format!("r-{n:06}")
}

/// MCP revision `2026-07-28`, which `assay-mcp-server` does not implement.
///
/// Retained at its 5.0.0 path, type, and value only so that code compiled against
/// `assay-mcp-server` 5.0.0 keeps compiling. It is an API compatibility artifact, not a capability
/// claim: the constant is absent from the accepted version set, is never advertised on the wire,
/// and no accept or dispatch path reads it. A request declaring `2026-07-28` in `_meta` is refused
/// with a typed `-32022` before dispatch, and `server/discover`, which that revision makes
/// mandatory, returns `-32601`.
#[deprecated(
    note = "assay-mcp-server does not implement MCP revision 2026-07-28; this constant is retained only for API compatibility with 5.0.0"
)]
pub const MODERN_PROTOCOL_VERSION: &str = "2026-07-28";

/// The reserved `_meta` key a request may use to declare its revision.
const PROTOCOL_VERSION_META_KEY: &str = "io.modelcontextprotocol/protocolVersion";

/// `UnsupportedProtocolVersionError`, per the 2026-07-28 versioning spec.
pub const ERROR_UNSUPPORTED_PROTOCOL_VERSION: i32 = -32022;

/// JSON-RPC `Method not found`. `server/discover` stays on this code while the gate is closed.
pub const ERROR_METHOD_NOT_FOUND: i32 = -32601;

/// Every protocol revision this server accepts on the public wire, oldest to newest.
///
/// Independent of [`MODERN_PROTOCOL_VERSION`]. Adding that name here is the #2483 tripwire.
pub const ACCEPTED_PROTOCOL_VERSIONS: &[&str] = &[
    LegacyProtocolVersion::V2024_11_05.as_str(),
    LegacyProtocolVersion::V2025_06_18.as_str(),
    LegacyProtocolVersion::V2025_11_25.as_str(),
];

/// Refuse a request that declares a revision this legacy server does not implement.
///
/// `Err` carries the JSON-RPC error data the spec requires: what was requested and what is
/// supported, so a client can fall forward without a second round trip.
fn check_request_version(params: Option<&Value>) -> Result<(), Value> {
    let Some(requested) = params
        .and_then(Value::as_object)
        .and_then(|p| p.get("_meta"))
        .and_then(Value::as_object)
        .and_then(|m| m.get(PROTOCOL_VERSION_META_KEY))
        .and_then(Value::as_str)
    else {
        return Ok(());
    };
    if LegacyProtocolVersion::parse(requested).is_some() {
        return Ok(());
    }
    Err(serde_json::json!({
        "requested": requested,
        "supported": LegacyProtocolVersion::supported(),
    }))
}

#[derive(Clone, Copy)]
enum LegacyProtocolVersion {
    V2024_11_05,
    V2025_06_18,
    V2025_11_25,
}

impl LegacyProtocolVersion {
    const LATEST: Self = Self::V2025_11_25;

    /// Every legacy revision this server implements, oldest to newest.
    const ALL: &[Self] = &[Self::V2024_11_05, Self::V2025_06_18, Self::V2025_11_25];

    fn parse(requested: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|version| version.as_str() == requested)
    }

    fn supported() -> Vec<&'static str> {
        ACCEPTED_PROTOCOL_VERSIONS.to_vec()
    }

    fn negotiate(params: Option<&Value>) -> Result<Self, ()> {
        let params = params.and_then(Value::as_object).ok_or(())?;
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or(())?;
        params
            .get("capabilities")
            .and_then(Value::as_object)
            .ok_or(())?;
        let client_info = params
            .get("clientInfo")
            .and_then(Value::as_object)
            .ok_or(())?;
        client_info.get("name").and_then(Value::as_str).ok_or(())?;
        client_info
            .get("version")
            .and_then(Value::as_str)
            .ok_or(())?;

        Ok(Self::parse(requested).unwrap_or(Self::LATEST))
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::V2024_11_05 => "2024-11-05",
            Self::V2025_06_18 => "2025-06-18",
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

    /// A JSON-RPC error carrying structured `data`.
    ///
    /// `UnsupportedProtocolVersionError` is only useful with it: the spec requires the requested
    /// and supported versions so a client can fall forward without a second round trip.
    fn error_with_data(id: Option<Value>, code: i32, message: String, data: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: Some(data),
            }),
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
        anyhow::ensure!(
            policy_root_canon.is_dir(),
            "invalid --policy-root: not a directory: {}",
            policy_root.display()
        );

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

                // Pre-parse: Request id is unknown, so JSON-RPC requires id null. This is a
                // transport refusal (-32000 server error), not CallToolResult / tool-domain
                // E_LIMIT_EXCEEDED, and must not reflect the rejected line.
                let resp = JsonRpcResponse::error_with_data(
                    None,
                    -32000,
                    "Message too large".to_string(),
                    serde_json::json!({
                        "kind": "transport_limit",
                        "limit": cfg.max_msg_bytes,
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

            // Revision before dispatch. A revision this server does not implement is refused by name,
            // with the supported set in `data`, rather than served as whatever the method happens
            // to do -- which is the silent best-effort the 2026-07-28 spec replaced.
            if let Err(data) = check_request_version(req.params.as_ref()) {
                let resp = JsonRpcResponse::error_with_data(
                    req.id.clone(),
                    ERROR_UNSUPPORTED_PROTOCOL_VERSION,
                    "unsupported protocol version".to_string(),
                    data,
                );
                let resp_json = serde_json::to_string(&resp)?;
                writeln!(stdout, "{}", resp_json)?;
                stdout.flush()?;
                continue;
            }

            // Dispatch
            let resp = match req.method.as_str() {
                "initialize" => match LegacyProtocolVersion::negotiate(req.params.as_ref()) {
                    Ok(version) => JsonRpcResponse::ok(req.id.clone(), initialize_result(version)),
                    Err(()) => JsonRpcResponse::error(
                        req.id.clone(),
                        -32602,
                        INVALID_INITIALIZE_PARAMS.to_string(),
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

                        let bytes_in = line.len();
                        let args_bytes = serde_json::to_vec(args).map(|b| b.len()).unwrap_or(0);

                        let start = std::time::Instant::now();

                        tracing::info!(
                           event="tool_call_start",
                           rid=%rid,
                           rpc_id=?req.id,
                           bytes_in=bytes_in,
                           args_bytes=args_bytes,
                        );

                        // Metered billing telemetry
                        assay_metrics::usage::log_usage_event("policy_check", 1);

                        // Execute with timeout
                        let fut = tools::handle_call(&ctx, name, args);
                        let result = match timeout(Duration::from_millis(cfg.timeout_ms), fut).await
                        {
                            Ok(Ok(value)) => value,
                            Ok(Err(_error)) => {
                                tracing::error!(
                                    event = "tool_execution_error",
                                    rid = %rid,
                                    rpc_id = ?req.id,
                                    duration_ms = start.elapsed().as_millis() as u64,
                                    code = "E_INTERNAL"
                                );
                                fail_closed_tool_result("E_INTERNAL", TOOL_EXECUTION_FAILED)?
                            }
                            Err(_) => {
                                let dur = start.elapsed().as_millis() as u64;
                                tracing::warn!(
                                   event="tool_call_timeout",
                                   rid=%rid,
                                   rpc_id=?req.id,
                                   duration_ms=dur,
                                   code="E_TIMEOUT"
                                );
                                fail_closed_tool_result("E_TIMEOUT", TOOL_EXECUTION_TIMED_OUT)?
                            }
                        };

                        let dur = start.elapsed().as_millis() as u64;
                        // Log outcome
                        let (allowed, is_error) = classify_tool_result(&result);
                        if let Some(err) = result.get("error") {
                            let code = err.get("code").and_then(|v| v.as_str()).unwrap_or("");
                            tracing::info!(
                              event="tool_call_done",
                              rid=%rid,
                              rpc_id=?req.id,
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
                              duration_ms=dur,
                              outcome="ok",
                              allowed=allowed
                            );
                        }

                        // P57b: emit the observed tool decision (assay.tool_decision_surface.v0) as
                        // its own structured event. Redaction and the asserted-vs-verified rule are
                        // enforced inside build_decision; this site never has SaaS-verified evidence.
                        {
                            use crate::tool_decision::{build_decision, Effect, ObservedCall};
                            let (effect, status) = if let Some(code) = result
                                .get("error")
                                .and_then(|e| e.get("code"))
                                .and_then(|v| v.as_str())
                            {
                                (Effect::Error, code.to_string())
                            } else if allowed {
                                (Effect::Allow, "success".to_string())
                            } else {
                                (Effect::Deny, "blocked".to_string())
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
                                // SEP-414 (MCP 2026-07-28): trace context travels in `_meta`.
                                // Validation and basis typing happen inside build_decision.
                                traceparent: crate::tool_decision::traceparent_from_params(&params),
                            });
                            tracing::info!(
                                event = "tool_decision",
                                rid = %rid,
                                rpc_id = ?req.id,
                                decision = %serde_json::to_string(&decision).unwrap_or_default(),
                            );
                        }

                        // MCP Compliance: wrap every tool outcome in CallToolResult.
                        let json_text = serde_json::to_string_pretty(&result).unwrap_or_default();
                        let mcp_result = serde_json::json!({
                            "content": [{"type": "text", "text": json_text}],
                            "isError": is_error
                        });
                        JsonRpcResponse::ok(req.id.clone(), mcp_result)
                    } else {
                        JsonRpcResponse::error(req.id.clone(), -32602, "Missing params".to_string())
                    }
                }
                _ => JsonRpcResponse::error(
                    req.id.clone(),
                    ERROR_METHOD_NOT_FOUND,
                    "Method not found".to_string(),
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
    use super::{
        classify_tool_result, fail_closed_tool_result, initialize_result, LegacyProtocolVersion,
        TOOL_EXECUTION_FAILED, TOOL_EXECUTION_TIMED_OUT,
    };

    #[test]
    fn tool_result_classification_separates_decision_from_mcp_error() {
        for (result, expected) in [
            (serde_json::json!({"allowed": true}), (true, false)),
            (serde_json::json!({"allowed": false}), (false, true)),
            (
                serde_json::json!({"allowed": false, "error": {"code": "E_INTERNAL"}}),
                (false, true),
            ),
            // Defence: an error object without `allowed` is still an MCP error. Production
            // `ToolError::result` always sets both, so this row is the only independent
            // witness for the `has_error` arm.
            (
                serde_json::json!({"error": {"code": "E_INTERNAL"}}),
                (false, true),
            ),
            // Report tools return data rather than a policy decision. Preserve the existing
            // decision telemetry while keeping their successful MCP result non-error.
            (serde_json::json!({"report": {}}), (false, false)),
        ] {
            assert_eq!(classify_tool_result(&result), expected, "result: {result}");
        }
    }

    /// ADR-043 §2's closed set of forbidden public wire status claims, in one place.
    ///
    /// It lived inline in the `initialize` test, which made it the stop list for exactly one
    /// response: adding a word covered that response and nothing else, and a second generated
    /// surface could only be covered by writing a second list free to drift from this one
    /// (#2232). One list, one meaning — adding a word here now covers every surface that calls
    /// [`assert_no_unearned_status`].
    const UNEARNED_STATUS_WORDS: [&str; 8] = [
        "certified",
        "certification",
        "partner",
        "compliant",
        "compliance",
        "approved",
        "endorsed",
        "accredited",
    ];

    /// Assert that one Assay-originated generated response asserts no unearned status.
    ///
    /// A denylist over the serialized value catches a claim reintroduced anywhere in the object
    /// under any nesting, but it only knows the words it was given. It is a backstop, never the
    /// primary control: a claim can still live in a *value* on a permitted path, which is what
    /// `initialize_result_pins_every_value` exists to catch for the handshake. Applying this to a
    /// surface is therefore a floor, not a certificate that the surface is fully pinned.
    fn assert_no_unearned_status(label: &str, value: &serde_json::Value) {
        let wire = serde_json::to_string(value).expect("serializable");
        let haystack = wire.to_ascii_lowercase();
        for forbidden in UNEARNED_STATUS_WORDS {
            assert!(
                !haystack.contains(forbidden),
                "{label} asserts `{forbidden}` without a checkable basis: {wire}"
            );
        }
    }

    /// The control for [`assert_no_unearned_status`]. A guard never shown to reject anything
    /// proves nothing, and this one is a denylist, so the thing worth pinning is that it actually
    /// fires — including on a word nested below the top level, which is the shape it exists for.
    #[test]
    #[should_panic(expected = "asserts `certified`")]
    fn unearned_status_rule_rejects_a_nested_claim() {
        assert_no_unearned_status(
            "control",
            &serde_json::json!({"serverInfo": {"name": "assay", "status": "certified"}}),
        );
    }

    /// ADR-042 stop list, ADR-043 §2. The handshake must not assert a status the server cannot
    /// substantiate. Every accepted protocol version is checked, not just `LATEST`: each builds
    /// its own response, so checking one left the others unread.
    #[test]
    fn initialize_result_asserts_no_unearned_status() {
        for version in [
            LegacyProtocolVersion::V2024_11_05,
            LegacyProtocolVersion::V2025_06_18,
            LegacyProtocolVersion::V2025_11_25,
        ] {
            assert_no_unearned_status("initialize result", &initialize_result(version));
        }
    }

    /// The deny path is Assay's own words reaching a client, and it was not covered by any claims
    /// assertion. Both call sites are checked by their real arguments rather than by a
    /// constructed sample, so the test reads what ships.
    #[test]
    fn fail_closed_tool_result_asserts_no_unearned_status() {
        for (code, message) in [
            ("E_INTERNAL", TOOL_EXECUTION_FAILED),
            ("E_TIMEOUT", TOOL_EXECUTION_TIMED_OUT),
        ] {
            let value = fail_closed_tool_result(code, message).expect("fail-closed result");
            assert_no_unearned_status("fail-closed tool result", &value);
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
            (LegacyProtocolVersion::V2025_06_18, "2025-06-18"),
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
    fn accepted_protocol_versions_match_the_legacy_enum() {
        let from_enum: Vec<&str> = LegacyProtocolVersion::ALL
            .iter()
            .map(|version| version.as_str())
            .collect();
        assert_eq!(from_enum, super::ACCEPTED_PROTOCOL_VERSIONS);
        #[allow(deprecated)]
        {
            assert!(!super::ACCEPTED_PROTOCOL_VERSIONS.contains(&super::MODERN_PROTOCOL_VERSION));
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
