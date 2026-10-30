//! Separate bounded observation; never inserted into the decision stream.
use assay_core::mcp::proxy::ProxyRunReport;
use serde_json::{json, Value};

pub(super) fn event(
    report: &ProxyRunReport,
    source: &str,
    server_id: &str,
    session_id: &str,
    context_requested: bool,
    log_requested: bool,
    temporary_log: bool,
) -> Value {
    let exit_code = report.child_status.and_then(|s| s.code());
    #[cfg(unix)]
    let (signal, core_dumped) = {
        use std::os::unix::process::ExitStatusExt;
        (
            report.child_status.and_then(|s| s.signal()),
            report.child_status.map(|s| s.core_dumped()),
        )
    };
    #[cfg(not(unix))]
    let (signal, core_dumped): (Option<i32>, Option<bool>) = (None, None);
    let termination = if exit_code.is_some() {
        "exited"
    } else if signal.is_some() {
        "signaled"
    } else {
        "unknown"
    };
    let log = match report.decision_log.as_ref() {
        Some(status) if status.incomplete || report.result.is_err() => "incomplete",
        Some(_) if temporary_log => "temporary",
        Some(_) => "complete",
        None if log_requested => "unknown",
        None => "absent",
    };
    json!({
        "specversion": "1.0", "type": "assay.mcp.session.stop.v1",
        "id": format!("evt_stop_{session_id}"), "source": source,
        "time": chrono::Utc::now().to_rfc3339(), "datacontenttype": "application/json",
        "data": {
            "server_id": server_id,
            "session_context_id": context_requested.then(|| format!("evt_context_{session_id}")),
            "session_end": if report.result.is_ok() { "client_closed" } else { "proxy_error" },
            "termination": termination, "exit_code": exit_code, "signal": signal,
            "core_dumped": core_dumped, "server_pid": report.server_pid,
            "assay_kill_requested": report.assay_kill_requested,
            "decision_log": log,
            "decision_events_written": report.decision_log.as_ref().map(|s| s.written),
            "decision_events_dropped": report.decision_log.as_ref().map(|s| s.dropped),
            "decision_prefix_sha256": report.decision_log.as_ref().map(|s| &s.prefix_sha256),
            "assertion_basis": "producer_observation",
            "non_claims": ["not_execution_proof", "not_authenticated", "not_global_coverage", "not_durable_storage_proof"]
        }
    })
}
