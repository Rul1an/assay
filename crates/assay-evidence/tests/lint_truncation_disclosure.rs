//! A truncated lint run has to say so, on every path and where a consumer looks.
//!
//! `max_results` applies to every run, but the truncation flags used to be recorded only in the
//! pack metadata, which is absent whenever no packs are configured. On that path a run could drop
//! findings and present as though it had none to drop. For a tool whose own position is that a
//! partial scan must not read as a clean one, that was the wrong silence to keep.
//!
//! The SARIF half is about reach, and about not overclaiming what the disclosure buys. SARIF 2.1.0
//! has a position on a reporting cap rather than a gap where one would go: 3.14.23 is normative and
//! says `results` "SHALL be present and SHALL contain all results detected by the tool", so a
//! configured `max_results` puts this producer out of conformance whenever it fires. The informative
//! Appendix I ("Detecting incomplete result sets") does not rescue it either: its three conditions
//! each describe a tool that failed to *analyse*, not one that reported less than it found. So the
//! cap is disclosed twice and neither disclosure is claimed as conformance. `run.properties` is the
//! machine-readable home, now emitted on every path rather than only when packs are configured. The
//! notification is a human-readable aid at `warning`, which 3.58.6 defines as covering results that
//! "might be incomplete" while remaining probably valid; it deliberately misses Appendix I's `error`
//! gate, because per 3.20.21 an error-level notification means the run failed and a cap is not a
//! failure.
//!
//! An earlier version of this doc said SARIF had no construct for a reporting cap. That was wrong
//! and is retracted here as it is in `lint::sarif`: the format has a ruling, and it rules against
//! us. The disclosure is what an out-of-conformance producer owes a consumer, not a substitute for
//! conformance.

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

/// The notification exists and carries the count. This does NOT claim Appendix I coverage: at
/// `warning` it sits below that appendix's `error` gate, deliberately, since `error` would mean the
/// run failed. What it pins is that the notice is emitted and is legible to a consumer that reads
/// notifications rather than only Appendix I's three conditions.
#[test]
fn truncation_is_announced_in_a_tool_execution_notification() {
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
    // OWASP agentic-skills #49 review point: a disclosed truncation is only actionable if the
    // consumer can see the ceiling, not just the overflow. The prose still names it, for a human
    // reading the notice alone.
    assert!(
        text.contains(&report.applied_cap.to_string()),
        "the notice must name the cap the count was measured against: {text}"
    );
    // The machine-readable ceiling is NOT here. It is on the run, unconditionally, so it is
    // resolvable from every report including the ones with no notification at all. See
    // `the_cap_is_declared_once_at_run_level`.
    assert!(
        notifications[0]["properties"].get("appliedCap").is_none(),
        "the cap is configuration and belongs on the run, not duplicated per event: {}",
        notifications[0]["properties"]
    );
    assert_eq!(
        notifications[0]["properties"]["droppedCount"],
        report.truncated_count
    );
}

/// Each fact in exactly one carrier, split by what kind of fact it is.
///
/// This used to assert that both carriers disclosed the same two values, which was the best
/// available answer while the cap lived in both places: the names differed, so the invariant had to
/// be on the numbers. The split makes the question go away. The cap is configuration and is stated
/// once, on the run; the drop is an event and is stated once, in the notification. Nothing is
/// duplicated, so nothing can disagree.
#[test]
fn each_fact_travels_in_exactly_one_carrier() {
    let bundle = bundle_with_many_findings(10);
    let report = lint_capped(&bundle, Some(3));
    let sarif = to_sarif(&report);

    let note = &sarif["runs"][0]["invocations"][0]["toolExecutionNotifications"][0]["properties"];
    let run = &sarif["runs"][0]["properties"];

    assert_eq!(run["appliedCap"], 3, "the ceiling is on the run: {run}");
    assert_eq!(
        note["droppedCount"], report.truncated_count,
        "the drop is on the event: {note}"
    );

    // Neither carrier may grow the other's fact back. A future edit that "helpfully" restores the
    // cap to the notification fails here rather than on someone else's consumer.
    assert!(
        note.get("appliedCap").is_none(),
        "the cap is not duplicated per event: {note}"
    );
    assert!(
        note.get("truncatedCount").is_none(),
        "the notification carries the cross-emitter name only: {note}"
    );
    assert!(
        run.get("droppedCount").is_none(),
        "run.properties carries this tool's published names only: {run}"
    );
}

/// The reason the split is worth the churn: silence has to mean something.
///
/// `engine.rs` resolves `max_results.unwrap_or(5000)`, so every run is bounded. While the cap was
/// gated on truncation, a clean report said nothing about it, and a consumer could not tell an
/// unbounded run from a bounded one that did not fire. Since the default path is always bounded,
/// that silence was always misleading in the same direction.
#[test]
fn the_cap_is_declared_once_at_run_level() {
    let bundle = bundle_with_many_findings(3);
    let report = lint_capped(&bundle, Some(5000));
    let sarif = to_sarif(&report);

    assert!(!report.truncated, "precondition: nothing was dropped");

    let run = &sarif["runs"][0]["properties"];
    assert_eq!(
        run["appliedCap"], 5000,
        "a run that stayed under its ceiling still had one: {run}"
    );
    assert!(
        run.get("truncated").is_none() && run.get("truncatedCount").is_none(),
        "declaring the cap must not imply a drop that did not happen: {run}"
    );
}

/// The spec reading, pinned so it cannot be "fixed" into a plausible mistake later. SARIF 2.1.0
/// 3.20.14 makes `executionSuccessful` true when the engineering system knows the tool succeeded,
/// and its own example pairs true with a non-zero exit code. A capped run succeeded; it reported
/// less than it found.
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

/// The machine-readable half, which was the actual regression: `run.properties` used to be gated on
/// pack metadata, so a default-path run disclosed nothing there.
#[test]
fn truncation_reaches_run_properties_without_packs() {
    let bundle = bundle_with_many_findings(10);
    let report = lint_capped(&bundle, Some(3));
    let sarif = to_sarif(&report);

    let props = &sarif["runs"][0]["properties"];
    assert_eq!(
        props["truncated"], true,
        "no packs were configured: {props}"
    );
    assert_eq!(props["truncatedCount"], report.truncated_count);
    assert_eq!(
        props["appliedCap"], 3,
        "the run properties carry the configured ceiling, not only the overflow: {props}"
    );
}

/// Appendix I's notification condition gates on `level == "error"`, and 3.20.21 makes that mean the
/// run failed. A cap is not a failure, so this must stay below that gate. Pinned because raising it
/// looks like a fix and would contradict `executionSuccessful` three lines away.
#[test]
fn the_truncation_notice_does_not_assert_a_failed_run() {
    let bundle = bundle_with_many_findings(10);
    let report = lint_capped(&bundle, Some(3));
    let sarif = to_sarif(&report);

    let level = &sarif["runs"][0]["invocations"][0]["toolExecutionNotifications"][0]["level"];
    assert_eq!(level, "warning");
    assert_ne!(
        level, "error",
        "error would declare the run failed (3.20.21)"
    );
}
