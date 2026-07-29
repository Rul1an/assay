//! Two-sided coverage: every break has an accepting twin, so a check that stops firing shows up as
//! a failure rather than as continued green.

use super::*;
use crate::crypto::id::compute_content_hash;
use chrono::{Duration, TimeZone, Utc};

const RUN: &str = "run-liveness";
const H: u64 = 60_000;
const TOL: u64 = 5_000;

fn declaration() -> LivenessDeclaration {
    LivenessDeclaration {
        interval_ms: H,
        tolerance_ms: TOL,
    }
}

/// Build one event at `seq`, `offset_ms` after a fixed epoch, with its content hash filled in the
/// way the writer would.
fn event(type_: &str, seq: u64, offset_ms: i64, payload: serde_json::Value) -> EvidenceEvent {
    let mut e = EvidenceEvent::new(type_, "urn:assay:test", RUN, seq, payload);
    e.time = Utc.timestamp_opt(1_800_000_000, 0).unwrap() + Duration::milliseconds(offset_ms);
    e.producer_version = "test".into();
    e.git_sha = "test".into();
    e.content_hash = Some(compute_content_hash(&e).expect("content hash"));
    e
}

/// A run that opens, beats once inside the declared interval, and closes honestly.
fn healthy_run() -> Vec<EvidenceEvent> {
    let open = event(TYPE_RUN_OPEN, 0, 0, open_payload(&declaration()));
    let beat = event(TYPE_RUN_HEARTBEAT, 1, 10_000, serde_json::json!({}));
    let preceding = vec![open, beat];
    let mut close = event(TYPE_RUN_CLOSE, 2, 20_000, close_payload(&preceding));
    close.content_hash = Some(compute_content_hash(&close).expect("content hash"));
    let mut all = preceding;
    all.push(close);
    all
}

#[test]
fn a_healthy_run_is_complete() {
    assert_eq!(verify_liveness(&healthy_run()), LivenessOutcome::Complete);
}

/// Liveness is opt-in. A bundle that never declared a cadence has not broken one, and reporting a
/// break here would make every pre-existing bundle look defective.
#[test]
fn a_run_without_an_open_record_is_not_declared() {
    let events = vec![event("assay.tool.call", 0, 0, serde_json::json!({}))];
    assert_eq!(verify_liveness(&events), LivenessOutcome::NotDeclared);
}

/// The case this whole module exists for: cut the tail and the remainder still hash-verifies, so
/// only the missing close distinguishes it. `Open` says that without accusing anyone.
#[test]
fn a_truncated_tail_reads_as_open_not_complete_and_not_broken() {
    let mut events = healthy_run();
    events.pop();
    assert_eq!(verify_liveness(&events), LivenessOutcome::Open);
}

#[test]
fn silence_longer_than_the_declared_interval_is_a_break() {
    let open = event(TYPE_RUN_OPEN, 0, 0, open_payload(&declaration()));
    let late = event(
        TYPE_RUN_HEARTBEAT,
        1,
        (H + TOL) as i64 + 1,
        serde_json::json!({}),
    );
    match verify_liveness(&[open, late]) {
        LivenessOutcome::Broken(LivenessBreak::SilenceExceeded {
            after_seq,
            gap_ms,
            allowed_ms,
        }) => {
            assert_eq!(after_seq, 0);
            assert_eq!(allowed_ms, H + TOL);
            assert_eq!(gap_ms, (H + TOL) as i64 + 1);
        }
        other => panic!("expected SilenceExceeded, got {other:?}"),
    }
}

/// The accepting twin: exactly at the allowance is not a break. An off-by-one here would fire on
/// every producer that hits its own deadline precisely.
#[test]
fn silence_exactly_at_the_allowance_is_not_a_break() {
    let open = event(TYPE_RUN_OPEN, 0, 0, open_payload(&declaration()));
    let beat = event(
        TYPE_RUN_HEARTBEAT,
        1,
        (H + TOL) as i64,
        serde_json::json!({}),
    );
    assert_eq!(verify_liveness(&[open, beat]), LivenessOutcome::Open);
}

/// Tolerance has to actually widen the window, or declaring it separately buys nothing.
#[test]
fn a_gap_inside_the_tolerance_but_past_the_interval_is_accepted() {
    let open = event(TYPE_RUN_OPEN, 0, 0, open_payload(&declaration()));
    let beat = event(TYPE_RUN_HEARTBEAT, 1, H as i64 + 1, serde_json::json!({}));
    assert_eq!(verify_liveness(&[open, beat]), LivenessOutcome::Open);
}

#[test]
fn a_sequence_gap_is_a_break() {
    let open = event(TYPE_RUN_OPEN, 0, 0, open_payload(&declaration()));
    let skipped = event(TYPE_RUN_HEARTBEAT, 2, 1_000, serde_json::json!({}));
    assert_eq!(
        verify_liveness(&[open, skipped]),
        LivenessOutcome::Broken(LivenessBreak::SequenceGap {
            expected: 1,
            found: 2
        })
    );
}

