use assay_evidence::bundle::BundleWriter;
use assay_evidence::lint::engine::lint_bundle;
use assay_evidence::lint::sarif::to_sarif;
use assay_evidence::lint::Severity;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::VerifyLimits;
use chrono::{TimeZone, Utc};
use std::io::Cursor;

fn create_golden_bundle() -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for seq in 0..3u64 {
        let mut event = EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_lint",
            seq,
            serde_json::json!({"seq": seq}),
        );
        event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
        writer.add_event(event);
    }
    writer.finish().unwrap();
    buffer
}

fn create_bundle_with_secret_subject() -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    let mut event = EvidenceEvent::new(
        "assay.net.connect",
        "urn:assay:test",
        "run_secret",
        0,
        serde_json::json!({"url": "https://api.example.com"}),
    );
    event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    event = event.with_subject("https://api.example.com?api_key=sk-1234567890abcdef");
    writer.add_event(event);
    writer.finish().unwrap();
    buffer
}

fn create_bundle_from_events(events: Vec<EvidenceEvent>) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().unwrap();
    buffer
}

/// Mirrors the shipped `assay.denied_call_observation.v0` producer
/// (assay-mcp-server/src/proxy/denied_observation.rs).
fn observed_proxy_refusal_event(seq: u64, tool_name: &str, target_digest: &str) -> EvidenceEvent {
    denied_call_observation_event(seq, tool_name, serde_json::json!(target_digest))
}

fn denied_call_observation_event(
    seq: u64,
    tool_name: &str,
    target_digest: serde_json::Value,
) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "assay.mcp_call.observed",
        "urn:assay:test",
        "run_lint",
        seq,
        serde_json::json!({
            "schema": "assay.denied_call_observation.v0",
            "call": {
                "tool_name": tool_name,
                "target_digest": target_digest
            },
            "caller_visible_error": {
                "code": -32042,
                "origin": "assay-proxy",
                "reason": "credential_scope_insufficient"
            },
            "caller_visible_response_digest":
                "sha256:6d5a1f3b8c9e2d4a7b0c1e5f8a3d6b9c2e5f8a1d4b7c0e3f6a9d2c5b8e1f4a7d",
            "non_claims": [
                "caller-visible proxy denial observation only; policy decision lives in assay.enforcement_decision.v0",
                "does not assert or verify the upstream side effect",
                "does not assert maliciousness, safety, approval, or whole-action trust",
                "must not be read as a replacement for the bound enforcement decision record"
            ]
        }),
    );
    event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
    event
}

fn enforcement_decision_event(
    seq: u64,
    tool_name: &str,
    target_digest: &str,
    decision: &str,
) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "assay.enforcement_decision",
        "urn:assay:test",
        "run_lint",
        seq,
        serde_json::json!({
            "schema": "assay.enforcement_decision.v0",
            "tool": {
                "name": tool_name,
                "action_class": "fs_write"
            },
            "action": {
                "verb": "write",
                "resource_type": "file",
                "target": {
                    "path": "/workspace/out/report.md"
                },
                "target_digest": target_digest
            },
            "decision": decision,
            "reason": decision,
            "fail_closed": decision == "deny"
        }),
    );
    event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
    event
}

fn prose_only_guardrail_log_event(seq: u64) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "assay.mcp_call.observed",
        "urn:assay:test",
        "run_lint",
        seq,
        serde_json::json!({
            "call": {
                "tool_name": "fs_write",
                "target_digest": "sha256:call-target"
            },
            "producer_log": ["guardrail blocked this call"]
        }),
    );
    event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
    event
}

#[test]
fn test_golden_bundle_zero_findings() {
    let bundle = create_golden_bundle();
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    assert!(report.verified);
    assert_eq!(report.findings.len(), 0);
    assert_eq!(report.summary.total, 0);
}

#[test]
fn test_w004_flags_proxy_marker_without_bound_decision_record() {
    let bundle = create_bundle_from_events(vec![observed_proxy_refusal_event(
        0,
        "fs_write",
        "sha256:call-target",
    )]);
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    let findings: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.rule_id == "ASSAY-W004")
        .collect();
    assert_eq!(findings.len(), 1, "expected one ASSAY-W004 finding");
    assert!(findings[0]
        .message
        .contains("not backed by a digest-bound assay.enforcement_decision.v0 deny record"));
}

#[test]
fn test_w004_flags_proxy_marker_contradicted_by_bound_allow_record() {
    let bundle = create_bundle_from_events(vec![
        observed_proxy_refusal_event(0, "fs_write", "sha256:call-target"),
        enforcement_decision_event(1, "fs_write", "sha256:call-target", "allow"),
    ]);
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    let findings: Vec<_> = report
        .findings
        .iter()
        .filter(|finding| finding.rule_id == "ASSAY-W004")
        .collect();
    assert_eq!(findings.len(), 1, "expected one ASSAY-W004 finding");
    assert!(findings[0].message.contains("contradicted by"));
}

