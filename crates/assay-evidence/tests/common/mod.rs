//! Shared evidence bundle bytes for H1 alignment tests and P2a `mcp-signal-followup` tests.
//! Keep a single builder per vector so Trust Basis / MCP-001 lockstep cannot drift.

use assay_evidence::{BundleWriter, EvidenceEvent};
use chrono::{TimeZone, Utc};
use serde_json::json;

fn event_with_payload(
    type_: &str,
    run_id: &str,
    seq: u64,
    payload: serde_json::Value,
) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        type_,
        "urn:assay:test:trust-kernel-vectors",
        run_id,
        seq,
        payload,
    );
    event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
    event
}

/// Used by `mcp_signal_followup_pack` for MCP-002/003 vectors (same event source as alignment bundles).
#[allow(dead_code)] // only referenced from the P2a test binary, not the H1 binary
pub fn make_event(
    type_: &str,
    run_id: &str,
    seq: u64,
    payload: serde_json::Value,
) -> EvidenceEvent {
    event_with_payload(type_, run_id, seq, payload)
}

#[allow(dead_code)]
pub fn make_bundle(events: Vec<EvidenceEvent>) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().expect("bundle should finish");
    buffer
}

/// G3 verified + delegation + degradation (H1 lockstep + P2a full-signal coverage).
pub fn full_signal_bundle() -> Vec<u8> {
    make_bundle(vec![
        event_with_payload(
            "assay.tool.decision",
            "run_all",
            0,
            json!({
                "tool": "t",
                "decision": "allow",
                "principal": "alice@example.com",
                "auth_scheme": "jwt_bearer",
                "auth_issuer": "https://issuer.example/",
                "delegated_from": "agent:planner",
            }),
        ),
        event_with_payload(
            "assay.sandbox.degraded",
            "run_all",
            1,
            json!({
                "reason_code": "policy_conflict",
                "degradation_mode": "audit_fallback",
                "component": "landlock"
            }),
        ),
    ])
}

/// G3 absent (principal-only path): MCP-001 should fail; Trust Basis G3 absent.
pub fn g3_absent_principal_only_bundle() -> Vec<u8> {
    make_bundle(vec![event_with_payload(
        "assay.tool.decision",
        "run_g3_absent",
        0,
        json!({
            "tool": "t",
            "decision": "allow",
            "principal": "user:alice",
            "approval_state": "granted"
        }),
    )])
}
