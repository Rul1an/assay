//! The conditions a bundle must satisfy to exist at all, stated once for both ends of the format.
//!
//! `BundleWriter::finish` refuses to emit a bundle that violates any of these. Until now the
//! verifier enforced only some of them, so bundles its own writer cannot produce still verified —
//! five of them, four found by looking rather than by report: an empty bundle, an inconsistent
//! `source`, a `source` that is not a URI, and (separately) a line count standing in for an event
//! count.
//!
//! Patching each one leaves the next unwritten. What makes the class closed is that the rules live
//! in one place and both sides read them, and that a rule cannot be added without a test naming it:
//! [`StreamRule`] is matched exhaustively in
//! `crates/assay-evidence/tests/writer_verifier_symmetry.rs`, so a ninth variant will not compile
//! until someone has shown the verifier rejects what the writer refuses.
//!
//! These are *stream* rules — properties of the event sequence itself. Container concerns (member
//! allowlist, path safety, sizes) and manifest concerns (`bundle_id`, `run_root`) are not here;
//! they have no writer-side counterpart to be symmetric with.

use crate::types::EvidenceEvent;

/// Every condition under which the writer refuses to emit a bundle.
///
/// Deliberately **not** `#[non_exhaustive]`, against this crate's usual posture (compare
/// `store::bounded`, which documents the closed-public-enum trap and avoids it). The reason is
/// that exhaustive matching is the point here: a consumer implementing this format from outside
/// should be able to match every rule and be told by the compiler when the format gains one. That
/// benefit is the same mechanism the symmetry test relies on, and it cannot be had behind
/// `#[non_exhaustive]`. The price is real and is accepted with open eyes: a ninth rule is a
/// breaking change for downstream exhaustive matches, and `cargo semver-checks` runs on this
/// crate, so rule nine arrives with a major bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamRule {
    /// A bundle carries at least one event.
    NonEmpty,
    /// `seq` runs contiguously from 0.
    ///
    /// The writer sorts by `seq` before checking, so it refuses a gap but accepts events handed to
    /// it out of order; the verifier sees only stored order and refuses that too. The verifier is
    /// therefore stricter here, which is the safe direction — every bundle the writer emits still
    /// verifies — but the two ends are not testing the identical condition under this one name.
    SeqContiguousFromZero,
    /// Every event carries the same `run_id`.
    RunIdConsistent,
    /// Every event carries the same `source`.
    SourceConsistent,
    /// `source` is a URI: it contains a colon and does not begin with one.
    SourceIsUri,
    /// `run_id` contains no colon, so `run_id:seq` splits unambiguously.
    RunIdHasNoColon,
    /// A stored `content_hash` equals the hash recomputed from the event.
    ContentHashMatchesEvent,
    /// `id` equals `run_id:seq`.
    IdIsRunIdColonSeq,
}

impl StreamRule {
    /// Every rule, as the symmetry test iterates them.
    ///
    /// The or-pattern is exhaustive, so a ninth variant stops this compiling and the arm that
    /// fixes it sits on the line above the list it has to join. That is the strongest guarantee
    /// available without a derive macro, and it is worth being exact about what it is not: it
    /// forces the author to *edit here*, next to the array, rather than proving the array grew.
    /// Naming the variant in the pattern and omitting it from the slice would still compile.
    ///
    /// Nor does anything oblige a new refusal in `BundleWriter::finish` to become a variant at
    /// all — see the note there. Both residuals are the same shape: a list held beside a
    /// behaviour rather than derived from it, which is exactly the defect class these rules
    /// exist to close, surviving one level up in the mechanism that closes it.
    pub const ALL: &'static [StreamRule] = match StreamRule::NonEmpty {
        StreamRule::NonEmpty
        | StreamRule::SeqContiguousFromZero
        | StreamRule::RunIdConsistent
        | StreamRule::SourceConsistent
        | StreamRule::SourceIsUri
        | StreamRule::RunIdHasNoColon
        | StreamRule::ContentHashMatchesEvent
        | StreamRule::IdIsRunIdColonSeq => &[
            StreamRule::NonEmpty,
            StreamRule::SeqContiguousFromZero,
            StreamRule::RunIdConsistent,
            StreamRule::SourceConsistent,
            StreamRule::SourceIsUri,
            StreamRule::RunIdHasNoColon,
            StreamRule::ContentHashMatchesEvent,
            StreamRule::IdIsRunIdColonSeq,
        ],
    };

    /// Why the writer refuses, phrased for a human who has to fix a bundle.
    pub fn describe(self) -> &'static str {
        match self {
            Self::NonEmpty => "a bundle must carry at least one event",
            Self::SeqContiguousFromZero => "event seq must be contiguous from 0",
            Self::RunIdConsistent => "all events must carry the same run_id",
            Self::SourceConsistent => "all events must carry the same source",
            Self::SourceIsUri => "source must be a URI (e.g. urn:assay:..., https://...)",
            Self::RunIdHasNoColon => "run_id cannot contain colons",
            Self::ContentHashMatchesEvent => "a stored content_hash must match the event",
            Self::IdIsRunIdColonSeq => "id must equal run_id:seq",
        }
    }
}

/// `source` is a URI for this format's purposes.
///
/// Deliberately not a full RFC 3986 parse: the writer has always applied exactly this test, and a
/// verifier that applied a stricter one would reject bundles the writer emits — the same asymmetry
/// in the other direction. Widening or narrowing it is a format change, not an implementation
/// detail, and belongs in both ends at once.
pub fn source_is_uri(source: &str) -> bool {
    source.contains(':') && !source.starts_with(':')
}

/// `run_id` keeps `run_id:seq` unambiguous.
pub fn run_id_has_no_colon(run_id: &str) -> bool {
    !run_id.contains(':')
}

/// The rules that hold across the whole stream, checked against a parsed event and the first event.
///
/// Returns the first rule violated, or `None`. Per-event rules that the verifier already enforced
/// separately (`seq`, `content_hash`, `id`) stay where they are so their error classification does
/// not change; this covers the ones that had no verifier-side counterpart.
pub fn violated_by_event(event: &EvidenceEvent, first: &EvidenceEvent) -> Option<StreamRule> {
    if event.run_id != first.run_id {
        return Some(StreamRule::RunIdConsistent);
    }
    if event.source != first.source {
        return Some(StreamRule::SourceConsistent);
    }
    if !source_is_uri(&event.source) {
        return Some(StreamRule::SourceIsUri);
    }
    if !run_id_has_no_colon(&event.run_id) {
        return Some(StreamRule::RunIdHasNoColon);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_uri_test_matches_the_writers_historical_rule() {
        assert!(source_is_uri("urn:assay:x"));
        assert!(source_is_uri("https://example.test/x"));
        assert!(!source_is_uri("not-a-uri"));
        assert!(!source_is_uri(":leading"));
        // A colon anywhere else is enough, which is the writer's rule and therefore the format's.
        assert!(source_is_uri("a:b"));
    }

    #[test]
    fn run_id_colon_rule_is_exact() {
        assert!(run_id_has_no_colon("run_abc"));
        assert!(!run_id_has_no_colon("run:abc"));
        assert!(!run_id_has_no_colon(":"));
    }
}
