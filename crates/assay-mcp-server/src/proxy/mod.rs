//! P61b + P61c: MCP upstream proxy mode — manifest-observation v0. A safe stdio forwarding skeleton
//! with hard denials (P61b) that also observes the upstream `tools/list` and emits manifest evidence
//! (P61c). Spec: docs/reference/mcp-upstream-proxy-mode.md.
//!
//! P61b — forwarding skeleton: spawn one stdio upstream, forward a tiny exhaustive method allowlist
//! (the handshake and `tools/list`), relay upstream output verbatim, deny everything else — first of
//! all `tools/call` — with a proxy-originated error the upstream never sees.
//!
//! P61c — manifest observation: read-only tap on `tools/list` responses, track the pagination chain
//! (complete / partial / unknown), select latest-complete-else-best-observed, and emit
//! `assay.mcp_manifest_observed.v0` (via the P60b producer) plus a small observation-health record.
//!
//! What this is NOT: tool execution through the proxy, policy enforcement, per-tool drift, or any
//! maliciousness claim. A privileged `tools/call` is never forwarded — not even observe-only — because
//! a credential-bearing proxy relaying privileged calls without a blocking decision is the
//! confused-deputy trap. The manifest artifact is exactly the P60b shape the consumer already gates on;
//! "how completely was it observed" lives in the separate observation-health artifact, never folded in.

use anyhow::{Context, Result};
use assay_mcp_server::manifest_observed::{self, Completeness};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};

/// The exhaustive v0 allowlist of client→upstream methods. Everything else is denied.
const ALLOWLIST: &[&str] = &[
    "initialize",
    "notifications/initialized",
    "ping",
    "tools/list",
    "notifications/tools/list_changed",
];

// Proxy-originated JSON-RPC error codes (private server-error range). `data.origin` marks them so they
// are unambiguously distinguishable from upstream errors regardless of code.
const PROXY_UNSUPPORTED: i32 = -32040;
const PROXY_FAILED: i32 = -32041;
// PROXY_DENIED (-32042) is reserved for the future enforcing arc (P61e); it never occurs in v0.

const HEALTH_SCHEMA: &str = "assay.proxy_observation_health.v0";

fn proxy_error_line(id: Value, code: i32, reason: &str, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": { "origin": "assay-proxy", "reason": reason }
        }
    })
    .to_string()
}

// ---------------------------------------------------------------------------------------------------
// Observer: a read-only model of the upstream tool-surface observation. One active tools/list chain
// per session in v0; a new cursorless tools/list starts a new operation, leaving any unfinished chain
// partial. complete requires a start (no-cursor request) through a terminal page (no nextCursor); a
// chain whose start was never observed is unknown; an unfinished started chain is partial.
// ---------------------------------------------------------------------------------------------------

#[derive(Default)]
struct Observer {
    server_id: Option<String>,
    /// tools/list request ids seen (so a response can be recognized as a list response).
    list_req_ids: HashMap<String, bool>,
    acc: Vec<Value>,
    chain_started: bool,
    outstanding_cursor: bool,
    op_active: bool,
    latest_complete: Option<Vec<Value>>,
    /// Most recent non-complete accumulation and its kind ("partial" | "unknown").
    best_incomplete: Option<(Vec<Value>, &'static str)>,
    observed_list_operations: u64,
    tools_list_changed_observed: bool,
    later_incomplete_chain_observed: bool,
    any_tools_list_observed: bool,
}

struct Emission {
    server_id: String,
    tools: Option<Vec<Value>>,
    completeness: Option<Completeness>,
    source: &'static str,
}

impl Observer {
    fn id_key(id: &Value) -> String {
        id.to_string()
    }

    /// An active operation ended without a terminal page (superseded, or shutdown): record it as the
    /// best non-complete observation, and note honestly if it came after a complete chain.
    fn close_active_as_incomplete(&mut self) {
        if !self.op_active {
            return;
        }
        let kind = if self.chain_started {
            "partial"
        } else {
            "unknown"
        };
        self.best_incomplete = Some((self.acc.clone(), kind));
        if self.latest_complete.is_some() {
            self.later_incomplete_chain_observed = true;
        }
        self.op_active = false;
    }

