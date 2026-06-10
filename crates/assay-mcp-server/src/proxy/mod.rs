//! P61b: MCP upstream proxy mode — manifest-observation v0. A safe stdio forwarding skeleton with
//! hard denials. Spec: docs/reference/mcp-upstream-proxy-mode.md.
//!
//! What this is: an explicit, opt-in proxy that spawns one stdio upstream MCP server, forwards a tiny
//! exhaustive method allowlist (the handshake and `tools/list`), relays the upstream's output back to
//! the client verbatim, and denies everything else — first of all `tools/call` — with a
//! proxy-originated error. The upstream never receives a denied method.
//!
//! What this is NOT (P61b is a forwarding skeleton, not manifest evidence and not tool execution
//! through the proxy): no manifest artifact is emitted, no pagination is tracked, no policy decision is
//! made, no tool-decision evidence is produced. Those are later slices (P61c+). A privileged
//! `tools/call` is never forwarded — not even observe-only — because a credential-bearing proxy that
//! relays privileged calls without a blocking decision is the confused-deputy trap.
//!
//! Credential boundary: the proxy injects no transport authentication of its own into forwarded
//! traffic. Allowlisted client requests are forwarded verbatim; the upstream's own credentials come
//! only from how the operator configured the spawned command's environment.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

/// The exhaustive v0 allowlist of client→upstream methods. Everything else is denied. This is
/// deliberately tiny: v0 enables live manifest observation, not general read-only MCP forwarding.
const ALLOWLIST: &[&str] = &[
    "initialize",
    "notifications/initialized",
    "ping",
    "tools/list",
    "notifications/tools/list_changed",
];

// Proxy-originated JSON-RPC error codes (private server-error range). The `data.origin` marker makes a
// proxy-originated error unambiguously distinguishable from an upstream error regardless of code.
const PROXY_UNSUPPORTED: i32 = -32040;
const PROXY_FAILED: i32 = -32041;
// PROXY_DENIED (-32042) is reserved for the future enforcing arc (P61e); it never occurs in v0.

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

/// Run the manifest-observation proxy against one stdio upstream. Returns when the client closes
/// stdin (EOF) or the upstream connection ends.
pub async fn run(upstream_command: String, upstream_args: Vec<String>) -> Result<()> {
    let spawned = Command::new(&upstream_command)
        .args(&upstream_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();

    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            // Fail-closed honesty: the upstream is unavailable, so never forward; answer every client
            // request with a proxy-originated failure rather than silence or a fabricated success.
            tracing::error!(event = "proxy_upstream_spawn_failed", error = %e);
            return degraded_loop("upstream_spawn_failed").await;
        }
    };

    let mut child_stdin = child.stdin.take().context("upstream stdin")?;
    let child_stdout = child.stdout.take().context("upstream stdout")?;

    // A single writer owns the client's stdout; both the upstream relay and proxy-originated errors
    // funnel through it so their lines never interleave.
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

    // Upstream → client: relay valid JSON verbatim. A non-JSON upstream line is never relayed as a
    // success; it surfaces as a proxy-originated failure (malformed upstream is not trusted).
    let up_tx = tx.clone();
    let upstream_reader = tokio::spawn(async move {
        let mut lines = BufReader::new(child_stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&line) {
                Ok(_) => {
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

    // Client → upstream: gate by the allowlist. Denied requests get a proxy_unsupported error and are
    // never sent upstream; denied notifications are dropped (no id to answer).
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
                // Forward the original bytes verbatim — the proxy injects nothing of its own.
                if forward_line(&mut child_stdin, &line).await.is_err() {
                    break;
                }
            }
            Some(method) => {
                // Denied: tools/call and every other non-allowlisted method. The upstream never sees it.
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
                // No method: a client response to an upstream-initiated request. It cannot invoke a
                // tool, so relay it verbatim to keep the session working.
                if forward_line(&mut child_stdin, &line).await.is_err() {
                    break;
                }
            }
        }
    }

    // Client EOF: close the upstream's stdin, tear down, and drain.
    drop(child_stdin);
    let _ = child.start_kill();
    drop(tx);
    let _ = upstream_reader.await;
    let _ = writer.await;
    let _ = child.wait().await;
    Ok(())
}

async fn forward_line<W: AsyncWriteExt + Unpin>(w: &mut W, line: &str) -> std::io::Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await
}

/// Upstream unavailable: never forward, never fabricate. Answer every client request with a
/// proxy-originated failure until the client closes stdin.
async fn degraded_loop(reason: &str) -> Result<()> {
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
    Ok(())
}
