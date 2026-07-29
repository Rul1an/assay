//! Run liveness: making silence and truncation into statements rather than gaps.
//!
//! A hash-chained, signed event stream makes interior edits detectable. It does not make
//! *absence* detectable: cut the stream at any point and the remainder still verifies, and a
//! producer that stops emitting looks exactly like a producer with nothing to say. Both are the
//! same defect from the reader's side, which is that nothing in the artifact distinguishes a quiet
//! interval from a missing one.
//!
//! Three records close that gap, none of which change the bundle format:
//!
//! - [`TYPE_RUN_OPEN`] carries the run identity and, crucially, the *declared* maximum interval
//!   `H`. A cadence the artifact does not state is a cadence a verifier cannot check, so the
//!   declaration has to travel with the evidence rather than live in a verifier's configuration.
//! - [`TYPE_RUN_HEARTBEAT`] is emitted so that no two consecutive records are further apart than
//!   `H`. That is what turns an interval without activity into a signed statement: silence becomes
//!   something the producer had to actively assert.
//! - [`TYPE_RUN_CLOSE`] commits to how many records the run contained and to the chain head over
//!   everything before it, so removing the tail is a contradiction rather than a shorter run.
//!
//! # Why these live in events and not in the manifest
//!
//! The signed subject of an Assay attestation is the run root, which is computed over event content
//! hashes only ([`crate::crypto::id::compute_run_root`]). The manifest is a table of contents and
//! is not itself the signed payload. A declared `H` sitting in the manifest would therefore be an
//! unsigned number governing a signed stream, and an attacker who can hand you a bundle can hand
//! you a larger `H`. Putting the declaration in the run-open record puts it under the same
//! commitment as the records whose spacing it governs. It also means this module adds no field to
//! any existing structure: these are ordinary events with reserved type strings.
//!
//! # What this does not do
//!
//! Nothing here defends against a dishonest key holder. Someone who can sign can fabricate an
//! entire run, heartbeats and close included, and it will verify end to end. This makes omission
//! *by a truncating intermediary* detectable, which is a different and weaker claim than
//! completeness. It is also per-run: deleting a whole run remains silent, because a run cannot
//! attest to its own existence.

use crate::crypto::id::compute_run_root;
use crate::types::EvidenceEvent;
use serde::{Deserialize, Serialize};

/// Reserved event type for the record that opens a run and declares its cadence.
pub const TYPE_RUN_OPEN: &str = "assay.run.open";

/// Reserved event type for a record whose only purpose is to assert that the producer was alive.
pub const TYPE_RUN_HEARTBEAT: &str = "assay.run.heartbeat";

/// Reserved event type for the record that closes a run and commits to its extent.
pub const TYPE_RUN_CLOSE: &str = "assay.run.close";

/// The cadence a run declares for itself, carried in the `data` of its run-open record.
///
/// `tolerance_ms` is separate from `interval_ms` on purpose. Scheduler jitter, GC pauses and clock
/// skew all widen real intervals slightly, and folding that slack into `H` itself would overstate
/// the guarantee: a reader could no longer tell how tight the cadence actually was from how much
/// slop the producer needed. Declaring them apart keeps the promise and the allowance legible as
/// two numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LivenessDeclaration {
    /// The maximum gap the producer promises between consecutive records, in milliseconds.
    pub interval_ms: u64,
    /// Slack a verifier should allow on top of `interval_ms` before calling a gap a break.
    pub tolerance_ms: u64,
}

/// What a run-close record commits to.
///
/// `record_count` counts every record in the run *including the close itself*, so the number is
/// checkable against what a reader is holding without knowing whether to add one. `chain_head` is
/// the run root over every record *before* the close, since a record cannot commit to a value
/// computed over itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseCommitment {
    /// Total records in the run, counting this close record.
    pub record_count: u64,
    /// Run root over every record preceding this one.
    pub chain_head: String,
}

/// How a run ended, as a first-class answer rather than a boolean.
///
/// `Open` is deliberately not a failure. A run that is still in progress, and a producer that was
/// killed, are both real states that an honest artifact can be in, and collapsing them into "fail"
/// would tell a reader that something is wrong with the evidence when what is actually true is that
/// the run did not finish. The distinction matters most for the case it is easiest to get wrong:
/// tail truncation by an intermediary produces exactly this shape, so `Open` is the outcome that
/// says "you may be holding less than was produced" without asserting that anyone misbehaved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LivenessOutcome {
    /// Opened, cadence held, closed, and the close agrees with what is present.
    Complete,
    /// No close record. Either still running, ended abnormally, or truncated; the artifact alone
    /// cannot distinguish these, and this outcome says so instead of guessing.
    Open,
    /// A declared property did not hold.
    Broken(LivenessBreak),
    /// The run carries no run-open record, so it never declared a cadence and none of this applies.
    /// Not a defect: liveness is opt-in, and a bundle that never promised is not a bundle that
    /// broke a promise.
    NotDeclared,
}