    /// Tap a forwarded client tools/list request. `had_cursor` is whether `params.cursor` was present.
    fn on_client_tools_list(&mut self, id: Option<&Value>, had_cursor: bool) {
        self.any_tools_list_observed = true;
        if !had_cursor {
            // A cursorless request starts a new operation; an unfinished prior chain becomes partial.
            if self.op_active && self.outstanding_cursor {
                self.close_active_as_incomplete();
            }
            self.acc.clear();
            self.chain_started = true;
            self.outstanding_cursor = false;
            self.op_active = true;
            self.observed_list_operations += 1;
        } else if !self.op_active {
            // A cursored request with no active operation: joined mid-stream, start unprovable.
            self.acc.clear();
            self.chain_started = false;
            self.outstanding_cursor = false;
            self.op_active = true;
            self.observed_list_operations += 1;
        }
        if let Some(id) = id {
            self.list_req_ids.insert(Self::id_key(id), had_cursor);
        }
    }

    /// Tap an upstream response. Returns true iff a chain just COMPLETED (so the caller may write the
    /// latest-complete manifest mid-run).
    fn on_upstream_response(&mut self, v: &Value) -> bool {
        if self.server_id.is_none() {
            if let Some(name) = v
                .pointer("/result/serverInfo/name")
                .and_then(|n| n.as_str())
            {
                self.server_id = Some(name.to_string());
            }
        }
        let id = match v.get("id") {
            Some(i) if !i.is_null() => Self::id_key(i),
            _ => return false,
        };
        if !self.list_req_ids.contains_key(&id) {
            return false;
        }
        if let Some(tools) = v.pointer("/result/tools").and_then(|t| t.as_array()) {
            self.acc.extend(tools.iter().cloned());
        }
        let has_next = v
            .pointer("/result/nextCursor")
            .map(|c| !c.is_null())
            .unwrap_or(false);
        if has_next {
            self.outstanding_cursor = true;
            return false;
        }
        // Terminal page.
        self.outstanding_cursor = false;
        self.op_active = false;
        if self.chain_started {
            self.latest_complete = Some(self.acc.clone());
            true
        } else {
            self.best_incomplete = Some((self.acc.clone(), "unknown"));
            if self.latest_complete.is_some() {
                self.later_incomplete_chain_observed = true;
            }
            false
        }
    }

    fn on_list_changed(&mut self) {
        self.tools_list_changed_observed = true;
    }

