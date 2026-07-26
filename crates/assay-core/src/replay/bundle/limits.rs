//! Replay-bundle-local resource ceilings and their typed refusals.
//!
//! ADR-043 §1 requires every ingest entrypoint to apply the whole limit set to the source
//! stream before the input is materialized. The evidence verifier already does this through
//! its own [`VerifyLimits`], but replay used to have no ceilings at all: `read_bundle_tar_gz`
//! read every entry with an unbounded `read_to_end`, on top of an unbounded gzip decoder, on
//! top of an unbounded compressed source.
//!
//! The mechanism moves through the shared [`assay_common::limits::LimitReader`] primitive.
//! The *vocabulary* stays local: a replay bundle is not an evidence bundle, its members are
//! not events or manifests in the evidence sense, and dragging `VerifyLimits` into
//! `assay-core` would tell downstream readers that the two contracts agree when they do not.
//! Callers who really want to share a ceiling still can, by initialising both structs from
//! the same numbers.
//!
//! Refusals travel as [`ReplayIngestError`]. The typed cause on the underlying `io::Error`
//! is a [`LimitExceeded`], recovered through `LimitExceeded::from_io`; the enum wraps that
//! with the semantic context the reader had at the time (which member, which path). Callers
//! match the enum, not the rendered text.

use assay_common::limits::{LimitExceeded, LimitKind};

/// Resource ceilings applied while reading a replay bundle.
///
/// Numbers deliberately match the evidence defaults where the two contracts describe the same
/// resource (compressed source and gzip expansion); the per-member and entry-count values
/// come from what a replay bundle actually contains: a manifest, a handful of small files,
/// and cassettes that fit under `max_member_bytes`.
#[derive(Debug, Clone, Copy)]
pub struct ReplayLimits {
    /// Maximum bytes read from the compressed source.
    pub max_source_bytes: u64,
    /// Maximum bytes produced by the gzip decoder.
    pub max_decoded_bytes: u64,
    /// Maximum bytes of `manifest.json`.
    pub max_manifest_bytes: u64,
    /// Maximum bytes of any single non-manifest member.
    pub max_member_bytes: u64,
    /// Maximum length in bytes of any entry path.
    pub max_path_len: usize,
    /// Maximum number of entries in the archive (including the manifest).
    pub max_entries: usize,
    /// Maximum JSON nesting depth accepted in `manifest.json`.
    ///
    /// A ceiling on manifest *bytes* says nothing about its shape: a small document can nest
    /// deeply enough to exhaust the stack in the parser. The evidence side carries the same
    /// dimension as `max_json_depth`; replay names it locally for the same reason the rest of
    /// this struct is local.
    pub max_manifest_json_depth: usize,
}

impl Default for ReplayLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 100 * 1024 * 1024,   // 100 MiB compressed
            max_decoded_bytes: 1024 * 1024 * 1024, // 1 GiB expanded
            max_manifest_bytes: 10 * 1024 * 1024,  // 10 MiB
            max_member_bytes: 500 * 1024 * 1024,   // 500 MiB per file (large cassettes)
            max_path_len: 256,
            max_entries: 100_000,
            max_manifest_json_depth: 64,
        }
    }
}

/// Refusal raised by the bounded replay reader. Callers match the variant; the rendered
/// message is a diagnostic, never a contract.
#[derive(Debug, thiserror::Error)]
pub enum ReplayIngestError {
    #[error("replay bundle exceeded {kind} limit of {limit}")]
    SourceCeiling { kind: LimitKind, limit: u64 },

    /// Value-free like the others: the member name comes from the archive, so echoing it hands
    /// an attacker a channel into the diagnostic while telling the reader nothing they can act
    /// on. The dimension and the ceiling are what distinguishes this from a source refusal.
    #[error("replay bundle member exceeded {kind} limit of {limit}")]
    MemberCeiling { kind: LimitKind, limit: u64 },

    /// Deliberately free of the offending value. The path and its length are chosen by the
    /// archive, and echoing either back gives a reader nothing to act on while handing an
    /// attacker a channel into the diagnostic.
    #[error("replay bundle entry path exceeds the configured maximum length of {limit}")]
    PathTooLong { limit: usize },