#[test]
fn test_w004_accepts_proxy_marker_with_bound_deny_record() {
    let bundle = create_bundle_from_events(vec![
        observed_proxy_refusal_event(0, "fs_write", "sha256:call-target"),
        enforcement_decision_event(1, "fs_write", "sha256:call-target", "deny"),
    ]);
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    assert!(
        report
            .findings
            .iter()
            .all(|finding| finding.rule_id != "ASSAY-W004"),
        "bound deny record should satisfy ASSAY-W004"
    );
}

#[test]
fn test_w004_sarif_rule_includes_security_severity() {
    let bundle = create_bundle_from_events(vec![observed_proxy_refusal_event(
        0,
        "fs_write",
        "sha256:call-target",
    )]);
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();
    let sarif = to_sarif(&report);

    let rules = sarif["runs"][0]["tool"]["driver"]["rules"]
        .as_array()
        .unwrap();
    let w004_rule = rules
        .iter()
        .find(|rule| rule["id"] == "ASSAY-W004")
        .unwrap();
    assert_eq!(
        w004_rule["properties"]["security-severity"].as_str(),
        Some("4.0")
    );
}

#[test]
fn test_w004_does_not_match_prose_only_guardrail_log() {
    let bundle = create_bundle_from_events(vec![prose_only_guardrail_log_event(0)]);
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    assert!(
        report
            .findings
            .iter()
            .all(|finding| finding.rule_id != "ASSAY-W004"),
        "ASSAY-W004 must not fall back to producer-log prose matching"
    );
}

#[test]
fn test_w004_skips_unbindable_observation_without_target_digest() {
    // The shipped carrier emits `target_digest: null` when classification produced no digest.
    // Binding cannot be checked for such an observation, so it is out of W004's scope.
    let bundle = create_bundle_from_events(vec![denied_call_observation_event(
        0,
        "fs_write",
        serde_json::Value::Null,
    )]);
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    assert!(
        report
            .findings
            .iter()
            .all(|finding| finding.rule_id != "ASSAY-W004"),
        "an unbindable observation must not produce an ASSAY-W004 finding"
    );
}

/// Mirrors the shipped approval-retention block on `assay.enforcement_decision.v0`
/// (assay-core src/mcp/decision_next/event_types.rs). `retained_view` None omits the block
/// entirely, matching a decision without an approval basis.
fn approval_decision_event(
    seq: u64,
    retained_view: Option<&str>,
    with_plaintext_commitment: bool,
) -> EvidenceEvent {
    let mut payload = serde_json::json!({
        "schema": "assay.enforcement_decision.v0",
        "tool": { "name": "fs_write", "action_class": "fs_write" },
        "action": {
            "verb": "write",
            "resource_type": "file",
            "target": { "path": "/workspace/out/report.md" },
            "target_digest": "sha256:call-target"
        },
        "decision": "allow"
    });
    if let Some(view) = retained_view {
        payload["approval_retained_view"] = serde_json::json!(view);
        payload["approval_artifact_digest"] = serde_json::json!(
            "sha256:2b7c1e5f8a3d6b9c2e5f8a1d4b7c0e3f6a9d2c5b8e1f4a7d6d5a1f3b8c9e2d4a"
        );
        payload["approval_artifact_digest_alg"] = serde_json::json!("sha256");
    }
    if with_plaintext_commitment {
        payload["approval_plaintext_commitment"] = serde_json::json!(
            "sha256:8a3d6b9c2e5f8a1d4b7c0e3f6a9d2c5b8e1f4a7d6d5a1f3b8c9e2d4a2b7c1e5f"
        );
    }
    let mut event = EvidenceEvent::new(
        "assay.enforcement_decision",
        "urn:assay:test",
        "run_lint",
        seq,
        payload,
    );
    event.time = Utc.timestamp_opt(1700000000 + seq as i64, 0).unwrap();
    event
}

fn w005_findings(bundle: &[u8]) -> Vec<String> {
    let report = lint_bundle(Cursor::new(bundle), VerifyLimits::default()).unwrap();
    report
        .findings
        .iter()
        .filter(|finding| finding.rule_id == "ASSAY-W005")
        .map(|finding| finding.message.clone())
        .collect()
}

#[test]
fn test_w005_flags_encrypted_retained_view_as_opaque_unbindable() {
    let bundle =
        create_bundle_from_events(vec![approval_decision_event(0, Some("encrypted"), false)]);
    let findings = w005_findings(&bundle);
    assert_eq!(findings.len(), 1, "expected one ASSAY-W005 finding");
    assert!(findings[0].contains("opaque_unbindable"));
    assert!(findings[0].contains("cap at incomplete"));
}

