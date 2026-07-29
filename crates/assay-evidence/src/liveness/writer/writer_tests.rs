//! The writer's own tests. The verifier is the oracle throughout: a run this writer produced should
//! read as `Complete`, because a producer and a checker that disagree are worse than either alone.

use super::super::{verify_liveness, LivenessBreak, LivenessDeclaration, LivenessOutcome};
use super::LivenessWriter;
use chrono::{Duration, TimeZone, Utc};
use std::io::{self, Write};

const H: u64 = 60_000;

fn epoch() -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).unwrap()
}

fn declaration() -> LivenessDeclaration {
    LivenessDeclaration {
        interval_ms: H,
        tolerance_ms: 5_000,
    }
}

/// Parse a written NDJSON stream back into events, the way any reader would.
fn parse(sink: &[u8]) -> Vec<crate::types::EvidenceEvent> {
    std::str::from_utf8(sink)
        .expect("utf8")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse record"))
        .collect()
}

fn writer(sink: Vec<u8>) -> LivenessWriter<Vec<u8>> {
    LivenessWriter::open(sink, "run-w", "urn:assay:test", declaration(), epoch()).expect("open run")
}

/// A sink that fails on the Nth write, standing in for a full disk or a read-only mount. The
/// read-only evidence path is not hypothetical: it is the reported failure that motivated the
/// fail-closed choice here.
struct FailingSink {
    writes_before_failure: usize,
    seen: usize,
}

impl Write for FailingSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.seen >= self.writes_before_failure {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "read-only"));
        }
        self.seen += 1;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn an_opened_run_writes_its_open_record_immediately() {
    let w = writer(Vec::new());
    assert_eq!(w.emitted().len(), 1);
    assert_eq!(w.emitted()[0].type_, super::TYPE_RUN_OPEN);
    assert_eq!(w.emitted()[0].seq, 0);
}

/// The round trip that matters: what the writer produces, the verifier calls complete. The records
/// are read back out of the sink bytes rather than from the in-memory list, so this also pins that
/// what was written is what the verifier will parse.
#[test]
fn a_run_this_writer_closes_verifies_as_complete() {
    let mut w = writer(Vec::new());
    w.tick(epoch() + Duration::milliseconds(10_000)).unwrap();
    let sink = w.close(epoch() + Duration::milliseconds(20_000)).unwrap();
    let events = parse(&sink);
    assert_eq!(verify_liveness(&events), LivenessOutcome::Complete);
    assert_eq!(events.last().unwrap().type_, super::TYPE_RUN_CLOSE);
}

/// A tick inside the interval writes nothing, so a caller may poll as often as it likes.
#[test]
fn a_tick_inside_the_interval_emits_nothing() {
    let mut w = writer(Vec::new());
    w.tick(epoch() + Duration::milliseconds(H as i64 - 1))
        .unwrap();
    assert_eq!(w.emitted().len(), 1);
}

/// A long silence produces the beats it owed, not one record papering over the gap. This is what
/// keeps the cadence checkable: the verifier sees intervals, not a single jump.
#[test]
fn a_long_silence_is_filled_with_the_beats_it_owed() {
    let mut w = writer(Vec::new());
    w.tick(epoch() + Duration::milliseconds(H as i64 * 3 + 1))
        .unwrap();
    let beats = w
        .emitted()
        .iter()
        .filter(|e| e.type_ == super::TYPE_RUN_HEARTBEAT)
        .count();
    assert_eq!(beats, 3, "three intervals elapsed, so three beats are owed");
    // And the run it produces holds its own declaration.
    let mut w2 = writer(Vec::new());
    w2.tick(epoch() + Duration::milliseconds(H as i64 * 3 + 1))
        .unwrap();
    assert!(matches!(
        verify_liveness(w2.emitted()),
        LivenessOutcome::Open
    ));
}

/// Sequence numbers are contiguous across every record type, which the verifier requires.
#[test]
fn sequence_numbers_stay_contiguous_across_record_types() {
    let mut w = writer(Vec::new());
    w.tick(epoch() + Duration::milliseconds(H as i64 * 2 + 1))
        .unwrap();
    for (i, e) in w.emitted().iter().enumerate() {
        assert_eq!(e.seq, i as u64);
        assert_eq!(e.id, format!("run-w:{i}"));
    }
}

