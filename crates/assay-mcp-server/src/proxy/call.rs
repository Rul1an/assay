use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{atomic::AtomicU64, Arc};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Mutex};

use super::establish_runner::{run_establish, EstablishRunOutcome};
use super::io::{
    append_decision_record, forward_line, proxy_error_line, PROXY_DENIED, PROXY_FAILED,
};
use super::{
    annotation_conformance, denied_observation, enforce, establish, observer::Observer,
    relay_routing,
};

pub(super) struct EnforcementRuntime<'a> {
    pub(super) policy: &'a enforce::EnforcePolicy,
    pub(super) baseline: &'a enforce::DeclaredManifest,
    pub(super) decision_out: &'a Option<PathBuf>,
    pub(super) denied_call_observation_out: &'a Option<PathBuf>,
    pub(super) establish_out: &'a Option<PathBuf>,
    pub(super) tool_conformance_out: &'a Option<PathBuf>,
    pub(super) establish_budget: Duration,
    pub(super) registry: &'a relay_routing::EstablishRegistry,
    pub(super) establish_id_counter: &'a AtomicU64,
    pub(super) observer: &'a Arc<Mutex<Observer>>,
    pub(super) tx: &'a mpsc::UnboundedSender<String>,
}

/// Enforce one privileged `tools/call`. This is the only path that forwards a privileged call, and only
/// after the PDP clears classification, caller allowance, credential scope, and drift gates.
pub(super) async fn handle_tools_call<W: AsyncWriteExt + Unpin>(
    child_stdin: &mut W,
    runtime: EnforcementRuntime<'_>,
    v: &Value,
    line: &str,
) -> std::io::Result<()> {
    let tool_name = v
        .pointer("/params/name")
        .and_then(|n| n.as_str())
        .unwrap_or("");
    let empty = json!({});
    let call_args = v.pointer("/params/arguments").unwrap_or(&empty);

    // The current observed per-tool digest, from this session's observed tools/list (read-only). The
    // guard is dropped before any await below.
    let observed = runtime
        .observer
        .lock()
        .await
        .observed_tool_digest(tool_name);
    let mut decision = enforce::decide(
        runtime.policy,
        runtime.baseline,
        &observed,
        tool_name,
        call_args,
    );

    // Pre-decision recovery for the manifest-availability gap ONLY. This never relaxes a gate:
    // Ambiguous, baseline-missing, and real drift skip establish and stay denied.
    let est_action = establish::establish_action(&observed);
    let mut run_outcome: Option<EstablishRunOutcome> = None;
    if !decision.allow
        && decision.reason == "manifest_current_observation_incomplete"
        && matches!(
            est_action,
            establish::EstablishAction::ReList | establish::EstablishAction::ReListOnce
        )
    {
        let outcome = run_establish(
            child_stdin,
            runtime.registry,
            runtime.observer,
            runtime.establish_id_counter,
            runtime.establish_budget,
        )
        .await;
        run_outcome = Some(outcome);
    }

    // Capture ONE effective observation after establish has run or been skipped, and decide the FINAL
    // verdict from it. The tool-annotation carrier reads its declared annotations and digest from this
    // SAME snapshot, so the verdict and the carrier cannot diverge.
    let (eff_digest, eff_annotations) = runtime
        .observer
        .lock()
        .await
        .effective_tool_observation(tool_name);
    decision = enforce::decide(
        runtime.policy,
        runtime.baseline,
        &eff_digest,
        tool_name,
        call_args,
    );
    if run_outcome.is_some() {
        tracing::info!(
            event = "establish_attempted",
            tool = %tool_name,
            run_outcome = ?run_outcome,
            effective_decision = if decision.allow { "allow" } else { "deny" },
            reason = decision.reason,
            note = "pre-call manifest-establish; verdict lives in enforcement_decision.v0"
        );
    }

    let est_outcome = run_outcome
        .map(|r| r.to_carrier())
        .unwrap_or(establish::EstablishOutcome::NotPerformed);
    let establish_path = establish::establish_path(est_action, est_outcome, decision.allow);
    let run_outcome_str = run_outcome
        .map(|r| r.as_str())
        .unwrap_or(establish::RUN_OUTCOME_NOT_PERFORMED);
    tracing::info!(
        event = "enforce_decision",
        caller = %runtime.policy.caller.id,
        tool = %tool_name,
        action_class = decision.action_class.as_deref().unwrap_or("none"),
        target_digest = decision.target_digest.as_deref().unwrap_or("none"),
        decision = if decision.allow { "allow" } else { "deny" },
        reason = decision.reason,
        note = "diagnostic decision log; not canonical evidence"
    );

    let decision_record_ok = match runtime.decision_out {
        Some(path) => {
            let record = enforce::decision_record(runtime.policy, &decision, tool_name, call_args);
            append_record_or_log("enforcement_record_write_failed", path, &record)
        }
        None => true,
    };
    let establish_record_ok = match runtime.establish_out {
        Some(path) => {
            let record = establish::build_manifest_establish_record(
                establish_path,
                decision.action_class.as_deref(),
                run_outcome_str,
            );
            append_record_or_log("manifest_establish_record_write_failed", path, &record)
        }
        None => true,
    };
    let conformance_record_ok = match runtime.tool_conformance_out {
        Some(path) => {
            let basis = if matches!(eff_digest, enforce::ObservedToolDigest::Present(_)) {
                annotation_conformance::ObservationBasis::Complete
            } else {
                annotation_conformance::ObservationBasis::Incomplete
            };
            let declared = annotation_conformance::extract_declared_annotations(
                eff_annotations.as_ref().unwrap_or(&Value::Null),
            );
            let digest = match &eff_digest {
                enforce::ObservedToolDigest::Present(d) => Some(d.as_str()),
                _ => None,
            };
            let record = annotation_conformance::build_tool_annotation_conformance_record(
                basis, &declared, tool_name, digest, call_args,
            );
            append_record_or_log("tool_conformance_record_write_failed", path, &record)
        }
        None => true,
    };
    let records_ok = decision_record_ok && establish_record_ok && conformance_record_ok;

    if decision.allow && !records_ok {
        if let Some(id) = v.get("id") {
            if !id.is_null() {
                let _ = runtime.tx.send(proxy_error_line(
                    id.clone(),
                    PROXY_FAILED,
                    "enforcement_record_write_failed",
                    "a required evidence record (enforcement_decision / manifest_establish / tool_annotation_conformance) could not be written; call not forwarded",
                ));
            }
        }
    } else if decision.allow {
        // Forward the privileged call; the upstream's response relays verbatim via the upstream reader.
        forward_line(child_stdin, line).await?;
    } else {
        // A deny stands regardless of a record-write failure; a missing deny-record is logged above.
        match v.get("id") {
            Some(id) if !id.is_null() => {
                let response_line = proxy_error_line(
                    id.clone(),
                    PROXY_DENIED,
                    decision.reason,
                    &format!("tools/call denied by enforcing proxy: {}", decision.reason),
                );
                if let Some(path) = runtime.denied_call_observation_out {
                    let record = denied_observation::denied_call_observation_record(
                        tool_name,
                        decision.target_digest.as_deref(),
                        PROXY_DENIED,
                        decision.reason,
                        &response_line,
                    );
                    let _ =
                        append_record_or_log("denied_call_observation_write_failed", path, &record);
                }
                let _ = runtime.tx.send(response_line);
            }
            _ => { /* denied notification: nothing to answer, drop it */ }
        }
    }
    Ok(())
}

fn append_record_or_log(event: &'static str, path: &std::path::Path, record: &Value) -> bool {
    match append_decision_record(path, record) {
        Ok(()) => true,
        Err(e) => {
            tracing::error!(event, error = %e, path = %path.display());
            false
        }
    }
}
