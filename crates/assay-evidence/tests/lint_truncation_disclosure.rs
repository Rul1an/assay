//! A truncated lint run has to say so, on every path and where a consumer looks.
//!
//! `max_results` applies to every run, but the truncation flags used to be recorded only in the
//! pack metadata, which is absent whenever no packs are configured. On that path a run could drop
//! findings and present as though it had none to drop. For a tool whose own position is that a
//! partial scan must not read as a clean one, that was the wrong silence to keep.
//!
//! The SARIF half is about placement rather than presence. `run.properties.truncated` is a vendor
//! extension; SARIF 2.1.0 Appendix I, "Detecting incomplete result sets", names the two places a
//! consumer actually examines, and `invocations[].toolExecutionNotifications` is the one that
//! carries conditions bearing on completeness.

use assay_evidence::bundle::BundleWriter;
use assay_evidence::lint::engine::{lint_bundle_with_options, LintOptions};
use assay_evidence::lint::sarif::to_sarif;
use assay_evidence::types::EvidenceEvent;
use assay_evidence::VerifyLimits;
use chrono::{TimeZone, Utc};
use std::io::Cursor;

/// Events that each trip a built-in rule, so findings outnumber a small cap.
fn bundle_with_many_findings(n: u64) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for seq in 0..n {
        let mut event = EvidenceEvent::new(
            "assay.net.connect",
            "urn:assay:test",
            "run_truncation",
            seq,
            serde_json::json!({"url": "https://api.example.com"}),
        );
        event.time = Utc.timestamp_opt(1_700_000_000 + seq as i64, 0).unwrap();
        event = event.with_subject(format!(
            "https://api.example.com?api_key=sk-{seq:016}abcdef"
        ));
        writer.add_event(event);
    }
    writer.finish().unwrap();
    buffer
}

fn lint_capped(bundle: &[u8], max_results: Option<usize>) -> assay_evidence::lint::LintReport {
    lint_bundle_with_options(
        Cursor::new(bundle),
        VerifyLimits::default(),
        LintOptions {
            packs: Vec::new(),
            max_results,
            bundle_path: None,
        },
    )
    .expect("lint should succeed")
    .report
}

/// The root case: no packs configured, so nothing else could have carried this.
#[test]
fn a_truncated_default_path_run_records_it_on_the_report() {
    let bundle = bundle_with_many_findings(10);
    let report = lint_capped(&bundle, Some(3));

    assert!(
        report.truncated,
        "a run that dropped findings must say so even with no packs configured"
    );
    assert_eq!(report.findings.len(), 3, "the cap still applies");
    assert!(
        report.truncated_count > 0,
        "the count of dropped findings must be reported, not just the fact"
    );
}

#[test]
fn an_untruncated_run_reports_no_truncation() {
    let bundle = bundle_with_many_findings(3);
    let report = lint_capped(&bundle, Some(5000));

    assert!(!report.truncated, "nothing was dropped");
    assert_eq!(report.truncated_count, 0);
}

/// Placement, not presence. A generic consumer reads Appendix I's two indicators, so the notice has
/// to be one of them rather than a vendor property it has no reason to know about.
#[test]
fn truncation_reaches_sarif_where_a_consumer_examines_completeness() {
    let bundle = bundle_with_many_findings(10);
    let report = lint_capped(&bundle, Some(3));
    let sarif = to_sarif(&report);

    let invocation = &sarif["runs"][0]["invocations"][0];
    let notifications = invocation["toolExecutionNotifications"]
        .as_array()
        .expect("a truncated run must carry a tool execution notification");
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0]["descriptor"]["id"], "ASSAY-LINT-TRUNCATED");
    assert_eq!(notifications[0]["level"], "warning");
    let text = notifications[0]["message"]["text"]
        .as_str()
        .expect("message text");
    assert!(
        text.contains("incomplete"),
        "the notice must name the condition in the consumer's vocabulary: {text}"
    );
    assert!(
        text.contains(&report.truncated_count.to_string()),
        "the notice must carry how many were dropped: {text}"
    );
}

/// The spec reading, pinned so it cannot be "fixed" into a plausible mistake later. SARIF 2.1.0
/// 3.20.14 ties `executionSuccessful` to whether the tool analyzed the full set of specified
/// targets. Truncation caps what is reported, not what was analyzed, so this stays true and the
/// completeness signal travels in the notification instead.
#[test]
fn truncation_does_not_claim_the_analysis_failed() {
    let bundle = bundle_with_many_findings(10);
    let report = lint_capped(&bundle, Some(3));
    let sarif = to_sarif(&report);

    assert_eq!(
        sarif["runs"][0]["invocations"][0]["executionSuccessful"], true,
        "a capped report is still a completed analysis"
    );
}

#[test]
fn an_untruncated_run_carries_no_notification() {
    let bundle = bundle_with_many_findings(3);
    let report = lint_capped(&bundle, Some(5000));
    let sarif = to_sarif(&report);

    assert!(
        sarif["runs"][0]["invocations"][0]
            .get("toolExecutionNotifications")
            .is_none(),
        "a complete run must not carry an incompleteness notice, or the notice means nothing"
    );
}
