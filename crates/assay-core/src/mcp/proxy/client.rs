mod branches;

use self::branches::{handle_policy_decision, BranchOutcome, PolicyBranchContext};
use super::decisions::{extract_tool_call_id, request_tool_call_id};
use super::frame::with_call_arguments;
use super::observation::ToolObservations;
use super::session_stop::ClientTraffic;
use super::{OutputCommit, ProxyConfig, TdtProducer};
use crate::mcp::audit::AuditLog;
use crate::mcp::decision::{DecisionEmitter, FileDecisionEmitter};
use crate::mcp::era::observe_request_metadata;
use crate::mcp::jsonrpc::{error_codes, unique_value_has_method_member, JsonRpcRequest};
use crate::mcp::policy::{McpPolicy, PolicyState};
use crate::mcp::tool_call_handler::enforce_for_wrap;
use crate::mcp::tool_decision_truth as tdt;
use crate::mcp::unique_json::{parse_client_frame, UniqueJsonError};
use std::{
    io::{self, BufRead, Write},
    process::ChildStdin,
    sync::{Arc, Mutex, OnceLock},
};

/// Outputs the caller creates only once the run is more than a probe.
pub(super) struct DeferredOutputs {
    pub(super) commit: Option<OutputCommit>,
    pub(super) decision_log: Arc<OnceLock<Arc<FileDecisionEmitter>>>,
}

impl DeferredOutputs {
    /// Create the outputs on the first call and return the receipts file, if there is one.
    fn open(&mut self) -> io::Result<Option<std::fs::File>> {
        let Some(commit) = self.commit.take() else {
            return Ok(None);
        };
        let (decision_log, receipts) = commit()?;
        if let Some(file) = decision_log {
            let _ = self
                .decision_log
                .set(Arc::new(FileDecisionEmitter::from_file(file)));
        }
        Ok(receipts)
    }
}

#[expect(clippy::too_many_arguments)]
pub(super) fn run_client_to_server(
    child_stdin: ChildStdin,
    stdout: Arc<Mutex<io::Stdout>>,
    policy: McpPolicy,
    config: ProxyConfig,
    emitter: Arc<dyn DecisionEmitter>,
    event_source: String,
    observations: Arc<Mutex<ToolObservations>>,
    receipt_file: Option<std::fs::File>,
    client_traffic: Arc<ClientTraffic>,
    mut deferred: DeferredOutputs,
) -> io::Result<()> {
    let result = forward_client_frames(
        child_stdin,
        stdout,
        policy,
        config,
        emitter,
        event_source,
        &observations,
        receipt_file,
        &client_traffic,
        &mut deferred,
    );
    // A run that ended with no frame at all is a session that did nothing, not a probe.
    let opened = if client_traffic.saw_discover() {
        Ok(None)
    } else {
        deferred.open()
    };
    result.and(opened.map(drop))
}

