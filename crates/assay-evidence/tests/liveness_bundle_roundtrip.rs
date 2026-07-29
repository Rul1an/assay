//! Liveness records survive a real bundle round-trip, and cost the format nothing.
//!
//! The unit tests in `liveness::tests` check the verifier against hand-built event vectors. What
//! they cannot show is the claim the module's documentation actually makes: that these are ordinary
//! events, so a bundle carrying them is still just a bundle. That claim is only worth anything if
//! it goes through the real writer and the real verifying reader, which is what this file does.

use assay_evidence::bundle::{BundleReader, BundleWriter};
use assay_evidence::liveness::{
    close_payload, open_payload, verify_liveness, LivenessDeclaration, LivenessOutcome,
    TYPE_RUN_CLOSE, TYPE_RUN_HEARTBEAT, TYPE_RUN_OPEN,
};
use assay_evidence::types::EvidenceEvent;
use chrono::{Duration, TimeZone, Utc};

const RUN: &str = "run-liveness-roundtrip";

fn declaration() -> LivenessDeclaration {
    LivenessDeclaration {
        interval_ms: 60_000,
        tolerance_ms: 5_000,
    }
}

fn event(type_: &str, seq: u64, offset_ms: i64, payload: serde_json::Value) -> EvidenceEvent {
    let mut e = EvidenceEvent::new(type_, "urn:assay:test", RUN, seq, payload);
    e.time = Utc.timestamp_opt(1_800_000_000, 0).unwrap() + Duration::milliseconds(offset_ms);
    e
}

/// Open, one heartbeat, close. Content hashes are left for the writer to normalise, which is the
/// point: the close commitment has to be computed over the same hashes the writer will store, so
/// this also pins that `close_payload` agrees with the writer's normalisation.
fn liveness_run() -> Vec<EvidenceEvent> {
    let mut open = event(TYPE_RUN_OPEN, 0, 0, open_payload(&declaration()));
    let mut beat = event(TYPE_RUN_HEARTBEAT, 1, 10_000, serde_json::json!({}));
    for e in [&mut open, &mut beat] {
        e.content_hash = Some(assay_evidence::crypto::id::compute_content_hash(e).unwrap());
    }
    let preceding = vec![open, beat];
    let close = event(TYPE_RUN_CLOSE, 2, 20_000, close_payload(&preceding));
    let mut all = preceding;
    all.push(close);
    all
}

fn write_bundle(events: Vec<EvidenceEvent>) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut writer = BundleWriter::new(&mut buf);
        for e in events {
            writer.add_event(e);
        }
        writer.finish().expect("write bundle");
    }
    buf
}

/// The whole additivity claim in one test: a bundle carrying liveness records opens under the
/// ordinary verifying reader, with no special mode and no format flag.
#[test]
fn a_bundle_carrying_liveness_records_verifies_as_an_ordinary_bundle() {
    let bytes = write_bundle(liveness_run());
    let reader = BundleReader::open(bytes.as_slice()).expect("bundle verifies");
    assert_eq!(reader.manifest().event_count, 3);
    let events = reader.events_vec().expect("read events");
    assert_eq!(verify_liveness(&events), LivenessOutcome::Complete);
}

/// A bundle with no liveness records verifies exactly as before and reports `NotDeclared`, so
/// adding this module does not retroactively mark every existing bundle as defective.
#[test]
fn an_ordinary_bundle_is_unaffected() {
    let bytes = write_bundle(vec![event(
        "assay.tool.call",
        0,
        0,
        serde_json::json!({"tool": "read"}),
    )]);
    let reader = BundleReader::open(bytes.as_slice()).expect("bundle verifies");
    let events = reader.events_vec().expect("read events");
    assert_eq!(verify_liveness(&events), LivenessOutcome::NotDeclared);
}

/// Tail truncation is the case AEL-1 exists for. Dropping the close still produces a bundle that
/// verifies on every integrity property it has, which is precisely why the liveness answer has to
/// come from somewhere else. `Open` is that answer.
#[test]
fn a_truncated_bundle_still_verifies_but_no_longer_reads_as_complete() {
    let mut events = liveness_run();
    events.pop();
    let bytes = write_bundle(events);

    let reader = BundleReader::open(bytes.as_slice()).expect("truncated bundle still verifies");
    assert_eq!(
        reader.manifest().event_count,
        2,
        "the manifest honestly describes the shorter run, which is the problem"
    );
    assert_eq!(
        verify_liveness(&reader.events_vec().expect("read events")),
        LivenessOutcome::Open,
        "integrity alone cannot see the missing tail; liveness can"
    );
}
