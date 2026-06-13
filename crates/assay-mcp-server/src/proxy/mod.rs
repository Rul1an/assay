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

pub mod enforce;
// Pre-call manifest-establish decision layer (P61e, Increment 1). Pure logic + the
// `assay.manifest_establish.v0` sibling carrier; not yet wired into the relay (Increment 2).
pub mod establish;
// Internal request routing for the establish flow (P61e, Increment 2 slice 1): reserved-id namespace,
// pending registry, single-reader suppression routing, and client-id collision rejection. The
// suppression decision and collision guard are wired below (behavior-preserving); proxy-originated
// re-list lands in slice 2.
pub mod relay_routing;

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
// PROXY_DENIED: a P61e enforcing-mode policy denial. The `reason` is the precedence-pinned gate that
// fired (unclassified_tool_call / classification_incomplete / no_declared_allowance /
// credential_scope_insufficient / credential_scope_unknown / manifest_baseline_missing /
// manifest_current_observation_incomplete / manifest_observation_ambiguous /
// manifest_drifted_since_approval).
const PROXY_DENIED: i32 = -32042;

const HEALTH_SCHEMA: &str = "assay.proxy_observation_health.v0";

/// The one total deadline for a pre-call manifest-establish run (Increment 2b). The operator-facing
/// CLI surface lands in slice 2c; for now this is a private default, with an internal env override
/// (`ASSAY_ESTABLISH_BUDGET_MS`) used only to keep the timeout acceptance test fast and deterministic —
/// it is not an operator interface.
const DEFAULT_ESTABLISH_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