/// The whole point of the departure from the neighbouring emitters: a failed write surfaces.
#[test]
fn a_sink_that_cannot_be_written_fails_the_open_rather_than_going_quiet() {
    let sink = FailingSink {
        writes_before_failure: 0,
        seen: 0,
    };
    let result = LivenessWriter::open(sink, "run-w", "urn:assay:test", declaration(), epoch());
    assert!(
        result.is_err(),
        "an unwritable evidence sink must not look like a healthy run"
    );
}

/// And on a later record, not just at open: this is the read-only-mount case, where the first write
/// succeeds and a later one does not.
#[test]
fn a_sink_that_fails_mid_run_surfaces_on_the_next_record() {
    let sink = FailingSink {
        writes_before_failure: 2,
        seen: 0,
    };
    let mut w = LivenessWriter::open(sink, "run-w", "urn:assay:test", declaration(), epoch())
        .expect("open succeeds while the sink still accepts writes");
    let err = w.tick(epoch() + Duration::milliseconds(H as i64 + 1));
    assert!(
        err.is_err(),
        "a heartbeat that could not be written is not a heartbeat"
    );
}

/// Dropping without closing leaves the run `Open`. Synthesising a close on drop would make a crash
/// indistinguishable from a clean shutdown, which is the one thing the close record must never do.
#[test]
fn dropping_without_closing_leaves_the_run_open() {
    let mut w = writer(Vec::new());
    w.tick(epoch() + Duration::milliseconds(10_000)).unwrap();
    let events = w.emitted().to_vec();
    drop(w);
    assert_eq!(verify_liveness(&events), LivenessOutcome::Open);
    assert!(!events.iter().any(|e| e.type_ == super::TYPE_RUN_CLOSE));
}

/// A declared interval of zero would spin forever in the backfill loop. It is refused by returning
/// early rather than by panicking, because a nonsensical declaration is a caller's bug and not a
/// reason to take the producer down.
#[test]
fn a_zero_interval_declaration_does_not_spin() {
    let mut w = LivenessWriter::open(
        Vec::new(),
        "run-w",
        "urn:assay:test",
        LivenessDeclaration {
            interval_ms: 0,
            tolerance_ms: 0,
        },
        epoch(),
    )
    .unwrap();
    w.tick(epoch() + Duration::milliseconds(10_000)).unwrap();
    assert_eq!(w.emitted().len(), 1, "no beats, and no hang");
}

/// Nothing here defends against a widened declaration; that is the verifier's business. This test
/// pins that the writer records what it was told, so the two halves cannot silently disagree.
#[test]
fn the_writer_records_the_declaration_it_was_given() {
    let w = writer(Vec::new());
    let declared = w.emitted()[0]
        .payload
        .get("liveness")
        .expect("open record carries its declaration");
    assert_eq!(declared["interval_ms"], H);
    assert_eq!(declared["tolerance_ms"], 5_000);
}

/// A break the writer can still produce, so the verifier is not being fed only happy paths: an
/// interval longer than the declaration plus tolerance, driven by a caller that never ticks.
#[test]
fn a_caller_that_never_ticks_produces_a_detectable_break() {
    let mut w = writer(Vec::new());
    // Emit a record far in the future without letting the writer backfill, by building the run and
    // then checking what the verifier says about a stream missing its beats.
    let mut events = w.emitted().to_vec();
    let mut late = events[0].clone();
    late.seq = 1;
    late.id = "run-w:1".into();
    late.type_ = "assay.tool.call".into();
    late.time = epoch() + Duration::milliseconds(H as i64 * 5);
    late.content_hash = Some(crate::crypto::id::compute_content_hash(&late).unwrap());
    events.push(late);
    assert!(matches!(
        verify_liveness(&events),
        LivenessOutcome::Broken(LivenessBreak::SilenceExceeded { .. })
    ));
    let _ = w.tick(epoch());
}
