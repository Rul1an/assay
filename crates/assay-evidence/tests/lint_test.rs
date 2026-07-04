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

fn observed_proxy_refusal_event(seq: u64, tool_name: &str, target_digest: &str) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "assay.mcp_call.observed",
        "urn:assay:test",
        "run_lint",
        seq,
        serde_json::json!({
            "call": {
                "tool_name": tool_name,
                "target_digest": target_digest
            },
            "observed_response": serde_json::json!({
                "jsonrpc": "2.0",
                "id": 7,
                "error": {
                    "code": -32600,
                    "message": "tool call denied by policy",
                    "data": {
                        "assay_proxy": "deny",
                        "reason": "credential_scope"
                    }
                }
            })
            .to_string()
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
        .contains("no digest-bound assay.enforcement_decision.v0 deny record"));
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