#[expect(clippy::too_many_arguments)]
fn forward_client_frames(
    mut child_stdin: ChildStdin,
    stdout: Arc<Mutex<io::Stdout>>,
    policy: McpPolicy,
    config: ProxyConfig,
    emitter: Arc<dyn DecisionEmitter>,
    event_source: String,
    observations: &Mutex<ToolObservations>,
    receipt_file: Option<std::fs::File>,
    client_traffic: &ClientTraffic,
    deferred: &mut DeferredOutputs,
) -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();

    let mut state = PolicyState::default();
    let mut audit_log = AuditLog::new(config.audit_log_path.as_deref());
    // Monotonic per-run observed-order counter for tool-decision-truth carriers (only advanced when the
    // opt-in producer mints a carrier).
    let mut tdt_order: i64 = 0;
    let mut receipts = receipt_file.map(super::request_receipts::Sink::new);

    while reader.read_line(&mut line)? > 0 {
        if line.trim().is_empty() {
            line.clear();
            continue;
        }
        // 1. Try Parse as MCP Request. Unique-member parse first so the tree the
        // policy sees cannot diverge from a later read of the same bytes.
        let frame = parse_client_frame(line.trim_end());
        // This frame is what needs the outputs, so a failure stops the run before it is handled.
        if !client_traffic.observe(frame.as_ref().ok()) {
            if let Some(file) = deferred.open()? {
                receipts = Some(super::request_receipts::Sink::new(file));
            }
        }
        match frame {
            Ok(value) => {
                let rpc_id_present = value.get("id").is_some();
                let request_id = value.get("id").cloned().unwrap_or(serde_json::Value::Null);
                let method_present = unique_value_has_method_member(&value);
                let request_metadata = observe_request_metadata(&value);
                let params_are_named = value.get("params").is_some_and(|p| p.is_object());
                match serde_json::from_value::<JsonRpcRequest>(value) {
                    Ok(req) if !req.is_tool_call() => {
                        // Unique non-tools/call objects (initialize, tools/list,
                        // notifications, ping) pass through byte-identically.
                        if req.method == "tools/list" {
                            if let Some(id) = &req.id {
                                observations
                                    .lock()
                                    .unwrap()
                                    .client_sent_list(id, &req.params);
                            }
                        }
                    }
                    // No tool name to decide on, so it must not pass as the empty one.
                    Ok(req) if !params_are_named || req.tool_params().is_none() => {
                        let mut out = stdout.lock().map_err(|e| io::Error::other(e.to_string()))?;
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "error": {
                                "code": error_codes::INVALID_PARAMS,
                                "message": "Invalid params"
                            }
                        });
                        out.write_all(response.to_string().as_bytes())?;
                        out.write_all(b"\n")?;
                        out.flush()?;
                        line.clear();
                        continue;
                    }
                    Ok(req) => {
                        let tool_params = req.tool_params();
                        let tool_name = tool_params
                            .as_ref()
                            .map(|params| params.name.as_str())
                            .unwrap_or_default();

                        // 2. Check Policy with Identity (Phase 9)
                        let (observation, tool_definition_binding) =
                            observations.lock().unwrap().lookup(tool_name);

                        let null_arguments = serde_json::Value::Null;
                        let tool_arguments = tool_params
                            .as_ref()
                            .map(|params| &params.arguments)
                            .unwrap_or(&null_arguments);

                        let mut policy_eval = policy.evaluate_at_session_boundary(
                            tool_name,
                            tool_arguments,
                            &mut state,
                            &observation,
                        );
                        let enforcement = enforce_for_wrap(
                            tool_name,
                            tool_arguments,
                            policy.redaction_covers_tool(tool_name),
                            &policy_eval.decision,
                            &mut policy_eval.metadata,
                            chrono::Utc::now(),
                            config.dry_run,
                        );
                        let tool_call_id = if enforcement.record_omits_arguments {
                            request_tool_call_id(&req)
                        } else {
                            extract_tool_call_id(&req, tool_params.as_ref())
                        };
                        if let Some(denial) = enforcement.denial {
                            policy_eval.decision = denial;
                        }
                        if let Some(arguments) = enforcement.effective_arguments {
                            line = with_call_arguments(&line, &arguments)?;
                        }

                        // EXPERIMENTAL opt-in: mint a tool-decision-truth carrier for this evaluated tool call,
                        // appended to its own NDJSON sink (separate from the decision log). Evidence-only — this
                        // never alters the decision below and never blocks the call, including when the call is
                        // about to be blocked: the carrier records that the decision was observed and classified.
                        if req.is_tool_call() {
                            if let Some(producer) = config.tdt_producer.as_ref() {
                                emit_tdt_carrier(
                                    producer,
                                    &policy,
                                    tool_name,
                                    tool_arguments,
                                    tdt_order,
                                    &tool_call_id,
                                    observation.identity().is_some(),
                                );
                                tdt_order += 1;
                            }
                        }

                        if enforcement.withhold_request_line {
                            if let Some(sink) = receipts.as_ref() {
                                super::request_receipts::withhold(sink);
                            }
                        }
                        let request_emitter = receipts.as_ref().map(|sink| {
                            super::request_receipts::for_request(
                                emitter.clone(),
                                sink.clone(),
                                &line,
                                rpc_id_present,
                                &request_id,
                                &request_metadata,
                            )
                        });
                        let active_emitter = request_emitter.as_ref().unwrap_or(&emitter);
                        let branch_outcome = handle_policy_decision(
                            policy_eval.decision,
                            PolicyBranchContext {
                                req: &req,
                                request_metadata: &request_metadata,
                                tool_params: tool_params.as_ref(),
                                tool_name,
                                tool_call_id: &tool_call_id,
                                metadata: &policy_eval.metadata,
                                obligation_outcomes: &enforcement.outcomes,
                                tool_definition_binding: tool_definition_binding.as_ref(),
                                config: &config,
                                emitter: active_emitter,
                                event_source: &event_source,
                                audit_log: &mut audit_log,
                            },
                            |response_json| {
                                let mut out =
                                    stdout.lock().map_err(|e| io::Error::other(e.to_string()))?;
                                out.write_all(response_json.as_bytes())?;
                                out.flush()
                            },
                        )?;

                        if branch_outcome == BranchOutcome::Blocked {
                            line.clear();
                            continue;
                        }
                    }
                    Err(_) if method_present => {
                        let mut out = stdout.lock().map_err(|e| io::Error::other(e.to_string()))?;
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request_id,
                            "error": {
                                "code": error_codes::INVALID_REQUEST,
                                "message": "Invalid Request"
                            }
                        });
                        out.write_all(response.to_string().as_bytes())?;
                        out.write_all(b"\n")?;
                        out.flush()?;
                        line.clear();
                        continue;
                    }
                    Err(_) => {
                        // Unique JSON with no method member: response / non-request, forward.
                    }
                }
            }
            Err(error) => {
                // Every rejected frame terminates here; no opaque raw fallback.
                let (code, message) = match error {
                    UniqueJsonError::InvalidFrame => {
                        (error_codes::INVALID_REQUEST, "Invalid Request")
                    }
                    _ => (error_codes::PARSE_ERROR, "Parse error"),
                };
                let mut out = stdout.lock().map_err(|e| io::Error::other(e.to_string()))?;
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": serde_json::Value::Null,
                    "error": { "code": code, "message": message }
                });
                out.write_all(response.to_string().as_bytes())?;
                out.write_all(b"\n")?;
                out.flush()?;
                line.clear();
                continue;
            }
        }

        // 3. Forward
        child_stdin.write_all(line.as_bytes())?;
        child_stdin.flush()?;
        line.clear();
    }

    Ok(())
}