fn establish_budget() -> std::time::Duration {
    std::env::var("ASSAY_ESTABLISH_BUDGET_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or(DEFAULT_ESTABLISH_BUDGET)
}

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
    /// True once a `tools/list_changed` is observed AFTER `latest_complete` was set, until a fresh
    /// complete chain replaces it. The enforcing drift gate (P61e-c3) must not authorize against a
    /// stale manifest: a post-approval `tools/list_changed` is exactly the rug-pull signal, so the
    /// current complete observation is invalidated until a fresh complete `tools/list` is observed.
    complete_superseded_by_change: bool,
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
        // A message carrying a `method` is a request or notification, never a response to a tools/list
        // request — never fold it into the accumulation, even if its id matches a list request id.
        if v.get("method").is_some() {
            return false;
        }
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
            // A fresh complete observation supersedes any prior list_changed invalidation.
            self.complete_superseded_by_change = false;
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
        // Invalidate the current complete observation for the enforcing drift gate: the surface may
        // have mutated, so a stale digest must not authorize a privileged call until re-observed.
        self.complete_superseded_by_change = true;
    }

    /// The current observed per-tool `tool_digest` for `tool_name`, from the latest COMPLETE observed
    /// manifest (P61c). Read-only. Used by the enforcing drift gate (P61e-c3); every non-`Present`
    /// outcome is fail-closed (the gate cannot establish a current digest), never an allow:
    /// - no complete manifest observed, OR one invalidated by a later `tools/list_changed` -> NoCompleteManifest;
    /// - a duplicate-name (ambiguous) observed manifest -> Ambiguous (the observation is inconclusive,
    ///   per the manifest-drift contract — never pick one of the colliding digests);
    /// - the tool absent from an otherwise-clean complete manifest -> CompleteButToolAbsent.
    fn observed_tool_digest(&self, tool_name: &str) -> enforce::ObservedToolDigest {
        // A post-approval list_changed invalidates the current complete view until it is re-observed.
        let tools = match &self.latest_complete {
            Some(t) if !self.complete_superseded_by_change => t,
            _ => return enforce::ObservedToolDigest::NoCompleteManifest,
        };
        let server = self
            .server_id
            .clone()
            .unwrap_or_else(|| "upstream".to_string());
        let manifest = manifest_observed::build_observed(&server, tools, Completeness::Complete);
        // A duplicate-name manifest is `status: ambiguous` (manifest_digest withheld): the observation
        // is inconclusive, so the drift gate must deny rather than pick one of the colliding per-tool
        // digests. See docs/reference/mcp-manifest-drift.md.
        if manifest["status"].as_str() != Some("observed") {
            return enforce::ObservedToolDigest::Ambiguous;
        }
        if let Some(arr) = manifest["observed"]["tool_digests"].as_array() {
            for e in arr {
                if e.get("name").and_then(|n| n.as_str()) == Some(tool_name) {
                    if let Some(d) = e.get("tool_digest").and_then(|d| d.as_str()) {
                        return enforce::ObservedToolDigest::Present(d.to_string());
                    }
                }
            }
        }
        enforce::ObservedToolDigest::CompleteButToolAbsent
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

/// Append one `assay.enforcement_decision.v0` record as an NDJSON line (P61e-d). Per-call streaming,
/// so a killed proxy still keeps the decisions recorded so far; the record is one compact JSON object
/// per line. A failure here is surfaced to the caller, which fails an allowed call closed rather than
/// forwarding it unrecorded.
fn append_decision_record(path: &Path, record: &Value) -> std::io::Result<()> {
    use std::io::Write as _;
    let line = serde_json::to_string(record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{line}")
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

    // Establish registry (P61e Increment 2): shared between the establish caller (slice 2) and the
    // single upstream reader. In slice 1 nothing registers a reserved id, so routing is a no-op and the
    // relay is behavior-identical to before.
    let establish_registry = relay_routing::EstablishRegistry::default();
    // Monotonic reserved-id source for proxy-originated establish requests (one per page), and the one
    // total establish deadline (Increment 2b).
    let establish_id_counter = std::sync::atomic::AtomicU64::new(1);
    let establish_budget = establish_budget();

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
                    // Single-reader routing, decided BEFORE the observer tap. A reserved id is the
                    // proxy's namespace (client requests in it are rejected), so any reserved-id upstream
                    // response is proxy-originated: a pending one is diverted to the establish caller and
                    // suppressed from the client; a NON-pending one (a late/duplicate establish reply, or
                    // an unprompted reserved id from the upstream) is dropped with NO relay and NO tap,
                    // so it can neither leak to the client nor corrupt the observer accumulation.
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
                    match route {
                        relay_routing::UpstreamRoute::DivertToEstablish(id) => {
                            // A pending establish page: fold into the observer (above) then deliver to
                            // the waiting caller; suppressed from the client.
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

        // Reserved-id collision guard (P61e Increment 2): the proxy reserves the `assay-establish-` id
        // namespace for its own originated requests. A client REQUEST whose id lands in that namespace
        // is rejected, never forwarded — otherwise a colliding client id could cause a real client
        // response to be routed into (and swallowed by) the establish registry. Only requests are
        // gated: a client RESPONSE to an upstream-initiated request (no `method`) still relays verbatim
        // via the response path below, so the relay stays behavior-preserving for it.
        if relay_routing::is_reserved_client_request(&v) {
            let _ = tx.send(proxy_error_line(
                v.get("id").cloned().unwrap_or(Value::Null),
                PROXY_FAILED,
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
                if forward_line(&mut child_stdin, &line).await.is_err() {
                    break;
                }
            }
            Some(method) if mode == Mode::Enforce && method == "tools/call" => {
                // The enforcing PDP runs on every tools/call. policy + baseline are always Some in
                // enforce mode (loaded + validated at startup). This is the ONLY path that forwards a
                // privileged call — and only after a clear allow (the confused-deputy discipline).
                let policy = policy
                    .as_ref()
                    .expect("enforce mode without a loaded policy is a startup bug");
                let baseline = baseline
                    .as_ref()
                    .expect("enforce mode without a loaded baseline is a startup bug");
                let tool_name = v
                    .pointer("/params/name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let empty = json!({});
                let call_args = v.pointer("/params/arguments").unwrap_or(&empty);
                // The current observed per-tool digest, from this session's observed tools/list
                // (read-only). The guard is dropped before any await below.
                let observed = observer.lock().await.observed_tool_digest(tool_name);
                let mut decision =
                    enforce::decide(policy, baseline, &observed, tool_name, call_args);
                // Pre-decision recovery for the manifest-availability gap ONLY (Increment 2b): when the
                // sole reason for the deny is that no current complete manifest was observed for this
                // tool, attempt ONE bounded proxy-originated re-list, then re-decide over the fresh
                // observation. The trigger is explicit on BOTH the reason and the establish action so a
                // future ObservedToolDigest/reason combination can never accidentally establish outside
                // the gap. This never relaxes a gate: Ambiguous, baseline-missing, and real drift skip
                // establish and stay denied, and a failed/timed-out establish leaves the deny standing.
                // enforcement_decision.v0 records only the EFFECTIVE (re-decided) decision below.
                let est_action = establish::establish_action(&observed);
                if !decision.allow
                    && decision.reason == "manifest_current_observation_incomplete"
                    && matches!(
                        est_action,
                        establish::EstablishAction::ReList | establish::EstablishAction::ReListOnce
                    )
                {
                    let run_outcome = run_establish(
                        &mut child_stdin,
                        &establish_registry,
                        &observer,
                        &establish_id_counter,
                        establish_budget,
                    )
                    .await;
                    let observed2 = observer.lock().await.observed_tool_digest(tool_name);
                    decision = enforce::decide(policy, baseline, &observed2, tool_name, call_args);
                    tracing::info!(
                        event = "establish_attempted",
                        tool = %tool_name,
                        run_outcome = ?run_outcome,
                        effective_decision = if decision.allow { "allow" } else { "deny" },
                        reason = decision.reason,
                        note = "pre-call manifest-establish; verdict lives in enforcement_decision.v0"
                    );
                }
                // Diagnostic decision log — operability/correlation only, NOT the canonical
                // assay.enforcement_decision.v0 evidence artifact (that is P61e-d).
                tracing::info!(
                    event = "enforce_decision",
                    caller = %policy.caller.id,
                    tool = %tool_name,
                    action_class = decision.action_class.as_deref().unwrap_or("none"),
                    target_digest = decision.target_digest.as_deref().unwrap_or("none"),
                    decision = if decision.allow { "allow" } else { "deny" },
                    reason = decision.reason,
                    note = "diagnostic decision log; not canonical evidence"
                );
                // P61e-d: write the canonical per-call evidence record (NDJSON) before acting. The
                // safety rule (spec §12): a record-write failure on a requested path must never become
                // a silent unrecorded forward, so an allowed call that cannot be recorded fails closed.
                let record_ok = match &decision_out {
                    Some(path) => {
                        let record =
                            enforce::decision_record(policy, &decision, tool_name, call_args);
                        match append_decision_record(path, &record) {
                            Ok(()) => true,
                            Err(e) => {
                                tracing::error!(
                                    event = "enforcement_record_write_failed",
                                    error = %e,
                                    path = %path.display()
                                );
                                false
                            }
                        }
                    }
                    None => true,
                };
                if decision.allow && !record_ok {
                    // Fail-closed: never forward an allowed call we could not record.
                    if let Some(id) = v.get("id") {
                        if !id.is_null() {
                            let _ = tx.send(proxy_error_line(
                                id.clone(),
                                PROXY_FAILED,
                                "enforcement_record_write_failed",
                                "enforcement decision could not be recorded; call not forwarded",
                            ));
                        }
                    }
                } else if decision.allow {
                    // Forward the privileged call; the upstream's response relays verbatim via the
                    // upstream reader (a tools/call response is not a list response, so it is untouched).
                    if forward_line(&mut child_stdin, &line).await.is_err() {
                        break;
                    }
                } else {
                    // A deny stands regardless of a record-write failure (the call is fail-closed
                    // either way); a missing deny-record is a completeness gap, already logged above.
                    match v.get("id") {
                        Some(id) if !id.is_null() => {
                            let _ = tx.send(proxy_error_line(
                                id.clone(),
                                PROXY_DENIED,
                                decision.reason,
                                &format!(
                                    "tools/call denied by enforcing proxy: {}",
                                    decision.reason
                                ),
                            ));
                        }
                        _ => { /* denied notification: nothing to answer, drop it */ }
                    }
                }
            }
            Some(method) => {
                // Every other non-allowlisted method — and observe mode's tools/call — is never sent
                // upstream and stays proxy_unsupported (distinct from the enforce-mode proxy_denied).
                match v.get("id") {
                    Some(id) if !id.is_null() => {
                        let _ = tx.send(proxy_error_line(
                            id.clone(),
                            PROXY_UNSUPPORTED,
                            "method_not_allowlisted",
                            &format!("method {method:?} is not forwarded in this proxy mode"),
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

/// The detailed result of a proxy-originated establish run. The carrier (`assay.manifest_establish.v0`)
/// collapses every non-complete variant to `EstablishFailed` per #1659; the runner returns the specific
/// reason so the wiring (slice 2b) can log/diagnose timeout vs partial vs transport vs upstream error
/// without expanding the coarse carrier contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // wired into the tools/call path by slice 2b.
enum EstablishRunOutcome {
    /// A terminal (no-`nextCursor`) page completed the chain.
    Complete,
    /// The total deadline elapsed before any page was received.
    TimedOut,
    /// At least one page arrived but the chain did not reach a terminal page within the deadline.
    Partial,
    /// Failed to write the synthetic request, or the response channel dropped.
    TransportError,
    /// The upstream answered a synthetic request with a JSON-RPC error.
    ErrorResponse,
    /// A reserved id was already pending (unreachable with the monotonic counter; fail-closed anyway).
    RegisterRefused,
}

impl EstablishRunOutcome {
    /// Map the detailed run result to the coarse carrier outcome. Only `Complete` is an establish
    /// success; everything else fails closed.
    #[allow(dead_code)] // wired into the tools/call path by slice 2b.
    fn to_carrier(self) -> establish::EstablishOutcome {
        match self {
            EstablishRunOutcome::Complete => establish::EstablishOutcome::EstablishedComplete,
            _ => establish::EstablishOutcome::EstablishFailed,
        }
    }
}

/// Drive a proxy-originated, possibly paginated `tools/list` against the upstream child to establish a
/// current complete observation, under ONE total deadline across ALL pages (P61e Increment 2 — never a
/// per-page timeout, so a many-page upstream cannot inflate the budget to `N * timeout`).
///
/// Each page is registered with BOTH the observer (`on_client_tools_list`, so the single upstream reader
/// recognizes the response as a list page and folds it into `latest_complete`) and the registry (so the
/// reader diverts that response here and suppresses it from the client stream). The cursor chain is
/// driven to its terminal page; a `nextCursor` response is NEVER enough to claim completion. Fail-closed:
/// timeout / partial / transport error / upstream error / refused registration all return a non-complete
/// outcome, and the caller re-runs `decide()` over whatever the observer now holds. The `PendingEstablish`
/// guard reclaims each page's registry entry on every outcome, so a non-responsive upstream cannot grow
/// the map. Wired into the tools/call path by slice 2b.
#[allow(dead_code)] // wired into the tools/call path by slice 2b.
async fn run_establish<W: AsyncWriteExt + Unpin>(
    child_stdin: &mut W,
    registry: &relay_routing::EstablishRegistry,
    observer: &Arc<Mutex<Observer>>,
    id_counter: &std::sync::atomic::AtomicU64,
    budget: std::time::Duration,
) -> EstablishRunOutcome {
    use std::sync::atomic::Ordering;
    let deadline = tokio::time::Instant::now() + budget;
    let mut cursor: Option<String> = None;
    let mut pages_received: u32 = 0;
    // A timeout before any page is TimedOut; after at least one page it is Partial.
    let timed_out = |pages: u32| {
        if pages > 0 {
            EstablishRunOutcome::Partial
        } else {
            EstablishRunOutcome::TimedOut
        }
    };
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return timed_out(pages_received);
        }
        let remaining = deadline - now;
        let n = id_counter.fetch_add(1, Ordering::Relaxed);
        let id = relay_routing::mint_reserved_id(&n.to_string());
        // Register with the REGISTRY first. The invariant is: no successful registration, no observer
        // mutation — so a refused (duplicate) registration fails closed without having started or
        // cleared a synthetic list operation that was never written upstream.
        let (guard, rx) = match registry.register(id.clone()) {
            Some(pair) => pair,
            None => return EstablishRunOutcome::RegisterRefused,
        };
        // Only now register the synthetic request with the observer (cursor/no-cursor), still BEFORE the
        // write, so the reader recognizes the response as a list page and folds it in.
        observer
            .lock()
            .await
            .on_client_tools_list(Some(&Value::String(id.clone())), cursor.is_some());
        let mut req = serde_json::Map::new();
        req.insert("jsonrpc".to_string(), Value::String("2.0".to_string()));
        req.insert("id".to_string(), Value::String(id.clone()));
        req.insert(
            "method".to_string(),
            Value::String("tools/list".to_string()),
        );
        if let Some(c) = &cursor {
            req.insert("params".to_string(), json!({ "cursor": c }));
        }
        let line = Value::Object(req).to_string();
        // Bound the synthetic write by the SAME total deadline: if the upstream stops reading stdin or
        // the pipe backpressures, the write must not hang past the promised budget before we even await.
        match tokio::time::timeout(remaining, forward_line(child_stdin, &line)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return EstablishRunOutcome::TransportError, // guard drops -> entry reclaimed
            Err(_) => return timed_out(pages_received),               // write itself timed out
        }
        // Recompute the budget AFTER the write so the response await uses only the time that is left.
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return timed_out(pages_received);
        }
        let resp = relay_routing::await_establish(rx, deadline - now).await;
        drop(guard); // reclaim the pending entry on every outcome (success, timeout, or error)
        let resp = match resp {
            Some(v) => v,
            None => return timed_out(pages_received),
        };
        if resp.get("error").is_some() {
            return EstablishRunOutcome::ErrorResponse;
        }
        pages_received += 1;
        // Completion MUST match the Observer's rule (on_upstream_response): a chain is complete only when
        // nextCursor is absent or JSON null. Any non-null nextCursor means more pages — paginate when it
        // is a usable non-empty string, otherwise the upstream claims more pages but gives no usable
        // cursor (empty/non-string): malformed, fail closed and never falsely report Complete.
        let next = resp.pointer("/result/nextCursor");
        if next.map(|c| !c.is_null()).unwrap_or(false) {
            match next.and_then(|c| c.as_str()) {
                Some(s) if !s.is_empty() => cursor = Some(s.to_string()),
                _ => return EstablishRunOutcome::ErrorResponse,
            }
        } else {
            return EstablishRunOutcome::Complete;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str) -> Value {
        json!({"name": name, "description": "d", "inputSchema": {"type": "object"}})
    }

    /// Drive a single complete cursorless tools/list (request + terminal response) into the observer.
    fn observe_complete(obs: &mut Observer, id: i64, tools: Vec<Value>) {
        let idv = json!(id);
        obs.on_client_tools_list(Some(&idv), false);
        let _ = obs.on_upstream_response(&json!({"id": id, "result": {"tools": tools}}));
    }

    #[test]
    fn observed_digest_none_when_no_complete_manifest() {
        let obs = Observer::default();
        assert!(matches!(
            obs.observed_tool_digest("github.add_deploy_key"),
            enforce::ObservedToolDigest::NoCompleteManifest
        ));
    }

    #[test]
    fn observed_digest_present_then_invalidated_by_list_changed_then_restored() {
        let mut obs = Observer::default();
        observe_complete(&mut obs, 2, vec![tool("github.add_deploy_key")]);
        assert!(matches!(
            obs.observed_tool_digest("github.add_deploy_key"),
            enforce::ObservedToolDigest::Present(_)
        ));

        // A post-approval tools/list_changed invalidates the current complete observation: the drift
        // gate must NOT authorize against the prior (possibly rug-pulled) manifest until re-observed.
        obs.on_list_changed();
        assert!(
            matches!(
                obs.observed_tool_digest("github.add_deploy_key"),
                enforce::ObservedToolDigest::NoCompleteManifest
            ),
            "stale-after-change must not authorize against the prior manifest"
        );

        // A fresh complete observation restores a usable current digest.
        observe_complete(&mut obs, 4, vec![tool("github.add_deploy_key")]);
        assert!(matches!(
            obs.observed_tool_digest("github.add_deploy_key"),
            enforce::ObservedToolDigest::Present(_)
        ));
    }

    #[test]
    fn observed_digest_ambiguous_on_duplicate_tool_names() {
        let mut obs = Observer::default();
        observe_complete(&mut obs, 2, vec![tool("dup"), tool("dup")]);
        assert!(
            matches!(
                obs.observed_tool_digest("dup"),
                enforce::ObservedToolDigest::Ambiguous
            ),
            "a duplicate-name observed manifest is inconclusive -> Ambiguous, never a picked digest"
        );
    }

    #[test]
    fn observed_digest_tool_absent_is_complete_but_tool_absent() {
        let mut obs = Observer::default();
        observe_complete(&mut obs, 2, vec![tool("search")]);
        assert!(matches!(
            obs.observed_tool_digest("github.add_deploy_key"),
            enforce::ObservedToolDigest::CompleteButToolAbsent
        ));
    }

    #[test]
    fn on_upstream_response_ignores_method_bearing_messages() {
        // An upstream-to-client REQUEST carrying an id that matches a tracked list req id must NOT be
        // folded into the accumulation or complete the chain — it is a request, not a list response.
        let mut obs = Observer::default();
        obs.on_client_tools_list(Some(&json!("assay-establish-1")), false);
        let completed =
            obs.on_upstream_response(&json!({"id": "assay-establish-1", "method": "ping"}));
        assert!(
            !completed,
            "a method-bearing message must never complete a chain"
        );
        assert!(matches!(
            obs.observed_tool_digest("anything"),
            enforce::ObservedToolDigest::NoCompleteManifest
        ));
    }

    // --- run_establish orchestration (P61e Increment 2 slice 2a) ---

    #[tokio::test]
    async fn run_establish_paginates_to_complete_and_updates_observer() {
        use std::sync::atomic::AtomicU64;
        use tokio::io::{AsyncBufReadExt, BufReader};

        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        // run_establish writes its tools/list requests to `wr`; the fake reader consumes them from `rd`.
        let (mut wr, rd) = tokio::io::duplex(8192);

        // Fake upstream reader: mimic the real single reader (tap observer, then divert+resolve), over a
        // two-page cursor chain. Page 1 carries a nextCursor; page 2 is terminal.
        let fake_reader = {
            let observer = observer.clone();
            let registry = registry.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(rd).lines();
                let id1 = req_id(&lines.next_line().await.unwrap().unwrap());
                let page1 = json!({
                    "id": id1,
                    "result": {"tools": [tool("github.create_deploy_key")], "nextCursor": "cursor-2"}
                });
                observer.lock().await.on_upstream_response(&page1);
                assert!(
                    registry.resolve(&id1, page1),
                    "page 1 must be a pending establish id"
                );
                let id2 = req_id(&lines.next_line().await.unwrap().unwrap());
                let page2 = json!({"id": id2, "result": {"tools": [tool("github.list_repos")]}});
                observer.lock().await.on_upstream_response(&page2);
                assert!(
                    registry.resolve(&id2, page2),
                    "page 2 must be a pending establish id"
                );
            })
        };

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_secs(2),
        )
        .await;
        fake_reader.await.unwrap();

        assert_eq!(outcome, EstablishRunOutcome::Complete);
        assert_eq!(
            outcome.to_carrier(),
            establish::EstablishOutcome::EstablishedComplete
        );
        // The observer now holds a current complete manifest folded from both pages.
        assert!(matches!(
            observer
                .lock()
                .await
                .observed_tool_digest("github.create_deploy_key"),
            enforce::ObservedToolDigest::Present(_)
        ));
        // No pending entries leaked.
        assert!(!registry.is_pending("assay-establish-1"));
        assert!(!registry.is_pending("assay-establish-2"));
    }

    #[tokio::test]
    async fn run_establish_times_out_fail_closed_and_leaves_no_pending() {
        use std::sync::atomic::AtomicU64;

        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        // Keep the read half alive (so the write succeeds) but never resolve -> the per-operation
        // timeout elapses and establish fails closed.
        let (mut wr, _rd) = tokio::io::duplex(8192);

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_millis(20),
        )
        .await;

        // No page ever arrived, so this is TimedOut (not Partial), and it fails closed at the carrier.
        assert_eq!(outcome, EstablishRunOutcome::TimedOut);
        assert_eq!(
            outcome.to_carrier(),
            establish::EstablishOutcome::EstablishFailed
        );
        assert!(
            !registry.is_pending("assay-establish-1"),
            "a timed-out establish must reclaim its pending entry"
        );
    }

    #[tokio::test]
    async fn run_establish_register_refused_does_not_mutate_observer() {
        use std::sync::atomic::AtomicU64;

        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        // Pre-occupy the id the counter will mint, so the runner's register() is refused.
        let (_held, _rx) = registry
            .register("assay-establish-1".to_string())
            .expect("pre-register the colliding id");
        let (mut wr, _rd) = tokio::io::duplex(8192);

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_secs(2),
        )
        .await;

        assert_eq!(outcome, EstablishRunOutcome::RegisterRefused);
        // Invariant: no successful registration -> no observer mutation. No synthetic list operation
        // was started, so the observer is untouched.
        assert_eq!(observer.lock().await.observed_list_operations, 0);
        // The pre-existing pending entry was not clobbered.
        assert!(registry.is_pending("assay-establish-1"));
    }

    #[tokio::test]
    async fn run_establish_unusable_nextcursor_never_completes() {
        use std::sync::atomic::AtomicU64;
        use tokio::io::{AsyncBufReadExt, BufReader};

        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        let (mut wr, rd) = tokio::io::duplex(8192);

        // A page that claims more pages with a NON-null but non-string nextCursor (numeric): the
        // Observer treats it as "more pages pending" (incomplete), so the runner must NOT report
        // Complete. It aligns by failing closed (ErrorResponse) instead.
        let fake_reader = {
            let observer = observer.clone();
            let registry = registry.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(rd).lines();
                let id1 = req_id(&lines.next_line().await.unwrap().unwrap());
                let page = json!({"id": id1, "result": {"tools": [tool("x")], "nextCursor": 7}});
                observer.lock().await.on_upstream_response(&page);
                registry.resolve(&id1, page);
            })
        };

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_secs(2),
        )
        .await;
        fake_reader.await.unwrap();

        assert_ne!(outcome, EstablishRunOutcome::Complete);
        assert_eq!(outcome, EstablishRunOutcome::ErrorResponse);
        // The Observer also did not complete the chain, so the two views agree: not complete.
        assert!(matches!(
            observer.lock().await.observed_tool_digest("x"),
            enforce::ObservedToolDigest::NoCompleteManifest
        ));
    }

    #[tokio::test]
    async fn run_establish_write_is_bounded_by_total_deadline() {
        use std::sync::atomic::AtomicU64;

        let observer = Arc::new(Mutex::new(Observer::default()));
        let registry = relay_routing::EstablishRegistry::default();
        let counter = AtomicU64::new(1);
        // 1-byte buffer, kept open but never read: the synthetic write backpressures (not a broken
        // pipe), so an unbounded write would hang. The deadline-bounded write must time out instead.
        let (mut wr, _rd) = tokio::io::duplex(1);

        let outcome = run_establish(
            &mut wr,
            &registry,
            &observer,
            &counter,
            std::time::Duration::from_millis(20),
        )
        .await;

        assert_eq!(outcome, EstablishRunOutcome::TimedOut);
        assert!(
            !registry.is_pending("assay-establish-1"),
            "a write-timeout must still reclaim the pending entry"
        );
    }

    fn req_id(line: &str) -> String {
        serde_json::from_str::<Value>(line).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}