    /// Compute the final emission: latest complete wins; otherwise the best observed; otherwise
    /// not_observed. Closes any still-active chain as incomplete first.
    fn finalize(&mut self) -> Emission {
        if self.op_active {
            self.close_active_as_incomplete();
        }
        let server_id = self
            .server_id
            .clone()
            .unwrap_or_else(|| "upstream".to_string());
        if !self.any_tools_list_observed {
            return Emission {
                server_id,
                tools: None,
                completeness: None,
                source: "not_observed",
            };
        }
        if let Some(tools) = &self.latest_complete {
            return Emission {
                server_id,
                tools: Some(tools.clone()),
                completeness: Some(Completeness::Complete),
                source: "latest_complete",
            };
        }
        if let Some((tools, kind)) = &self.best_incomplete {
            let (c, src) = if *kind == "partial" {
                (Completeness::Partial, "best_partial")
            } else {
                (Completeness::Unknown, "best_unknown")
            };
            return Emission {
                server_id,
                tools: Some(tools.clone()),
                completeness: Some(c),
                source: src,
            };
        }
        // tools/list seen but nothing accumulated/finalized: inconclusive, never clean.
        Emission {
            server_id,
            tools: Some(vec![]),
            completeness: Some(Completeness::Unknown),
            source: "best_unknown",
        }
    }
}

fn manifest_from(em: &Emission) -> Value {
    match (&em.tools, em.completeness) {
        (Some(tools), Some(c)) => manifest_observed::build_observed(&em.server_id, tools, c),
        _ => manifest_observed::not_observed(&em.server_id),
    }
}

fn health_from(manifest: &Value, em: &Emission, obs: &Observer) -> Value {
    let status = manifest["status"].as_str().unwrap_or("not_observed");
    // emitted_state_source reflects WHY this manifest was written; ambiguous overrides the chain source.
    let source = if status == "ambiguous" {
        "ambiguous"
    } else {
        em.source
    };
    json!({
        "schema": HEALTH_SCHEMA,
        "manifest_observation": {
            "tools_list_observed": manifest["observed"]["tools_list_observed"],
            "tools_list_complete": manifest["observed"]["tools_list_complete"],
            "status": status,
            "emitted_state_source": source,
            "observed_list_operations": obs.observed_list_operations,
            "tools_list_changed_observed": obs.tools_list_changed_observed,
            "later_incomplete_chain_observed": obs.later_incomplete_chain_observed
        },
        "non_claims": [
            "does not imply tool-call forwarding",
            "does not prove the upstream full tool set if observation was partial or unknown"
        ]
    })
}

/// Atomic-ish write: serialize to a sibling temp file in the same directory, then rename over the
/// destination. On Windows `rename` will not replace an existing file, so fall back to remove+rename
/// (not atomic there, documented). A missing parent directory makes this fail, surfacing as a
/// non-zero exit for a requested artifact.
fn write_json_atomic(path: &Path, value: &Value) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("artifact");
    let tmp_name = format!("{file_name}.tmp.{}", std::process::id());
    let tmp = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    };
    std::fs::write(&tmp, &bytes)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) if path.exists() => {
            std::fs::remove_file(path)?;
            std::fs::rename(&tmp, path)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Run the manifest-observation proxy against one stdio upstream.
pub async fn run(
    upstream_command: String,
    upstream_args: Vec<String>,
    manifest_out: Option<PathBuf>,
    health_out: Option<PathBuf>,
) -> Result<()> {
    let spawned = Command::new(&upstream_command)
        .args(&upstream_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();

    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            // Fail-closed: the upstream is unavailable, so never forward. Answer requests with a
            // proxy failure and (if requested) emit an honest not_observed artifact.
            tracing::error!(event = "proxy_upstream_spawn_failed", error = %e);
            degraded_loop("upstream_spawn_failed").await;
            return emit_not_observed(manifest_out, health_out);
        }
    };

    let mut child_stdin = child.stdin.take().context("upstream stdin")?;
    let child_stdout = child.stdout.take().context("upstream stdout")?;

    let observer = Arc::new(Mutex::new(Observer::default()));

    // Single writer owns client stdout; the upstream relay and proxy-originated errors funnel through
    // it so their lines never interleave.
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if out.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if out.write_all(b"\n").await.is_err() {
                break;
            }
            let _ = out.flush().await;
        }
    });

    // Upstream → client: tap (read-only) then relay verbatim. A non-JSON upstream line is never
    // relayed as success.
    let up_tx = tx.clone();
    let up_obs = observer.clone();
    let up_manifest_out = manifest_out.clone();
    let upstream_reader = tokio::spawn(async move {
        let mut lines = BufReader::new(child_stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&line) {
                Ok(v) => {
                    let completed = {
                        let mut o = up_obs.lock().await;
                        if v.get("method").and_then(|m| m.as_str())
                            == Some("notifications/tools/list_changed")
                        {
                            o.on_list_changed();
                            false
                        } else {
                            o.on_upstream_response(&v)
                        }
                    };
                    // Best-effort mid-run write so a killed proxy still leaves the latest complete.
                    if completed {
                        if let Some(path) = &up_manifest_out {
                            let (server, tools) = {
                                let o = up_obs.lock().await;
                                (
                                    o.server_id
                                        .clone()
                                        .unwrap_or_else(|| "upstream".to_string()),
                                    o.latest_complete.clone().unwrap_or_default(),
                                )
                            };
                            let m = manifest_observed::build_observed(
                                &server,
                                &tools,
                                Completeness::Complete,
                            );
                            let _ = write_json_atomic(path, &m);
                        }
                    }
                    let _ = up_tx.send(line);
                }
                Err(_) => {
                    let _ = up_tx.send(proxy_error_line(
                        Value::Null,
                        PROXY_FAILED,
                        "malformed_upstream_response",
                        "upstream emitted a non-JSON line; not relayed",
                    ));
                }
            }
        }
    });

    // Client → upstream: gate by the allowlist; tap tools/list requests for pagination tracking.
    let mut client = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = client.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                let _ = tx.send(proxy_error_line(
                    Value::Null,
                    PROXY_FAILED,
                    "malformed_client_request",
                    "client sent a non-JSON line",
                ));
                continue;
            }
        };

        match v.get("method").and_then(|m| m.as_str()) {
            Some(method) if ALLOWLIST.contains(&method) => {
                if method == "tools/list" {
                    let had_cursor = v
                        .pointer("/params/cursor")
                        .map(|c| !c.is_null())
                        .unwrap_or(false);
                    observer
                        .lock()
                        .await
                        .on_client_tools_list(v.get("id"), had_cursor);
                }
                if forward_line(&mut child_stdin, &line).await.is_err() {
                    break;
                }
            }
            Some(method) => {
                // Denied: tools/call and every other non-allowlisted method. Never sent upstream.
                match v.get("id") {
                    Some(id) if !id.is_null() => {
                        let _ = tx.send(proxy_error_line(
                            id.clone(),
                            PROXY_UNSUPPORTED,
                            "method_not_allowlisted",
                            &format!(
                                "method {method:?} is not forwarded in manifest-observation proxy mode"
                            ),
                        ));
                    }
                    _ => { /* denied notification: nothing to answer, drop it */ }
                }
            }
            None => {
                // A client response to an upstream-initiated request: cannot invoke a tool, relay it.
                if forward_line(&mut child_stdin, &line).await.is_err() {
                    break;
                }
            }
        }
    }

    // Client EOF: tear down, drain, then emit the authoritative artifacts.
    drop(child_stdin);
    let _ = child.start_kill();
    drop(tx);
    let _ = upstream_reader.await;
    let _ = writer.await;
    let _ = child.wait().await;

    if manifest_out.is_none() && health_out.is_none() {
        return Ok(());
    }
    let mut obs = observer.lock().await;
    let em = obs.finalize();
    let manifest = manifest_from(&em);
    if let Some(path) = &manifest_out {
        write_json_atomic(path, &manifest)
            .with_context(|| format!("writing manifest artifact to {}", path.display()))?;
    }
    if let Some(path) = &health_out {
        let health = health_from(&manifest, &em, &obs);
        write_json_atomic(path, &health).with_context(|| {
            format!("writing observation-health artifact to {}", path.display())
        })?;
    }
    Ok(())
}

