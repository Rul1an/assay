mod branches;

use self::branches::{handle_policy_decision, BranchOutcome, PolicyBranchContext};
use super::decisions::extract_tool_call_id;
use super::{ProxyConfig, TdtProducer};
use crate::mcp::audit::AuditLog;
use crate::mcp::decision::DecisionEmitter;
use crate::mcp::identity::ToolIdentity;
use crate::mcp::jsonrpc::JsonRpcRequest;
use crate::mcp::policy::{McpPolicy, PolicyState};
use crate::mcp::tool_decision_truth as tdt;
use crate::mcp::tool_definition::ToolDefinitionBinding;
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    process::ChildStdin,
    sync::{Arc, Mutex},
};

#[expect(clippy::too_many_arguments)]
pub(super) fn run_client_to_server(
    mut child_stdin: ChildStdin,
    stdout: Arc<Mutex<io::Stdout>>,
    policy: McpPolicy,
    config: ProxyConfig,
    emitter: Arc<dyn DecisionEmitter>,
    event_source: String,
    identity_cache: Arc<Mutex<HashMap<String, ToolIdentity>>>,
    tool_definition_cache: Arc<Mutex<HashMap<String, ToolDefinitionBinding>>>,
) -> io::Result<()> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line = String::new();

    let mut state = PolicyState::default();
    let mut audit_log = AuditLog::new(config.audit_log_path.as_deref());
    // Monotonic per-run observed-order counter for tool-decision-truth carriers (only advanced when the
    // opt-in producer mints a carrier).
    let mut tdt_order: i64 = 0;

    while reader.read_line(&mut line)? > 0 {
        // 1. Try Parse as MCP Request
        match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => {
                let tool_params = req.tool_params();
                let tool_name = tool_params
                    .as_ref()
                    .map(|params| params.name.as_str())
                    .unwrap_or_default();

                // 2. Check Policy with Identity (Phase 9)
                let (runtime_id, tool_definition_binding) = if req.is_tool_call() {
                    let runtime_id = {
                        let cache = identity_cache.lock().unwrap();
                        cache.get(tool_name).cloned()
                    };
                    let tool_definition_binding = {
                        let cache = tool_definition_cache.lock().unwrap();
                        cache.get(tool_name).cloned()
                    };
                    (runtime_id, tool_definition_binding)
                } else {
                    (None, None)
                };

                let tool_call_id = extract_tool_call_id(&req, tool_params.as_ref());
                let null_arguments = serde_json::Value::Null;
                let tool_arguments = tool_params
                    .as_ref()
                    .map(|params| &params.arguments)
                    .unwrap_or(&null_arguments);

                let policy_eval = policy.evaluate_with_metadata(
                    tool_name,
                    tool_arguments,
                    &mut state,
                    runtime_id.as_ref(),
                );

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
                            runtime_id.is_some(),
                        );
                        tdt_order += 1;
                    }
                }

                let branch_outcome = handle_policy_decision(
                    policy_eval.decision,
                    PolicyBranchContext {
                        req: &req,
                        tool_params: tool_params.as_ref(),
                        tool_name,
                        tool_call_id: &tool_call_id,
                        metadata: &policy_eval.metadata,
                        tool_definition_binding: tool_definition_binding.as_ref(),
                        config: &config,
                        emitter: &emitter,
                        event_source: &event_source,
                        audit_log: &mut audit_log,
                    },
                    |response_json| {
                        let mut out = stdout.lock().map_err(|e| io::Error::other(e.to_string()))?;
                        out.write_all(response_json.as_bytes())?;
                        out.flush()
                    },
                )?;

                if branch_outcome == BranchOutcome::Blocked {
                    line.clear();
                    continue;
                }
            }
            Err(_) => {
                // Hardening: Suspicious Unparsable JSON
                let trimmed = line.trim();
                if trimmed.starts_with('{')
                    && (trimmed.contains("\"method\"")
                        || trimmed.contains("\"params\"")
                        || trimmed.contains("\"tool\""))
                {
                    eprintln!("[assay] WARNING: Suspicious unparsable JSON, forwarding anyway (potential bypass attempt?): {:.60}...", trimmed);
                }
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

        assert_send::<Arc<Mutex<HashMap<String, ToolIdentity>>>>();
        assert_send::<Arc<Mutex<HashMap<String, ToolDefinitionBinding>>>>();
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