#[test]
fn test_w005_flags_encrypted_view_with_commitment_as_opaque_bindable() {
    let bundle =
        create_bundle_from_events(vec![approval_decision_event(0, Some("encrypted"), true)]);
    let findings = w005_findings(&bundle);
    assert_eq!(findings.len(), 1, "expected one ASSAY-W005 finding");
    assert!(findings[0].contains("opaque_bindable"));
    assert!(findings[0].contains("checkable against the commitment"));
}

#[test]
fn test_w005_flags_unknown_retained_view_fail_closed() {
    let bundle = create_bundle_from_events(vec![approval_decision_event(
        0,
        Some("rendered_screenshot"),
        false,
    )]);
    let findings = w005_findings(&bundle);
    assert_eq!(findings.len(), 1, "expected one ASSAY-W005 finding");
    assert!(findings[0].contains("unknown retained view 'rendered_screenshot'"));
    assert!(findings[0].contains("fail-closed"));
}

#[test]
fn test_w005_silent_on_shipped_structured_meta_jcs_view() {
    let bundle = create_bundle_from_events(vec![approval_decision_event(
        0,
        Some("structured_meta_jcs"),
        false,
    )]);
    assert!(
        w005_findings(&bundle).is_empty(),
        "the shipped readable view must not produce an ASSAY-W005 finding"
    );
}

#[test]
fn test_w005_silent_without_approval_retention_block() {
    let bundle = create_bundle_from_events(vec![
        approval_decision_event(0, None, false),
        enforcement_decision_event(1, "fs_write", "sha256:call-target", "deny"),
    ]);
    assert!(
        w005_findings(&bundle).is_empty(),
        "a decision without a declared retained view is out of W005's scope"
    );
}

#[test]
fn test_secret_in_subject_detected() {
    let bundle = create_bundle_with_secret_subject();
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    assert!(report.verified);
    assert!(!report.findings.is_empty());

    // Should have at least ASSAY-W001 (secret detection)
    let secret_findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.rule_id == "ASSAY-W001")
        .collect();
    assert!(!secret_findings.is_empty(), "Expected ASSAY-W001 finding");

    // Fingerprint should be stable
    let fp1 = &secret_findings[0].fingerprint;
    assert!(fp1.starts_with("sha256:"));
}

#[test]
fn test_stable_fingerprint() {
    let bundle = create_bundle_with_secret_subject();
    let report1 = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();
    let report2 = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    assert_eq!(report1.findings.len(), report2.findings.len());
    for (f1, f2) in report1.findings.iter().zip(report2.findings.iter()) {
        assert_eq!(
            f1.fingerprint, f2.fingerprint,
            "Fingerprints must be stable"
        );
    }
}

#[test]
fn test_corrupt_bundle_fails_verification() {
    let result = lint_bundle(Cursor::new(&[0xDE, 0xAD]), VerifyLimits::default());
    assert!(result.is_err());
}

#[test]
fn test_sarif_output_valid() {
    let bundle = create_bundle_with_secret_subject();
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();
    let sarif = to_sarif(&report);

    // Basic SARIF structure validation
    assert_eq!(sarif["version"], "2.1.0");
    assert!(sarif["runs"].is_array());

    let runs = sarif["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 1);

    let run = &runs[0];
    assert!(run["tool"]["driver"]["name"].is_string());
    assert!(run["tool"]["driver"]["rules"].is_array());
    assert!(run["results"].is_array());

    // Check automation details — includes run_id for cross-bundle uniqueness
    let automation_id = run["automationDetails"]["id"].as_str().unwrap();
    assert!(
        automation_id.starts_with("assay-evidence/lint/"),
        "automation_id should start with 'assay-evidence/lint/', got: {}",
        automation_id
    );

    // Check security-severity on rules that have it
    let rules = run["tool"]["driver"]["rules"].as_array().unwrap();
    let w001_rule = rules.iter().find(|r| r["id"] == "ASSAY-W001").unwrap();
    let security_sev = w001_rule["properties"]["security-severity"]
        .as_str()
        .unwrap();
    assert_eq!(security_sev, "7.0");

    // Check fingerprint on results
    let results = run["results"].as_array().unwrap();
    assert!(!results.is_empty());
    let first_result = &results[0];
    let fp = first_result["partialFingerprints"]["assayLintFingerprint/v1"]
        .as_str()
        .unwrap();
    assert!(fp.starts_with("sha256:"));
}

#[test]
fn test_has_findings_at_or_above() {
    let bundle = create_bundle_with_secret_subject();
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    // Secret findings are Warn level
    assert!(report.has_findings_at_or_above(&Severity::Warn));
    assert!(report.has_findings_at_or_above(&Severity::Info));
    // No Error-level findings in this bundle
    // (ASSAY-W003 is also Warn, not Error)
}
