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

/// A `serde_json::Value` that refuses duplicate object members as it is built.
///
/// `serde_json` collapses duplicate members last-value-wins, so by the time any guard reads the tree
/// the evidence that two were sent is already gone. A post-collapse check cannot recover it and a
/// second full deserialization would double what a hostile input costs, so the refusal belongs in the
/// one pass that builds the tree.
///
/// The rule is every duplicate rather than a list of security-significant names. A name list goes
/// stale the moment a significant key is added, and this file has already been through several
/// enumerations that missed a member. RFC 8259 says names SHOULD be unique and RFC 7493 requires it,
/// so refusing outright cannot go stale. Value-free: the diagnostic names neither the key nor either
/// value.
#[derive(Debug, Clone)]
pub(crate) struct UniqueValue(pub(crate) serde_json::Value);

impl<'de> serde::Deserialize<'de> for UniqueValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueValue;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("any JSON value with unique object members")
            }

            fn visit_unit<E>(self) -> Result<UniqueValue, E> {
                Ok(UniqueValue(serde_json::Value::Null))
            }
            fn visit_none<E>(self) -> Result<UniqueValue, E> {
                Ok(UniqueValue(serde_json::Value::Null))
            }
            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                d: D,
            ) -> Result<UniqueValue, D::Error> {
                <UniqueValue as serde::Deserialize>::deserialize(d)
            }
            fn visit_bool<E>(self, v: bool) -> Result<UniqueValue, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_i64<E>(self, v: i64) -> Result<UniqueValue, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<UniqueValue, E> {
                Ok(UniqueValue(v.into()))
            }
            fn visit_f64<E>(self, v: f64) -> Result<UniqueValue, E> {
                Ok(UniqueValue(
                    serde_json::Number::from_f64(v).map_or(serde_json::Value::Null, Into::into),
                ))
            }
            fn visit_str<E>(self, v: &str) -> Result<UniqueValue, E> {
                Ok(UniqueValue(v.into()))
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<UniqueValue, A::Error> {
                let mut out = Vec::new();
                while let Some(UniqueValue(v)) = seq.next_element()? {
                    out.push(v);
                }
                Ok(UniqueValue(serde_json::Value::Array(out)))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<UniqueValue, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    // Membership is checked on the key alone, before `next_value` materializes
                    // whatever the duplicate carries. Reading it first means a hostile input pays
                    // for a value that is only going to be refused, and the value may be an
                    // arbitrarily large subtree.
                    if out.contains_key(&key) {
                        return Err(serde::de::Error::custom(
                            "JSON object contains a duplicate member",
                        ));
                    }
                    let UniqueValue(value) = map.next_value()?;
                    out.insert(key, value);
                }
                Ok(UniqueValue(serde_json::Value::Object(out)))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

/// Which JSON-RPC message shape this is, carrying the method so that nothing has to read it a
/// second time.
///
/// The distinction is load-bearing rather than tidy. `RequestParams._meta` is required and carries
/// the protocol version; `NotificationParams._meta` is optional and is a different type that does
/// not carry it. Treating a notification as a request therefore invents a fault: a
/// `notifications/progress` under 2026 with no `_meta` is correct, and would be reported as missing
/// required metadata.
///
/// The method travels inside the variant on purpose. Two callers each reaching for `method` with
/// their own `as_str()` is how the discriminants drifted apart before: the parser read one thing and
/// the era axes another, and a shape only one of them rejected fell through the gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageKind<'a> {
    Request { method: &'a str },
    Notification { method: &'a str },
    Response,
}

/// A message shape that cannot be classified at all.
///
/// Value-free: the offending value is chosen by the input, and naming its type is all a reader can
/// act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum MessageShapeError {
    #[error("JSON-RPC method must be a string")]
    NonStringMethod,
}

/// Classify by the two fields JSON-RPC uses.
///
/// A notification is a request object *without* an `id` member, so absence is the discriminant and
/// nothing about the value is. An explicit `"id": null` is therefore a request, not a notification:
/// `RequestId` is `string | number`, which makes a null id an invalid request id rather than an
/// absent one, and calling it a notification drops the required 2026 request metadata for any
/// message that writes that one token.
///
/// A present `method` that is not a string is a malformed message shape rather than a message of
/// some other kind. Answering `Response` for it, which is what folding through `as_str()` does,
/// silently drops the request-metadata requirement the same way. The id vocabulary is settled
/// separately by `normalize_jsonrpc_id`, which refuses booleans, arrays and objects.
pub(crate) fn classify_message(
    v: &serde_json::Value,
) -> Result<MessageKind<'_>, MessageShapeError> {
    let Some(method) = v.get("method") else {
        return Ok(MessageKind::Response);
    };
    let Some(method) = method.as_str() else {
        return Err(MessageShapeError::NonStringMethod);
    };
    Ok(if v.get("id").is_some() {
        MessageKind::Request { method }
    } else {
        MessageKind::Notification { method }
    })
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
    ///
    /// Value-free by design. The token is attacker-chosen, so echoing it hands a channel into
    /// every log that ingests the finding while telling a reader nothing they can act on: the
    /// actionable fact is that this build has no rule for it, not which bytes it was.
    Unrecognized,
    /// The field is present and is not a token at all: a number, an object, an array.
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
    UnrecognizedResultType,
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
/// which blames the reader for a record that is wrong. The value is validated as a real calendar
/// date, leap years included, so `2026-02-31` is malformed rather than unsupported.
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
    // Checked before the era, for the same reason `conclude_request` checks its metadata first: an
    // unreadable field is a fault whatever the era turned out to be, and reading the era first
    // downgrades it to whatever the era's own gap was.
    if matches!(observed, ResultObservation::Malformed) {
        return ResultConclusion::Invalid(InvalidReason::MalformedResultType);
    }
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
        ResultObservation::Unrecognized => {
            ResultConclusion::Incomplete(IncompleteReason::UnrecognizedResultType)
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

/// The transport header carrying the same value. Compared with `eq_ignore_ascii_case`, so the
/// casing here is documentation rather than a value the comparison depends on, and no allocation is
/// made per key of an attacker-supplied header map.
pub(crate) const PROTOCOL_VERSION_HEADER: &str = "MCP-Protocol-Version";

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
        .filter(|(k, _)| k.eq_ignore_ascii_case(PROTOCOL_VERSION_HEADER))
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
    let Some(params) = raw.get("params") else {
        return RequestMetadata::Absent;
    };
    // The fourth container, and the one this rule was still not applied to. `params` is an object
    // by schema, and reaching through a scalar or array with `Value::get` answers `None`, which
    // reads a container that arrived and failed as silence. Under a legacy era that difference
    // decides the verdict: `Absent` is only a fault from 2026 on, so a deviant `params` came back
    // `Valid`, while `Malformed` is a fault whatever the era turned out to be.
    if !params.is_object() {
        return RequestMetadata::Malformed;
    }
    let Some(meta) = params.get("_meta") else {
        return RequestMetadata::Absent;
    };
    // `_meta` is an object by schema, so a scalar or array is a signal that arrived and failed.
    // The guard is explicit because `Value::get` on a non-object answers `None`, which the arm
    // below would have read as the version simply being absent.
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
    // A `result` that is not an object cannot be missing a field. `Value::get` answers `None` on a
    // scalar, array or null, which reported it as `Missing` and let the backward-compatibility rule
    // read it as a completed action.
    if !result.is_object() {
        return Some(ResultObservation::Malformed);
    }
    Some(match result.get("resultType") {
        None => ResultObservation::Missing,
        Some(v) => match v.as_str() {
            Some("complete") => ResultObservation::Complete,
            Some("input_required") => ResultObservation::InputRequired,
            // `ResultType` is an open string union, so any string is syntactically a token,
            // including an empty one. Unrecognized is a statement about this build's rules, so it
            // carries no value; only a non-string is unreadable.
            Some(_) => ResultObservation::Unrecognized,
            None => ResultObservation::Malformed,
        },
    })
}

#[cfg(test)]
mod tests;
