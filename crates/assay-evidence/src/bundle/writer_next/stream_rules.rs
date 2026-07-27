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
/// Adding a variant is a deliberate act: it breaks the exhaustive match in the symmetry test until
/// a case proves the verifier rejects a bundle violating it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamRule {
    /// A bundle carries at least one event.
    NonEmpty,
    /// `seq` runs contiguously from 0 in stored order.
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
