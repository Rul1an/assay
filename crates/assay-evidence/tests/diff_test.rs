use assay_evidence::bundle::BundleWriter;
use assay_evidence::diff::engine::{compare_bundles, diff_bundles};
use assay_evidence::types::EvidenceEvent;
use assay_evidence::VerifyLimits;
use chrono::{TimeZone, Utc};
use std::io::Cursor;

fn create_bundle(_run_id: &str, events: Vec<EvidenceEvent>) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut writer = BundleWriter::new(&mut buffer);
    for ev in events {
        writer.add_event(ev);
    }
    writer.finish().unwrap();
    buffer
}

fn base_events(run_id: &str) -> Vec<EvidenceEvent> {
    let mut events = Vec::new();
    // Profile started
    let mut e0 = EvidenceEvent::new(
        "assay.profile.started",
        "urn:assay:test",
        run_id,
        0,
        serde_json::json!({"name": "test"}),
    );
    e0.time = Utc.timestamp_opt(1700000000, 0).unwrap();
    events.push(e0);

    // Network event
    let mut e1 = EvidenceEvent::new(
        "assay.net.connect",
        "urn:assay:test",
        run_id,
        1,
        serde_json::json!({"host": "api.example.com"}),
    );
    e1.time = Utc.timestamp_opt(1700000001, 0).unwrap();
    e1 = e1.with_subject("api.example.com:443");
    events.push(e1);

    // FS event
    let mut e2 = EvidenceEvent::new(
        "assay.fs.access",
        "urn:assay:test",
        run_id,
        2,
        serde_json::json!({"path": "/etc/config"}),
    );
    e2.time = Utc.timestamp_opt(1700000002, 0).unwrap();
    e2 = e2.with_subject("/etc/config");
    events.push(e2);

    // Process exec
    let mut e3 = EvidenceEvent::new(
        "assay.process.exec",
        "urn:assay:test",
        run_id,
        3,
        serde_json::json!({"cmd": "curl"}),
    );
    e3.time = Utc.timestamp_opt(1700000003, 0).unwrap();
    e3 = e3.with_subject("curl");
    events.push(e3);

    events
}

#[test]
fn test_identical_bundles_empty_diff() {
    let events = base_events("run_base");
    let bundle = create_bundle("run_base", events);

    let comparison = compare_bundles(
        Cursor::new(&bundle),
        Cursor::new(&bundle),
        VerifyLimits::default(),
    )
    .unwrap();
    let report = &comparison.report;

    assert!(comparison.is_empty());
    assert!(report.is_empty());
    assert!(comparison.retained_events.run_root_equal);
    assert_eq!(report.summary.event_count_delta, 0);
    assert!(report.network.added.is_empty());
    assert!(report.network.removed.is_empty());
    assert!(report.filesystem.added.is_empty());
    assert!(report.processes.added.is_empty());
}

#[test]
fn test_extra_network_event_detected() {
    let baseline_events = base_events("run_base");
    let baseline = create_bundle("run_base", baseline_events);

    let mut candidate_events = base_events("run_cand");
    // Add extra network event
    let mut extra = EvidenceEvent::new(
        "assay.net.connect",
        "urn:assay:test",
        "run_cand",
        4,
        serde_json::json!({"host": "evil.example.com"}),
    );
    extra.time = Utc.timestamp_opt(1700000004, 0).unwrap();
    extra = extra.with_subject("evil.example.com:80");
    candidate_events.push(extra);
    let candidate = create_bundle("run_cand", candidate_events);

    let report = diff_bundles(
        Cursor::new(&baseline),
        Cursor::new(&candidate),
        VerifyLimits::default(),
    )
    .unwrap();

    assert_eq!(report.summary.event_count_delta, 1);
    assert!(report
        .network
        .added
        .contains(&"evil.example.com:80".to_string()));
}

fn policy_decision_event(run_id: &str, decision: &str) -> EvidenceEvent {
    let mut event = EvidenceEvent::new(
        "assay.policy.decision",
        "urn:assay:test",
        run_id,
        4,
        serde_json::json!({"tool": "write_file", "decision": decision}),
    );
    event.time = Utc.timestamp_opt(1700000004, 0).unwrap();
    event
}

/// Two bundles whose `.net` / `.fs` / `.process` projections are identical but whose retained
/// events differ in a field none of the projections read (#3037). The projections are empty, the
/// verified `run_root`s differ, and the report must not read as empty.
#[test]
fn test_equal_projections_differing_payload_is_not_empty() {
    let mut baseline_events = base_events("run_base");
    baseline_events.push(policy_decision_event("run_base", "allow"));
    let baseline = create_bundle("run_base", baseline_events);

    let mut candidate_events = base_events("run_cand");
    candidate_events.push(policy_decision_event("run_cand", "deny"));
    let candidate = create_bundle("run_cand", candidate_events);

    let comparison = compare_bundles(
        Cursor::new(&baseline),
        Cursor::new(&candidate),
        VerifyLimits::default(),
    )
    .unwrap();
    let report = &comparison.report;

    assert!(report.is_empty(), "the projection layer alone is equal");
    assert_ne!(report.baseline.run_root, report.candidate.run_root);

    assert!(
        !comparison.is_empty(),
        "run_root differs, so the comparison must not be empty"
    );
    let events = &comparison.retained_events;
    assert!(!events.run_root_equal);
    assert_eq!(events.added.len(), 1);
    assert_eq!(events.removed.len(), 1);
    let added = &events.added[0];
    let removed = &events.removed[0];
    assert_eq!(added.type_, "assay.policy.decision");
    assert_eq!(removed.type_, "assay.policy.decision");
    assert_eq!(added.seq, 4);
    assert_eq!(removed.seq, 4);
    assert_ne!(added.content_hash, removed.content_hash);
    assert!(added.content_hash.starts_with("sha256:"));
}

/// Stream identity (`run_id`, `id`) is not bound by the content id, so two runs that retained the
/// same events must compare equal by content id and by `run_root`.
#[test]
fn test_same_events_different_run_id_is_empty() {
    let baseline = create_bundle("run_base", base_events("run_base"));
    let candidate = create_bundle("run_cand", base_events("run_cand"));

    let comparison = compare_bundles(
        Cursor::new(&baseline),
        Cursor::new(&candidate),
        VerifyLimits::default(),
    )
    .unwrap();

    assert!(comparison.retained_events.run_root_equal);
    assert!(comparison.retained_events.is_empty());
    assert!(comparison.is_empty());
}

#[test]
fn test_corrupt_candidate_fails() {
    let baseline = create_bundle("run_base", base_events("run_base"));
    let result = diff_bundles(
        Cursor::new(&baseline),
        Cursor::new(&[0xDE, 0xAD]),
        VerifyLimits::default(),
    );
    assert!(result.is_err());
}

#[test]
fn test_removed_process_detected() {
    let baseline_events = base_events("run_base");
    let baseline = create_bundle("run_base", baseline_events);

    // Candidate without the process exec event
    let mut candidate_events: Vec<EvidenceEvent> = Vec::new();
    let events = base_events("run_cand");
    for (i, mut ev) in events.into_iter().enumerate() {
        if ev.type_.contains("process.exec") {
            continue; // skip process event
        }
        ev.seq = i as u64;
        ev.id = format!("run_cand:{}", i);
        candidate_events.push(ev);
    }
    let candidate = create_bundle("run_cand", candidate_events);

    let report = diff_bundles(
        Cursor::new(&baseline),
        Cursor::new(&candidate),
        VerifyLimits::default(),
    )
    .unwrap();

    assert!(report.processes.removed.contains(&"curl".to_string()));
}
