//! Protocol-era observation and resolution for imported MCP transcripts.
//!
//! Everything here is `pub(crate)`. The ledger calls this internal normalizer vocabulary, and it
//! stays that way until a public normalizer API is deliberately designed: `McpEvent` is a public,
//! constructible, non-`#[non_exhaustive]` struct, so adding fields to it would break downstream
//! struct literals and exhaustive destructuring for a shape that is still being worked out. The
//! era travels in a sidecar instead, and `parse_mcp_transcript` keeps returning `Vec<McpEvent>`.
//!
//! Three axes, deliberately not one. `EnvelopeObservation` is what the transport framing carried.
//! `EraResolution` is what the era turned out to be once every signal was read. `ResultObservation`
//! is what a response said about its own completeness. A format with no transport envelope has not
//! lost the era, because the 2026 request carries its own required
//! `_meta["io.modelcontextprotocol/protocolVersion"]`, and a header that disagrees with that field
//! is a third fact with the spec's own MUST behind it.

use super::types::McpEvent;

/// The protocol version that introduced `resultType` and the required request metadata. Compared
/// against, never keyed on: the rule is that no field is named after an era, not that no boundary
/// may be stated.
pub(crate) const RESULT_TYPE_SINCE: &str = "2026-07-28";

/// What the transport framing carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnvelopeObservation {
    /// The input format has no transport envelope, so there is nothing to be missing. Bare
    /// JSON-RPC and Inspector exports land here: neither reaches `parse_transport_transcript`.
    /// This describes Assay's current adapters, not the formats in principle.
    NotApplicable,
    Absent,
    Present(String),
    /// The framing carried something in the header slot that could not be read as a version:
    /// a number, an object, an empty string. Distinct from `Absent`, which is silence, because a
    /// signal that arrived and failed is a different finding with a different remediation.
    Malformed,
}

/// What a request carried in `params._meta`. Typed rather than `Option<String>` so that a missing
/// field and an unreadable one stay different findings: one is silence, the other is a malformed
/// signal, and they have different remediations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RequestMetadata {
    Absent,
    Present(String),
    /// Present but not a string, or not a usable version.
    Malformed,
}

/// Why an era could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnknownReason {
    NoSignal,
    /// A well-formed version this build does not know how to read.
    UnsupportedVersion(String),
    MalformedSignal,
}

/// What the era turned out to be once every available signal was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EraResolution {
    Known(String),
    Unknown(UnknownReason),
    /// Header and request metadata disagree. For the HTTP transport the metadata value MUST match
    /// the `MCP-Protocol-Version` header, and a server MUST otherwise answer 400, so this is a
    /// defined fault rather than something to resolve toward either side.
    Conflicting {
        header: String,
        body: String,
    },
}

/// What a response said about its own completeness, before any era was applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResultObservation {
    Missing,
    Complete,
    InputRequired,
    /// `ResultType` is an open union and the handling rule is a SHOULD, so conforming verifiers
    /// may legitimately differ here.
    Unrecognized(String),
    /// The field is present and is not a token at all: a number, an object, an empty string.
    /// Distinct from `Missing`, and the distinction is load-bearing on the legacy arm, where an
    /// absent field MUST be read as `"complete"` and an unreadable one must not inherit that.
    Malformed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