/// EXPERIMENTAL: mint a tool-decision-truth carrier for one evaluated tool call and append it to the
/// producer's NDJSON sink. Evidence-only and best-effort: a build or write failure is logged to stderr
/// and never blocks or alters the call (A1 takes no runtime action on the verdict). The carrier carries
/// only the keyed `args_digest`, never raw arguments.
fn emit_tdt_carrier(
    producer: &TdtProducer,
    policy: &McpPolicy,
    tool_name: &str,
    tool_arguments: &serde_json::Value,
    order: i64,
    call_id: &str,
    identity_present: bool,
) {
    // identity_state maps only present/absent here; required_missing/invalid are richer states the proxy
    // does not yet distinguish at this seam.
    let identity_state = if identity_present {
        "present"
    } else {
        "absent"
    };
    // Minimal evidence: the proxy does not observe class membership / approval / scope / redaction at this
    // seam, so those axes stay unknown and the verdict gate resolves any DECLARED such constraint to
    // `incomplete` rather than a guessed `match`.
    let evidence = tdt::DecisionEvidence::default();
    let carrier = tdt::build_classified_record(
        policy,
        tool_name,
        tool_arguments,
        order,
        producer.key(),
        producer.key_id(),
        "authoritative_boundary",
        call_id,
        "ok",
        identity_state,
        &evidence,
    );
    let Some(carrier) = carrier else {
        eprintln!(
            "[assay] WARNING: tool-decision-truth carrier could not be built for tool '{tool_name}'; skipping (no carrier emitted)"
        );
        return;
    };
    if let Err(e) = append_carrier_line(producer.out_path(), &carrier) {
        eprintln!("[assay] WARNING: failed to append tool-decision-truth carrier: {e}");
    }
}