/// Removing an interior record is what the count commitment is for: the close still says three.
#[test]
fn a_close_committing_to_more_records_than_are_present_is_a_break() {
    let mut events = healthy_run();
    events.remove(1);
    // Re-number so the sequence stays contiguous; only the count now disagrees, which isolates
    // this check from the sequence-gap one above.
    events[1].seq = 1;
    match verify_liveness(&events) {
        LivenessOutcome::Broken(LivenessBreak::CountMismatch {
            committed,
            presented,
        }) => {
            assert_eq!(committed, 3);
            assert_eq!(presented, 2);
        }
        other => panic!("expected CountMismatch, got {other:?}"),
    }
}

/// Swapping an interior record keeps the count right and the sequence contiguous, so only the
/// chain head can catch it.
#[test]
fn a_close_whose_chain_head_does_not_match_the_records_is_a_break() {
    let mut events = healthy_run();
    events[1] = event(
        TYPE_RUN_HEARTBEAT,
        1,
        10_000,
        serde_json::json!({"swapped": true}),
    );
    match verify_liveness(&events) {
        LivenessOutcome::Broken(LivenessBreak::ChainHeadMismatch { .. }) => {}
        other => panic!("expected ChainHeadMismatch, got {other:?}"),
    }
}

#[test]
fn an_open_record_that_is_not_first_is_a_break() {
    let first = event("assay.tool.call", 0, 0, serde_json::json!({}));
    let open = event(TYPE_RUN_OPEN, 1, 1_000, open_payload(&declaration()));
    assert_eq!(
        verify_liveness(&[first, open]),
        LivenessOutcome::Broken(LivenessBreak::OpenNotFirst { found_at_seq: 1 })
    );
}

#[test]
fn a_close_record_that_is_not_last_is_a_break() {
    let open = event(TYPE_RUN_OPEN, 0, 0, open_payload(&declaration()));
    let close = event(
        TYPE_RUN_CLOSE,
        1,
        1_000,
        close_payload(std::slice::from_ref(&open)),
    );
    let after = event("assay.tool.call", 2, 2_000, serde_json::json!({}));
    assert_eq!(
        verify_liveness(&[open, close, after]),
        LivenessOutcome::Broken(LivenessBreak::CloseNotLast { found_at_seq: 1 })
    );
}

/// A declaration that arrived and could not be read must never fall through to the opt-in
/// `NotDeclared` arm, or a producer could disable every check by emitting a broken open record.
#[test]
fn an_unreadable_open_payload_is_a_break_not_undeclared() {
    let open = event(TYPE_RUN_OPEN, 0, 0, serde_json::json!({"liveness": "60s"}));
    assert_eq!(
        verify_liveness(&[open]),
        LivenessOutcome::Broken(LivenessBreak::MalformedRecord {
            seq: 0,
            type_: TYPE_RUN_OPEN.to_string()
        })
    );
}

/// Same rule on the close side.
#[test]
fn an_unreadable_close_payload_is_a_break_not_open() {
    let open = event(TYPE_RUN_OPEN, 0, 0, open_payload(&declaration()));
    let close = event(
        TYPE_RUN_CLOSE,
        1,
        1_000,
        serde_json::json!({"liveness_close": 3}),
    );
    assert_eq!(
        verify_liveness(&[open, close]),
        LivenessOutcome::Broken(LivenessBreak::MalformedRecord {
            seq: 1,
            type_: TYPE_RUN_CLOSE.to_string()
        })
    );
}

/// The declaration governs the stream it is carried in, so a bundle handed over with a widened
/// interval changes the verdict. That is exactly why it belongs under the signature: this test
/// documents the attack the placement decision defends against.
#[test]
fn widening_the_declared_interval_changes_the_verdict() {
    let late = (H + TOL) as i64 + 1;
    let strict = vec![
        event(TYPE_RUN_OPEN, 0, 0, open_payload(&declaration())),
        event(TYPE_RUN_HEARTBEAT, 1, late, serde_json::json!({})),
    ];
    assert!(matches!(
        verify_liveness(&strict),
        LivenessOutcome::Broken(LivenessBreak::SilenceExceeded { .. })
    ));

    let widened = vec![
        event(
            TYPE_RUN_OPEN,
            0,
            0,
            open_payload(&LivenessDeclaration {
                interval_ms: H * 10,
                tolerance_ms: TOL,
            }),
        ),
        event(TYPE_RUN_HEARTBEAT, 1, late, serde_json::json!({})),
    ];
    assert_eq!(verify_liveness(&widened), LivenessOutcome::Open);
}