/// Emit a not_observed manifest/health pair (upstream never produced a tools/list).
fn emit_not_observed(manifest_out: Option<PathBuf>, health_out: Option<PathBuf>) -> Result<()> {
    if manifest_out.is_none() && health_out.is_none() {
        return Ok(());
    }
    let obs = Observer::default();
    let em = Emission {
        server_id: "upstream".to_string(),
        tools: None,
        completeness: None,
        source: "not_observed",
    };
    let manifest = manifest_from(&em);
    if let Some(path) = &manifest_out {
        write_json_atomic(&path.clone(), &manifest)
            .with_context(|| format!("writing manifest artifact to {}", path.display()))?;
    }
    if let Some(path) = &health_out {
        let health = health_from(&manifest, &em, &obs);
        write_json_atomic(path, &health).with_context(|| {
            format!("writing observation-health artifact to {}", path.display())
        })?;
    }
    Ok(())
}

async fn forward_line<W: AsyncWriteExt + Unpin>(w: &mut W, line: &str) -> std::io::Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await
}

/// Upstream unavailable: never forward, never fabricate. Answer every client request with a
/// proxy-originated failure until the client closes stdin.
async fn degraded_loop(reason: &str) {
    let mut out = tokio::io::stdout();
    let mut client = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = client.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(id) = v.get("id") {
            if !id.is_null() {
                let msg = proxy_error_line(
                    id.clone(),
                    PROXY_FAILED,
                    reason,
                    "upstream MCP server is unavailable",
                );
                let _ = out.write_all(msg.as_bytes()).await;
                let _ = out.write_all(b"\n").await;
                let _ = out.flush().await;
            }
        }
    }
}
