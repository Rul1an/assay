//! H1 — same bundle bytes must yield consistent Trust Basis, MCP-001 (P2a), and Trust Card views.

use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions, LintReportWithPacks};
use assay_evidence::lint::packs::loader::load_pack;
use assay_evidence::lint::packs::LoadedPack;
use assay_evidence::{
    generate_trust_basis, trust_basis_to_trust_card, TrustBasis, TrustBasisOptions, TrustClaimId,
    TrustClaimLevel, VerifyLimits, TRUST_CARD_NON_GOALS, TRUST_CARD_SCHEMA_VERSION,
};
use assay_evidence::{BundleWriter, EvidenceEvent};
use chrono::{TimeZone, Utc};
use serde_json::json;
use std::io::Cursor;

fn make_event(type_: &str, run_id: &str, seq: u64, payload: serde_json::Value) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(type_, "urn:assay:test:h1", run_id, seq, payload);
    event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
    event
}

fn make_bundle(events: Vec<EvidenceEvent>) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().expect("bundle should finish");
    buffer
}

/// Full G3 + delegation + degradation (matches `mcp_signal_followup_pack::full_signal_bundle`).
fn bundle_g3_full_pass() -> Vec<u8> {
    make_bundle(vec![
        make_event(
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
        make_event(
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

fn bundle_g3_absent_principal_only() -> Vec<u8> {
    make_bundle(vec![make_event(
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

fn load_mcp_pack() -> LoadedPack {
    load_pack("mcp-signal-followup").expect("built-in pack should load")
}

fn lint_mcp001_findings(bundle: &[u8]) -> LintReportWithPacks {
    let options = LintOptions {
        packs: vec![load_mcp_pack()],
        max_results: Some(500),
        bundle_path: Some("h1-alignment.tar.gz".to_string()),
    };
    lint_bundle_with_options(Cursor::new(bundle), VerifyLimits::default(), options)
        .expect("lint should succeed")
}

fn has_mcp001_finding(report: &LintReportWithPacks) -> bool {
    report
        .report
        .findings
        .iter()
        .any(|f| f.rule_id.starts_with("mcp-signal-followup@") && f.rule_id.ends_with("MCP-001"))
}

fn claim_level(tb: &TrustBasis, id: TrustClaimId) -> TrustClaimLevel {
    tb.claims
        .iter()
        .find(|c| c.id == id)
        .expect("claim present")
        .level
}

/// One code path: same `bundle` bytes → Trust Basis G3 claim + MCP-001 outcome must agree.
fn assert_kernel_lockstep(bundle: &[u8], expect_g3_verified: bool, expect_mcp001_finding: bool) {
    let tb = generate_trust_basis(
        Cursor::new(bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let g3 = claim_level(&tb, TrustClaimId::AuthorizationContextVisible);
    assert_eq!(
        g3 == TrustClaimLevel::Verified,
        expect_g3_verified,
        "Trust Basis authorization_context_visible must match G3 predicate expectation"
    );

    let lint = lint_mcp001_findings(bundle);
    assert_eq!(
        has_mcp001_finding(&lint),
        expect_mcp001_finding,
        "MCP-001 finding must align with Trust Basis G3 absent/verified"
    );
}

#[test]
fn h1_same_bundle_trust_basis_and_mcp001_lockstep_full_signal() {
    let bundle = bundle_g3_full_pass();
    assert_kernel_lockstep(&bundle, true, false);
}

#[test]
fn h1_same_bundle_trust_basis_and_mcp001_lockstep_g3_absent() {
    let bundle = bundle_g3_absent_principal_only();
    assert_kernel_lockstep(&bundle, false, true);
}

#[test]
fn h1_trust_card_matches_trust_basis_claims_and_frozen_top_level() {
    let bundle = bundle_g3_full_pass();
    let tb = generate_trust_basis(
        Cursor::new(&bundle),
        VerifyLimits::default(),
        TrustBasisOptions::default(),
    )
    .expect("trust basis");
    let card = trust_basis_to_trust_card(&tb);

    assert_eq!(card.schema_version, TRUST_CARD_SCHEMA_VERSION);
    assert_eq!(card.claims.len(), 7);
    assert_eq!(
        card.claims, tb.claims,
        "Trust Card must not reclassify claims"
    );
    assert_eq!(
        card.non_goals.len(),
        TRUST_CARD_NON_GOALS.len(),
        "frozen non-goals count"
    );

    let v = serde_json::to_value(&card).expect("serialize card");
    let obj = v.as_object().expect("card is object");
    let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["claims", "non_goals", "schema_version"],
        "Trust Card top-level keys remain frozen (no parallel claim model)"
    );
}
