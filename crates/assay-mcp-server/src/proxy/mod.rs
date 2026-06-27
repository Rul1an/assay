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

pub mod annotation_conformance;
pub mod enforce;
pub mod establish;
pub mod relay_routing;

mod call;
mod establish_runner;
mod io;
mod observer;

use anyhow::{Context, Result};
use assay_mcp_server::manifest_observed::{self, Completeness};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
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

/// Proxy run mode. Observe (P61a-d, shipped) forwards the allowlist and answers `tools/call` with
/// `proxy_unsupported`. Enforce runs the P61e-c PDP on every `tools/call` — classification,
/// caller-allowance, credential-scope, and drift gates — denying with the precedence-pinned reason of
/// the first gate that fails and FORWARDING only a call that clears every gate (the single allow path).
/// Everything other than `tools/call` is identical to Observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Observe,
    Enforce,
}

/// Run the proxy against one stdio upstream. `mode` selects observe (manifest-observation, P61a-d) or
/// enforce (the P61e-c PDP on `tools/call`); the only behavioral difference is how `tools/call` is
/// answered. `policy` and `baseline` are both required and present in enforce mode (loaded + validated
/// at startup) and `None` in observe mode: the c3 drift gate compares the current observed per-tool
/// digest against the approved `baseline`. `decision_out` (enforce only) is the optional NDJSON path
/// for the per-call `assay.enforcement_decision.v0` evidence record (P61e-d). Manifest emission flags
/// apply to observe only.
pub async fn run(
    upstream_command: String,
    upstream_args: Vec<String>,
    mode: Mode,
    enforce_inputs: enforce::EnforceInputs,
    manifest_out: Option<PathBuf>,
    health_out: Option<PathBuf>,
) -> Result<()> {
    let enforce::EnforceInputs {
        policy,
        baseline,
        decision_out,
        establish_out,
        tool_conformance_out,
        establish_budget,
    } = enforce_inputs;
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
            io::degraded_loop("upstream_spawn_failed").await;
            return observer::emit_not_observed(manifest_out, health_out);
        }
    };

    let mut child_stdin = child.stdin.take().context("upstream stdin")?;
    let child_stdout = child.stdout.take().context("upstream stdout")?;
    let observer = Arc::new(Mutex::new(observer::Observer::default()));

    // Single writer owns client stdout; the upstream relay and proxy-originated errors funnel through
    // it so their lines never interleave.
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if tokio::io::AsyncWriteExt::write_all(&mut out, line.as_bytes())
                .await
                .is_err()
            {
                break;
            }
            if tokio::io::AsyncWriteExt::write_all(&mut out, b"\n")
                .await
                .is_err()
            {
                break;
            }
            let _ = tokio::io::AsyncWriteExt::flush(&mut out).await;
        }
    });

    // Establish registry (P61e Increment 2): shared between the establish caller and the single
    // upstream reader. The reserved id namespace is rejected on client requests below.
    let establish_registry = relay_routing::EstablishRegistry::default();
    let establish_id_counter = std::sync::atomic::AtomicU64::new(1);

    // Upstream → client: tap (read-only) then relay verbatim. A non-JSON upstream line is never
    // relayed as success.
    let up_tx = tx.clone();
    let up_obs = observer.clone();
    let up_manifest_out = manifest_out.clone();
    let up_registry = establish_registry.clone();
    let upstream_reader = tokio::spawn(async move {
        let mut lines = BufReader::new(child_stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&line) {
                Ok(v) => {
                    let route = relay_routing::route_upstream(&v, |id| up_registry.is_pending(id));
                    if matches!(route, relay_routing::UpstreamRoute::SuppressReserved) {
                        continue;
                    }
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
                            let snapshot = up_obs.lock().await.latest_complete_snapshot();
                            if let Some((server, tools)) = snapshot {
                                let m = manifest_observed::build_observed(
                                    &server,
                                    &tools,
                                    Completeness::Complete,
                                );
                                let _ = io::write_json_atomic(path, &m);
                            }
                        }
                    }
                    match route {
                        relay_routing::UpstreamRoute::DivertToEstablish(id) => {
                            up_registry.resolve(&id, v);
                        }
                        relay_routing::UpstreamRoute::RelayToClient => {
                            let _ = up_tx.send(line);
                        }
                        relay_routing::UpstreamRoute::SuppressReserved => {
                            unreachable!("SuppressReserved is handled before the observer tap")
                        }
                    }
                }
                Err(_) => {
                    let _ = up_tx.send(io::proxy_error_line(
                        Value::Null,
                        io::PROXY_FAILED,
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
                let _ = tx.send(io::proxy_error_line(
                    Value::Null,
                    io::PROXY_FAILED,
                    "malformed_client_request",
                    "client sent a non-JSON line",
                ));
                continue;
            }
        };

        if relay_routing::is_reserved_client_request(&v) {
            let _ = tx.send(io::proxy_error_line(
                v.get("id").cloned().unwrap_or(Value::Null),
                io::PROXY_FAILED,
                "reserved_id_namespace",
                "client request id collides with the proxy's reserved establish id namespace; rejected",
            ));
            continue;
        }

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
                if io::forward_line(&mut child_stdin, &line).await.is_err() {
                    break;
                }
            }
            Some(method) if mode == Mode::Enforce && method == "tools/call" => {
                let policy = policy
                    .as_ref()
                    .expect("enforce mode without a loaded policy is a startup bug");
                let baseline = baseline
                    .as_ref()
                    .expect("enforce mode without a loaded baseline is a startup bug");
                let runtime = call::EnforcementRuntime {
                    policy,
                    baseline,
                    decision_out: &decision_out,
                    establish_out: &establish_out,
                    tool_conformance_out: &tool_conformance_out,
                    establish_budget,
                    registry: &establish_registry,
                    establish_id_counter: &establish_id_counter,
                    observer: &observer,
                    tx: &tx,
                };
                if call::handle_tools_call(&mut child_stdin, runtime, &v, &line)
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Some(method) => {
                // Every other non-allowlisted method — and observe mode's tools/call — is never sent
                // upstream and stays proxy_unsupported (distinct from the enforce-mode proxy_denied).
                match v.get("id") {
                    Some(id) if !id.is_null() => {
                        let _ = tx.send(io::proxy_error_line(
                            id.clone(),
                            io::PROXY_UNSUPPORTED,
                            "method_not_allowlisted",
                            &format!("method {method:?} is not forwarded in this proxy mode"),
                        ));
                    }
                    _ => { /* denied notification: nothing to answer, drop it */ }
                }
            }
            None => {
                // A client response to an upstream-initiated request: cannot invoke a tool, relay it.
                if io::forward_line(&mut child_stdin, &line).await.is_err() {
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
    let manifest = observer::manifest_from(&em);
    if let Some(path) = &manifest_out {
        io::write_json_atomic(path, &manifest)
            .with_context(|| format!("writing manifest artifact to {}", path.display()))?;
    }
    if let Some(path) = &health_out {
        let health = observer::health_from(&manifest, &em, &obs);
        io::write_json_atomic(path, &health).with_context(|| {
            format!("writing observation-health artifact to {}", path.display())
        })?;
    }
    Ok(())
}
