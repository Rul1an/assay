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

#[test]
fn test_golden_bundle_zero_findings() {
    let bundle = create_golden_bundle();
    let report = lint_bundle(Cursor::new(&bundle), VerifyLimits::default()).unwrap();

    assert!(report.verified);
    assert_eq!(report.findings.len(), 0);
    assert_eq!(report.summary.total, 0);
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

    // Check automation details
    let automation_id = run["automationDetails"]["id"].as_str().unwrap();
    assert!(automation_id.starts_with("assay-evidence/lint/default@"));
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