/// The specific way a declared run failed its own declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LivenessBreak {
    /// Sequence numbers are not contiguous from zero.
    SequenceGap { expected: u64, found: u64 },
    /// Two consecutive records are further apart than the declared interval plus tolerance.
    SilenceExceeded {
        after_seq: u64,
        gap_ms: i64,
        allowed_ms: u64,
    },
    /// A run-open record exists but is not the first record.
    OpenNotFirst { found_at_seq: u64 },
    /// A run-close record exists but is not the last record.
    CloseNotLast { found_at_seq: u64 },
    /// The close commits to a different number of records than the artifact presents.
    CountMismatch { committed: u64, presented: u64 },
    /// The close commits to a chain head that does not match the records preceding it.
    ChainHeadMismatch { committed: String, computed: String },
    /// A reserved record is present but its payload cannot be read as the declaration it claims.
    /// Distinct from absence: a signal that arrived and failed is a different finding from silence,
    /// and must never be folded into the opt-in `NotDeclared` arm.
    MalformedRecord { seq: u64, type_: String },
}

/// Build the `data` payload for a run-open record.
pub fn open_payload(declaration: &LivenessDeclaration) -> serde_json::Value {
    serde_json::json!({ "liveness": declaration })
}

/// Build the `data` payload for a run-close record.
///
/// `preceding` must be every record of the run in sequence order, excluding the close itself.
pub fn close_payload(preceding: &[EvidenceEvent]) -> serde_json::Value {
    let hashes: Vec<String> = preceding
        .iter()
        .map(|e| e.content_hash.clone().unwrap_or_default())
        .collect();
    let commitment = CloseCommitment {
        // +1 for the close record this payload belongs to.
        record_count: preceding.len() as u64 + 1,
        chain_head: compute_run_root(&hashes),
    };
    serde_json::json!({ "liveness_close": commitment })
}

fn read_declaration(event: &EvidenceEvent) -> Option<LivenessDeclaration> {
    serde_json::from_value(event.payload.get("liveness")?.clone()).ok()
}

fn read_commitment(event: &EvidenceEvent) -> Option<CloseCommitment> {
    serde_json::from_value(event.payload.get("liveness_close")?.clone()).ok()
}

/// Check a run against the cadence it declared for itself.
///
/// Events are expected in sequence order. The checks run in a deliberate order: structure before
/// cadence, and cadence before the close commitment. An out-of-place open record makes every later
/// question meaningless, and reporting a silence break on a stream whose ordering is already wrong
/// would name the wrong defect.
pub fn verify_liveness(events: &[EvidenceEvent]) -> LivenessOutcome {
    let Some(open) = events.iter().find(|e| e.type_ == TYPE_RUN_OPEN) else {
        return LivenessOutcome::NotDeclared;
    };
    if open.seq != 0 {
        return LivenessOutcome::Broken(LivenessBreak::OpenNotFirst {
            found_at_seq: open.seq,
        });
    }
    let Some(declaration) = read_declaration(open) else {
        return LivenessOutcome::Broken(LivenessBreak::MalformedRecord {
            seq: open.seq,
            type_: TYPE_RUN_OPEN.to_string(),
        });
    };

    for (index, event) in events.iter().enumerate() {
        if event.seq != index as u64 {
            return LivenessOutcome::Broken(LivenessBreak::SequenceGap {
                expected: index as u64,
                found: event.seq,
            });
        }
    }

    let allowed_ms = declaration
        .interval_ms
        .saturating_add(declaration.tolerance_ms);
    for pair in events.windows(2) {
        let gap_ms = (pair[1].time - pair[0].time).num_milliseconds();
        if gap_ms > allowed_ms as i64 {
            return LivenessOutcome::Broken(LivenessBreak::SilenceExceeded {
                after_seq: pair[0].seq,
                gap_ms,
                allowed_ms,
            });
        }
    }

    let Some(close_index) = events.iter().position(|e| e.type_ == TYPE_RUN_CLOSE) else {
        // Every cadence check above already passed, so the records present are internally
        // consistent. What is missing is any statement about where the run ended.
        return LivenessOutcome::Open;
    };
    if close_index != events.len() - 1 {
        return LivenessOutcome::Broken(LivenessBreak::CloseNotLast {
            found_at_seq: events[close_index].seq,
        });
    }
    let close = &events[close_index];
    let Some(commitment) = read_commitment(close) else {
        return LivenessOutcome::Broken(LivenessBreak::MalformedRecord {
            seq: close.seq,
            type_: TYPE_RUN_CLOSE.to_string(),
        });
    };

    let presented = events.len() as u64;
    if commitment.record_count != presented {
        return LivenessOutcome::Broken(LivenessBreak::CountMismatch {
            committed: commitment.record_count,
            presented,
        });
    }
    let hashes: Vec<String> = events[..close_index]
        .iter()
        .map(|e| e.content_hash.clone().unwrap_or_default())
        .collect();
    let computed = compute_run_root(&hashes);
    if commitment.chain_head != computed {
        return LivenessOutcome::Broken(LivenessBreak::ChainHeadMismatch {
            committed: commitment.chain_head,
            computed,
        });
    }

    LivenessOutcome::Complete
}

pub mod writer;
pub use writer::LivenessWriter;

#[cfg(test)]
mod tests;