    #[error("replay bundle entry count exceeds limit {limit}")]
    TooManyEntries { limit: usize },

    #[error("replay bundle manifest JSON nesting exceeds the configured maximum depth of {limit}")]
    ManifestTooDeep { limit: usize },
}

/// If `err` was produced by a [`LimitReader`](assay_common::limits::LimitReader) that wraps
/// the compressed source or gzip stream, promote it to a `ReplayIngestError::SourceCeiling`.
/// Otherwise return `None` so the caller can keep its own classification.
pub(crate) fn classify_source_ceiling(err: &std::io::Error) -> Option<ReplayIngestError> {
    let cause = LimitExceeded::from_io(err)?;
    Some(ReplayIngestError::SourceCeiling {
        kind: cause.kind,
        limit: cause.limit,
    })
}

/// Same as [`classify_source_ceiling`] but for reads scoped to a single member (the manifest
/// or an entry body).
/// A member read sits on top of the decoder, so an overflow surfacing here is not necessarily a
/// member overflow: the expansion ceiling trips through the same call. Only `MemberBytes` is a
/// member refusal; every other dimension keeps its own classification, otherwise a decode ceiling
/// is reported as if a single file were too large.
pub(crate) fn classify_member_ceiling(err: &std::io::Error) -> Option<ReplayIngestError> {
    let cause = LimitExceeded::from_io(err)?;
    Some(match cause.kind {
        LimitKind::MemberBytes => ReplayIngestError::MemberCeiling {
            kind: cause.kind,
            limit: cause.limit,
        },
        other => ReplayIngestError::SourceCeiling {
            kind: other,
            limit: cause.limit,
        },
    })
}

/// Refuse a manifest whose JSON nests deeper than the configured ceiling.
///
/// Counts structural depth over the raw bytes rather than parsing first: handing an unbounded
/// document to `serde_json` is the thing the ceiling exists to prevent, so the check cannot
/// depend on the parse succeeding.
pub(crate) fn check_manifest_json_depth(
    data: &[u8],
    max_depth: usize,
) -> Result<(), ReplayIngestError> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for &b in data {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > max_depth {
                    return Err(ReplayIngestError::ManifestTooDeep { limit: max_depth });
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_common::limits::{LimitExceeded, LimitKind};

    fn make_io(kind: LimitKind, limit: u64) -> std::io::Error {
        std::io::Error::other(LimitExceeded { kind, limit })
    }

    /// Classification goes through the typed cause, not the message. If it went through the
    /// message, a reworded diagnostic on `LimitExceeded` would silently reclassify.
    #[test]
    fn source_ceiling_is_recovered_from_the_typed_cause() {
        let io = make_io(LimitKind::DecodedBytes, 1024);
        match classify_source_ceiling(&io) {
            Some(ReplayIngestError::SourceCeiling { kind, limit }) => {
                assert_eq!(kind, LimitKind::DecodedBytes);
                assert_eq!(limit, 1024);
            }
            other => panic!("expected SourceCeiling, got {other:?}"),
        }
    }

    /// The dimension and the ceiling travel; the member name does not. It is archive-controlled
    /// and adds nothing a reader can act on.
    #[test]
    fn member_ceiling_carries_the_dimension_and_not_the_member_name() {
        let io = make_io(LimitKind::MemberBytes, 42);
        match classify_member_ceiling(&io) {
            Some(ReplayIngestError::MemberCeiling { kind, limit }) => {
                assert_eq!(kind, LimitKind::MemberBytes);
                assert_eq!(limit, 42);
            }
            other => panic!("expected MemberCeiling, got {other:?}"),
        }
        let rendered = ReplayIngestError::MemberCeiling {
            kind: LimitKind::MemberBytes,
            limit: 42,
        }
        .to_string();
        assert!(
            !rendered.contains('/'),
            "no archive path may appear: {rendered}"
        );
    }

    #[test]
    fn a_non_ceiling_io_error_is_not_promoted() {
        let io = std::io::Error::other("something else entirely");
        assert!(classify_source_ceiling(&io).is_none());
        assert!(classify_member_ceiling(&io).is_none());
    }
}