/// Append one carrier as a single NDJSON line to the producer sink, creating the file if absent. The sink
/// is append-only so concurrent or re-run appends never truncate earlier carriers.
fn append_carrier_line(path: &std::path::Path, carrier: &serde_json::Value) -> io::Result<()> {
    let mut line = serde_json::to_string(carrier).map_err(io::Error::other)?;
    line.push('\n');
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use tempfile::tempdir;

    #[test]
    fn proxy_contract_client_loop_inputs_remain_private() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<Arc<Mutex<ToolObservations>>>();
        assert_sync::<Arc<dyn DecisionEmitter>>();
    }

    fn test_policy() -> McpPolicy {
        serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": ["deploy"], "deny": ["delete_all"]},
            "schemas": {"deploy": {"type": "object", "required": ["env"],
                "properties": {"env": {"enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .expect("test policy")
    }

    fn test_producer(sink: std::path::PathBuf) -> TdtProducer {
        TdtProducer::new(
            sink,
            b"producer-test-key-v0".to_vec(),
            "fixture-kid-v0".to_string(),
        )
    }

    fn first_carrier(sink: &std::path::Path) -> Value {
        let body = std::fs::read_to_string(sink).expect("read sink");
        let line = body.lines().next().expect("at least one carrier line");
        serde_json::from_str(line).expect("carrier parses")
    }

    #[test]
    fn producer_emits_carrier_without_raw_args() {
        let dir = tempdir().unwrap();
        let sink = dir.path().join("carriers.ndjson");
        let producer = test_producer(sink.clone());
        // A sentinel raw argument value that must never reach the carrier (only the keyed digest may).
        let args = json!({"env": "prod", "trace": "ZZSENTINELRAWZZ"});

        emit_tdt_carrier(
            &producer,
            &test_policy(),
            "deploy",
            &args,
            0,
            "call-0",
            true,
        );

        let body = std::fs::read_to_string(&sink).unwrap();
        assert_eq!(body.lines().count(), 1, "exactly one carrier line");
        assert!(
            !body.contains("ZZSENTINELRAWZZ"),
            "raw argument value leaked into the carrier sink: {body}"
        );

        let carrier = first_carrier(&sink);
        assert_eq!(carrier["schema"], json!("assay.tool_decision_truth.v0"));
        assert_eq!(carrier["source_class"], json!("authoritative_boundary"));
        assert_eq!(carrier["result_status"], json!("ok"));
        assert_eq!(carrier["identity_state"], json!("present"));
        assert_eq!(carrier["call_id"], json!("call-0"));
        let ad = carrier["args_digest"].as_str().expect("args_digest string");
        assert!(
            ad.starts_with("hmac-sha256:fixture-kid-v0:"),
            "args_digest must be a keyed HMAC over the id, got {ad}"
        );
        // No raw-argument container key is ever present in the carrier.
        for k in ["args", "arguments", "input", "tool_arguments"] {
            assert!(
                carrier.get(k).is_none(),
                "carrier must not carry raw-arg key {k}"
            );
        }
        assert!(
            carrier.get("decision_verdict").is_some(),
            "carrier carries a verdict"
        );
    }

    #[test]
    fn producer_appends_are_append_only_and_ordered() {
        let dir = tempdir().unwrap();
        let sink = dir.path().join("carriers.ndjson");
        let producer = test_producer(sink.clone());
        let p = test_policy();

        emit_tdt_carrier(
            &producer,
            &p,
            "deploy",
            &json!({"env": "prod"}),
            0,
            "c0",
            true,
        );
        emit_tdt_carrier(
            &producer,
            &p,
            "deploy",
            &json!({"env": "staging"}),
            1,
            "c1",
            false,
        );

        let body = std::fs::read_to_string(&sink).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(
            lines.len(),
            2,
            "append-only: the second emit must not truncate the first"
        );
        let c0: Value = serde_json::from_str(lines[0]).unwrap();
        let c1: Value = serde_json::from_str(lines[1]).unwrap();
        assert_ne!(
            c0["observed_input_digest"], c1["observed_input_digest"],
            "distinct order yields distinct observed-input identity"
        );
        // identity_state is mapped from runtime-identity presence at this seam.
        assert_eq!(c0["identity_state"], json!("present"));
        assert_eq!(c1["identity_state"], json!("absent"));
    }

    #[test]
    fn producer_minimal_evidence_does_not_force_match_for_declared_obligation() {
        // The policy declares an approval obligation the proxy does not observe at this seam, so the
        // verdict gate must resolve it to `incomplete`, never a guessed `match`.
        let policy: McpPolicy = serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": ["deploy"], "approval_required": ["deploy"]},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .unwrap();
        let dir = tempdir().unwrap();
        let sink = dir.path().join("carriers.ndjson");
        let producer = test_producer(sink.clone());

        emit_tdt_carrier(
            &producer,
            &policy,
            "deploy",
            &json!({"env": "prod"}),
            0,
            "c0",
            true,
        );

        let carrier = first_carrier(&sink);
        assert_eq!(
            carrier["decision_verdict"],
            json!("incomplete"),
            "an unobserved declared obligation must resolve to incomplete"
        );
    }
}