// Targeted rather than module-wide, so a future dead item in this file is still reported.
// `expect` would be stricter but is unfulfilled here: the tests use these under `--all-targets`,
// so the lint does not fire and `expect` itself errors. Removed by the slice-2 conclusion layer.
#[allow(dead_code)]
pub(crate) enum IncompleteReason {
    EraUnknown(UnknownReason),
    UnrecognizedResultType(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
// Targeted rather than module-wide, so a future dead item in this file is still reported.
// `expect` would be stricter but is unfulfilled here: the tests use these under `--all-targets`,
// so the lint does not fire and `expect` itself errors. Removed by the slice-2 conclusion layer.
#[allow(dead_code)]
pub(crate) enum InvalidReason {
    EraConflicting {
        header: String,
        body: String,
    },
    MissingResultType,
    MissingRequestMetadata,
    MalformedRequestMetadata,
    /// A version signal arrived and could not be read. Invalid rather than incomplete: more
    /// evidence does not make an unreadable value readable.
    MalformedEraSignal,
    /// `resultType` is present and unreadable. Never folded into the absent-means-complete rule.
    MalformedResultType,
}

/// What a request is worth on its own terms. A request has no result, so `NonTerminal` would be a
/// category error here: "no objection" and "valid but unfinished" are different answers and only
/// the second belongs to a result.
#[derive(Debug, Clone, PartialEq, Eq)]
// Targeted rather than module-wide, so a future dead item in this file is still reported.
// `expect` would be stricter but is unfulfilled here: the tests use these under `--all-targets`,
// so the lint does not fire and `expect` itself errors. Removed by the slice-2 conclusion layer.
#[allow(dead_code)]
pub(crate) enum RequestAssessment {
    Valid,
    Incomplete(IncompleteReason),
    Invalid(InvalidReason),
}

/// What a response licenses under a resolved era. Deliberately not a boolean: unknown,
/// contradicted, and a missing required field are three different findings, and `input_required`
/// is valid while not being terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
// Targeted rather than module-wide, so a future dead item in this file is still reported.
// `expect` would be stricter but is unfulfilled here: the tests use these under `--all-targets`,
// so the lint does not fire and `expect` itself errors. Removed by the slice-2 conclusion layer.
#[allow(dead_code)]
pub(crate) enum ResultConclusion {
    Terminal,
    NonTerminal,
    Incomplete(IncompleteReason),
    Invalid(InvalidReason),
}

/// The era sidecar for one parsed event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct McpEraContext {
    pub(crate) envelope: EnvelopeObservation,
    pub(crate) era: EraResolution,
    pub(crate) request_metadata: Option<RequestMetadata>,
    pub(crate) result_observation: Option<ResultObservation>,
}

/// One event and what was observed about its era, kept apart so the public event shape is
/// unchanged.
#[derive(Debug, Clone)]
pub(crate) struct ParsedMcpEvent {
    pub(crate) event: McpEvent,
    /// Read by tests and by the slice-2 conclusion layer; nothing in production consumes it yet.
    #[allow(dead_code)]
    pub(crate) context: McpEraContext,
}

/// The protocol versions this build has era rules for, newest last. This is a statement about
/// what the reader knows, not about which versions exist: a well-formed version outside this set
/// is `UnsupportedVersion`, which is a gap in the reader rather than a fault in the record.
/// Enumerated from the specification repository's `schema/` directory.
pub(crate) const SUPPORTED_VERSIONS: &[&str] = &[
    "2024-11-05",
    "2025-03-26",
    "2025-06-18",
    "2025-11-25",
    RESULT_TYPE_SINCE,
];

/// Whether a string is a usable protocol version, which is an ISO calendar date.
///
/// Ordering against [`RESULT_TYPE_SINCE`] is lexicographic, and lexicographic ordering is date
/// ordering only once the value is a date. Ten bytes and two dashes is not enough: `2026-99-99`
/// would pass a shape-only check and be reported as a version this build merely does not support,
/// which blames the reader for a record that is wrong. Validated as a real calendar date, leap
/// years included, so `2026-02-31` is malformed rather than unsupported.
fn is_version_shaped(v: &str) -> bool {
    let b = v.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if !b
        .iter()
        .enumerate()
        .all(|(i, c)| matches!(i, 4 | 7) || c.is_ascii_digit())
    {
        return false;
    }
    let num = |from: usize, to: usize| v[from..to].parse::<u32>().unwrap_or(0);
    let (year, month, day) = (num(0, 4), num(5, 7), num(8, 10));
    if !matches!(month, 1..=12) || day == 0 {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if leap => 29,
        _ => 28,
    };
    day <= days
}

/// Classify a version already known to be shaped like one.
fn classify(version: &str) -> EraResolution {
    if SUPPORTED_VERSIONS.contains(&version) {
        EraResolution::Known(version.to_string())
    } else {
        EraResolution::Unknown(UnknownReason::UnsupportedVersion(version.to_string()))
    }
}

/// Resolve the era from the two signals a request can carry.
///
/// The order is the rule, and an earlier version of this function documented an order it did not
/// implement. Unreadable is settled first, because two values that are not both versions cannot be
/// said to disagree about a version: comparing them would report a contradiction where the real
/// finding is that one of them is not a version at all. Only then is a disagreement settled, and
/// only then is a single surviving value classified.
pub(crate) fn resolve_era(
    envelope: &EnvelopeObservation,
    metadata: &RequestMetadata,
) -> EraResolution {
    let malformed = EraResolution::Unknown(UnknownReason::MalformedSignal);
    let header = match envelope {
        EnvelopeObservation::Present(v) => Some(v.as_str()),
        EnvelopeObservation::Absent | EnvelopeObservation::NotApplicable => None,
        EnvelopeObservation::Malformed => return malformed,
    };
    let body = match metadata {
        RequestMetadata::Present(v) => Some(v.as_str()),
        RequestMetadata::Absent => None,
        RequestMetadata::Malformed => return malformed,
    };
    if [header, body]
        .into_iter()
        .flatten()
        .any(|v| !is_version_shaped(v))
    {
        return malformed;
    }
    match (header, body) {
        (Some(h), Some(b)) if h != b => EraResolution::Conflicting {
            header: h.to_string(),
            body: b.to_string(),
        },
        (Some(v), _) | (None, Some(v)) => classify(v),
        (None, None) => EraResolution::Unknown(UnknownReason::NoSignal),
    }
}

/// Whether a resolved version is one where `resultType` and the request metadata are required.
fn requires_result_type(version: &str) -> bool {
    version >= RESULT_TYPE_SINCE
}

/// Read a response observation under a resolved era.
///
/// The two MUSTs sit next to each other in the schema: a server implementing this version MUST
/// include `resultType`, and a client receiving a result from an earlier-version server MUST treat
/// the absent field as `"complete"`. Same observation, opposite conclusions.
// Targeted rather than module-wide, so a future dead item in this file is still reported.
// `expect` would be stricter but is unfulfilled here: the tests use these under `--all-targets`,
// so the lint does not fire and `expect` itself errors. Removed by the slice-2 conclusion layer.
#[allow(dead_code)]
pub(crate) fn conclude(era: &EraResolution, observed: &ResultObservation) -> ResultConclusion {
    let version = match era {
        EraResolution::Known(v) => v,
        EraResolution::Unknown(UnknownReason::MalformedSignal) => {
            return ResultConclusion::Invalid(InvalidReason::MalformedEraSignal)
        }
        EraResolution::Unknown(reason) => {
            return ResultConclusion::Incomplete(IncompleteReason::EraUnknown(reason.clone()))
        }
        EraResolution::Conflicting { header, body } => {
            return ResultConclusion::Invalid(InvalidReason::EraConflicting {
                header: header.clone(),
                body: body.clone(),
            })
        }
    };
    match observed {
        ResultObservation::Complete => ResultConclusion::Terminal,
        ResultObservation::InputRequired => ResultConclusion::NonTerminal,
        ResultObservation::Unrecognized(token) => {
            ResultConclusion::Incomplete(IncompleteReason::UnrecognizedResultType(token.clone()))
        }
        // Checked before the era, because an unreadable field is not an absent one and must not
        // reach the backward-compatibility rule that reads absence as completion.
        ResultObservation::Malformed => {
            ResultConclusion::Invalid(InvalidReason::MalformedResultType)
        }
        ResultObservation::Missing => {
            if requires_result_type(version) {
                ResultConclusion::Invalid(InvalidReason::MissingResultType)
            } else {
                ResultConclusion::Terminal
            }
        }
    }
}

/// Read a request under the resolved era. `RequestParams._meta` and the version inside it are both
/// required from the version that introduced them, with no `?` in the schema.
///
/// Every unreadable signal is a fault here, whichever side it arrived on. An earlier version fixed
/// the metadata arm and left the header arm, so a malformed header still resolved to an unknown
/// era and then to "no objection". Both now land on `Invalid`, because more evidence does not make
/// an unreadable value readable.
// Targeted rather than module-wide, so a future dead item in this file is still reported.
// `expect` would be stricter but is unfulfilled here: the tests use these under `--all-targets`,
// so the lint does not fire and `expect` itself errors. Removed by the slice-2 conclusion layer.
#[allow(dead_code)]
pub(crate) fn conclude_request(
    era: &EraResolution,
    metadata: &RequestMetadata,
) -> RequestAssessment {
    if matches!(metadata, RequestMetadata::Malformed) {
        return RequestAssessment::Invalid(InvalidReason::MalformedRequestMetadata);
    }
    match era {
        // A refused or aborted request is exactly the case with no response, so leaving the
        // contradiction to the response side loses it. Reporting it from both axes is a
        // deduplication problem for the consumer, which has the call id to do it with.
        EraResolution::Conflicting { header, body } => {
            RequestAssessment::Invalid(InvalidReason::EraConflicting {
                header: header.clone(),
                body: body.clone(),
            })
        }
        EraResolution::Unknown(UnknownReason::MalformedSignal) => {
            RequestAssessment::Invalid(InvalidReason::MalformedEraSignal)
        }
        EraResolution::Unknown(reason) => {
            RequestAssessment::Incomplete(IncompleteReason::EraUnknown(reason.clone()))
        }
        EraResolution::Known(version) if !requires_result_type(version) => RequestAssessment::Valid,
        EraResolution::Known(_) => match metadata {
            RequestMetadata::Present(_) => RequestAssessment::Valid,
            RequestMetadata::Absent => {
                RequestAssessment::Invalid(InvalidReason::MissingRequestMetadata)
            }
            RequestMetadata::Malformed => unreachable!("handled above"),
        },
    }
}

/// The `_meta` key the protocol version travels under on a request.
pub(crate) const PROTOCOL_VERSION_META_KEY: &str = "io.modelcontextprotocol/protocolVersion";

/// The transport header carrying the same value.
pub(crate) const PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

/// Read one header slot as an observation rather than as an `Option`.
///
/// The existing case-insensitive header helper answers `None` for a value that is present and not
/// a string, which reports silence where a signal arrived and failed. That difference is the whole
/// reason `Malformed` exists, so this reads the slot itself.
pub(crate) fn observe_header(headers: Option<&serde_json::Value>) -> Option<EnvelopeObservation> {
    let headers = headers?;
    // A headers node that is not an object is a signal that arrived and failed. `as_object`
    // answering `None` would drop the slot entirely and report silence instead.
    let Some(map) = headers.as_object() else {
        return Some(EnvelopeObservation::Malformed);
    };
    let mut found: Option<&str> = None;
    let mut any = false;
    for (_, value) in map
        .iter()
        .filter(|(k, _)| k.to_ascii_lowercase() == PROTOCOL_VERSION_HEADER)
    {
        any = true;
        match value.as_str() {
            // Shape is checked here rather than downstream so that `Present` can only ever hold a
            // value already accepted as a version. A rejected one is reported as `Malformed` and
            // its bytes are not retained, which is what stops an oversized header from being
            // cloned into every entry's sidecar.
            //
            // Two spellings carrying the same value agree, and agreement is not a choice. Only a
            // disagreement is: `find` would have silently taken whichever came first in map order.
            Some(v) if is_version_shaped(v) && found.is_none_or(|prev| prev == v) => {
                found = Some(v)
            }
            _ => return Some(EnvelopeObservation::Malformed),
        }
    }
    if !any {
        return None;
    }
    Some(match found {
        Some(v) => EnvelopeObservation::Present(v.to_string()),
        None => EnvelopeObservation::Malformed,
    })
}

/// Fold every header slot a transcript carried into one observation.
///
/// Agreement is `Present`; anything else is `Malformed`. Two levels of one transcript stating
/// different versions is not a signal to choose between, and this slice does not invent a
/// resolution rule the specification does not give.
pub(crate) fn fold_envelope(
    observations: Vec<EnvelopeObservation>,
    framed: bool,
) -> EnvelopeObservation {
    if observations.is_empty() {
        return if framed {
            EnvelopeObservation::Absent
        } else {
            EnvelopeObservation::NotApplicable
        };
    }
    let mut seen: Option<&str> = None;
    for o in &observations {
        match o {
            EnvelopeObservation::Present(v) => match seen {
                Some(prev) if prev != v => return EnvelopeObservation::Malformed,
                _ => seen = Some(v),
            },
            _ => return EnvelopeObservation::Malformed,
        }
    }
    seen.map(|v| EnvelopeObservation::Present(v.to_string()))
        .unwrap_or(EnvelopeObservation::Malformed)
}

/// Read `params._meta` as an observation.
pub(crate) fn observe_request_metadata(raw: &serde_json::Value) -> RequestMetadata {
    let Some(meta) = raw.get("params").and_then(|p| p.get("_meta")) else {
        return RequestMetadata::Absent;
    };
    // `_meta` is an object by schema. A scalar or array here is a signal that arrived and failed,
    // and `Value::get` on a non-object answers `None`, which would report it as silence.
    if !meta.is_object() {
        return RequestMetadata::Malformed;
    }
    match meta.get(PROTOCOL_VERSION_META_KEY) {
        None => RequestMetadata::Absent,
        Some(v) => match v.as_str() {
            // Same bound as the header: only an accepted version is retained.
            Some(s) if is_version_shaped(s) => RequestMetadata::Present(s.to_string()),
            _ => RequestMetadata::Malformed,
        },
    }
}

/// Read `result.resultType` as an observation. A present, non-string value is `Malformed` rather
/// than absent, so it can never reach the rule that reads absence as completion.
pub(crate) fn observe_result(raw: &serde_json::Value) -> Option<ResultObservation> {
    let result = raw.get("result")?;
    Some(match result.get("resultType") {
        None => ResultObservation::Missing,
        Some(v) => match v.as_str() {
            Some("complete") => ResultObservation::Complete,
            Some("input_required") => ResultObservation::InputRequired,
            Some(other) if !other.is_empty() => ResultObservation::Unrecognized(other.to_string()),
            _ => ResultObservation::Malformed,
        },
    })
}

#[cfg(test)]
mod tests;
