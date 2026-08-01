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
#[derive(Debug, Clone, Default)]
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

/// Track the member names of one object and refuse a repeat.
///
/// `UniqueValue` covers free-form values, but a struct deserialized by serde's derive is a different
/// path: it detects a repeated *known* field and ignores unknown ones entirely, so a duplicate
/// unknown member passed while the claim said every duplicate is refused. A manual `visit_map` that
/// runs this on every key closes it inside the same pass, with no second read of the input.
#[derive(Default)]
pub(crate) struct SeenMembers(std::collections::HashSet<String>);

impl SeenMembers {
    /// Set membership rather than a linear scan. Keys are attacker-chosen and need not repeat, so a
    /// scan is quadratic in the number of *unique* members, which is the cheap shape for an input to
    /// have. The existing input ceiling is the outer budget; this keeps the inner cost linear.
    pub(crate) fn insert<E: serde::de::Error>(&mut self, key: &str) -> Result<(), E> {
        if !self.0.insert(key.to_string()) {
            return Err(serde::de::Error::custom(
                "JSON object contains a duplicate member",
            ));
        }
        Ok(())
    }
}

/// Walk a value for duplicate members without building it.
///
/// `IgnoredAny` skips a subtree entirely, so a duplicate inside an unknown member escaped the rule
/// while the claim said every duplicate is refused. This visits the same ground and keeps nothing,
/// so an ignored subtree is still read for duplicates and still costs no tree.
pub(crate) struct DuplicateAwareSink;

impl<'de> serde::Deserialize<'de> for DuplicateAwareSink {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = DuplicateAwareSink;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("any JSON value with unique object members")
            }
            fn visit_unit<E>(self) -> Result<DuplicateAwareSink, E> {
                Ok(DuplicateAwareSink)
            }
            fn visit_none<E>(self) -> Result<DuplicateAwareSink, E> {
                Ok(DuplicateAwareSink)
            }
            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                d: D,
            ) -> Result<DuplicateAwareSink, D::Error> {
                <DuplicateAwareSink as serde::Deserialize>::deserialize(d)
            }
            fn visit_bool<E>(self, _: bool) -> Result<DuplicateAwareSink, E> {
                Ok(DuplicateAwareSink)
            }
            fn visit_i64<E>(self, _: i64) -> Result<DuplicateAwareSink, E> {
                Ok(DuplicateAwareSink)
            }
            fn visit_u64<E>(self, _: u64) -> Result<DuplicateAwareSink, E> {
                Ok(DuplicateAwareSink)
            }
            fn visit_f64<E>(self, _: f64) -> Result<DuplicateAwareSink, E> {
                Ok(DuplicateAwareSink)
            }
            fn visit_str<E>(self, _: &str) -> Result<DuplicateAwareSink, E> {
                Ok(DuplicateAwareSink)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<DuplicateAwareSink, A::Error> {
                while seq.next_element::<DuplicateAwareSink>()?.is_some() {}
                Ok(DuplicateAwareSink)
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<DuplicateAwareSink, A::Error> {
                let mut seen = SeenMembers::default();
                while let Some(key) = map.next_key::<String>()? {
                    seen.insert::<A::Error>(&key)?;
                    map.next_value::<DuplicateAwareSink>()?;
                }
                Ok(DuplicateAwareSink)
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// A correlation key that keeps the id's JSON type.
///
/// `McpEvent::jsonrpc_id` is a `String` on a public, published struct, and it renders JSON number
/// `1` and JSON string `"1"` identically. Those are different JSON-RPC ids, so keying correlation on
/// that rendering paired a response with a call it did not answer: it consumed the wrong outstanding
/// request, took its era, and a missing `resultType` under a borrowed legacy era could read
/// `Terminal`.
///
/// A number key is the exact text of an integer this build could represent without loss. The variant,
/// not the text, carries the type distinction, so a string is never a number however it is spelled.
/// Crate-private: the public field is unchanged, byte and API compatible.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CorrelationId {
    Str(String),
    Num(String),
}

/// Whether an id is usable at all, which is a different question from whether it can correlate.
///
/// MCP restricts `RequestId` to a string or a number, so those are the shapes a request or a success
/// response may carry. Nothing here decides correlation: see [`correlation_id`].
pub(crate) fn id_is_acceptable(v: &serde_json::Value) -> bool {
    matches!(
        v,
        serde_json::Value::String(_) | serde_json::Value::Number(_)
    )
}

/// Read the correlation key from a message's raw JSON.
///
/// `None` for an absent or unusable id. This function owns the correlation vocabulary; the parser's shape
/// diagnostics for booleans, arrays and objects are more specific messages for a subset of the same
/// refusal.
pub(crate) fn correlation_id(v: &serde_json::Value) -> Option<CorrelationId> {
    v.get("id").and_then(correlation_key)
}

/// The one place that decides whether an id is usable, and what key it becomes.
///
/// The correlation key for an id, or `None` when this build cannot key on it safely.
///
/// A string keys on itself, exactly. A number keys on its exact text only when it is representable as
/// an `i64` or a `u64`; every other JSON number is ingestible and simply does not correlate.
///
/// That asymmetry is the point. Upstream does not define whether `1`, `1.0` and `1e0` name one call,
/// and its own prose and schema disagree about whether an id may be fractional at all. Meanwhile
/// serde_json parses any non-integer number through `f64`, so `9007199254740993.0` has already become
/// `9007199254740992.0` before a key could be built, and two different ids would land on one call.
///
/// Correlating on a value that may be wrong is worse than not correlating: a false pairing hands a
/// response the era of a call it did not answer, which can license a terminal reading. Declining to
/// key leaves the response with the era its own envelope resolved, which is incomplete and true. A
/// later sidecar that keeps the raw lexeme can widen the subset; nothing here forecloses that.
pub(crate) fn correlation_key(v: &serde_json::Value) -> Option<CorrelationId> {
    match v {
        serde_json::Value::String(s) => Some(CorrelationId::Str(s.clone())),
        serde_json::Value::Number(n) if n.is_i64() || n.is_u64() => {
            Some(CorrelationId::Num(n.to_string()))
        }
        _ => None,
    }
}

/// Methods this build gives request semantics to.
///
/// A request MUST carry an id, so without this a request method could shed the requirement simply by
/// omitting the member, and with it the required 2026 `RequestParams._meta`, because notification
/// metadata is optional.
///
/// The set is the request methods of every revision this build recognizes, not only the two the
/// parser gives payloads to. An earlier head listed those two, which left `prompts/get` and the rest
/// evading the rule for exactly the same reason `tools/call` had. Collected from the `Request`
/// interfaces of `schema/{2024-11-05,2025-03-26,2025-06-18,2025-11-25,2026-07-28}` at spec commit
/// `5f5440bb`, which is what the ledger names as the source of truth.
///
/// Methods that left the protocol stay in: a transcript from an older revision is still a transcript,
/// and a request in it is still a request. A name that is in none of them stays a notification, so an
/// unknown extension is unaffected.
pub(crate) const REQUEST_ONLY_METHODS: &[&str] = &[
    "completion/complete",
    "elicitation/create",
    "initialize",
    "logging/setLevel",
    "ping",
    "prompts/get",
    "prompts/list",
    "resources/list",
    "resources/read",
    "resources/subscribe",
    "resources/templates/list",
    "resources/unsubscribe",
    "roots/list",
    "sampling/createMessage",
    "server/discover",
    "subscriptions/listen",
    "tasks/cancel",
    "tasks/get",
    "tasks/list",
    "tasks/result",
    "tools/call",
    "tools/list",
];

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
    /// A no-method object is a response, and a response carries a result or an error, never both and
    /// never neither. Both licensed completion: the result axis read `result`, saw `complete`, and a
    /// 2026 era concluded `Terminal` on a protocol-invalid message. Neither was correlated as a
    /// response, so it consumed an outstanding id and freed it for reuse without answering anything.
    #[error("a JSON-RPC response must carry exactly one of result or error")]
    NotExactlyOneResponseBody,
    /// `error` is not merely a discriminator. Accepting a null, scalar, or object without the two
    /// required members would let the bounded gate treat an unreadable body as a valid error
    /// response and skip result conclusion.
    #[error("a JSON-RPC error response must carry an integer code and string message")]
    MalformedErrorBody,
}

/// Classify by the two fields JSON-RPC uses.
///
/// A notification is a request object *without* an `id` member, so absence is the discriminant and
/// nothing about the value is. An explicit `"id": null` is therefore a request, not a notification:
/// MCP restricts `RequestId` to a string or a number, which makes a null id an invalid request id
/// rather than an absent one, and calling it a notification drops the required 2026 request metadata
/// for any message that writes that one token.
///
/// A present `method` that is not a string is a malformed message shape rather than a message of
/// some other kind. Answering `Response` for it, which is what folding through `as_str()` does,
/// silently drops the request-metadata requirement the same way. Absence is not the only way in:
/// [`REQUEST_ONLY_METHODS`] classifies as requests whatever their id member does, so a request-only
/// method cannot shed the requirement by omitting one.
///
/// The id vocabulary is not settled here. [`correlation_id`] owns it; the parser adds more specific shape
/// diagnostics for booleans, arrays and objects, which are a subset of the same refusal.
pub(crate) fn classify_message(
    v: &serde_json::Value,
) -> Result<MessageKind<'_>, MessageShapeError> {
    let Some(method) = v.get("method") else {
        // Exactly one of the two, checked here so no axis ever reads a shape that is not a response.
        // The `method` branch below is untouched: a hybrid carrying `method` and `result` stays a
        // request, which is a separate rule with its own tests.
        let has_result = v.get("result").is_some();
        let has_error = v.get("error").is_some();
        if has_result == has_error {
            return Err(MessageShapeError::NotExactlyOneResponseBody);
        }
        if has_error {
            let Some(error) = v.get("error").and_then(serde_json::Value::as_object) else {
                return Err(MessageShapeError::MalformedErrorBody);
            };
            let integer_code = error
                .get("code")
                .and_then(serde_json::Value::as_i64)
                .is_some()
                || error
                    .get("code")
                    .and_then(serde_json::Value::as_u64)
                    .is_some();
            let string_message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .is_some();
            if !integer_code || !string_message {
                return Err(MessageShapeError::MalformedErrorBody);
            }
        }
        return Ok(MessageKind::Response);
    };
    let Some(method) = method.as_str() else {
        return Err(MessageShapeError::NonStringMethod);
    };
    Ok(
        if v.get("id").is_some() || REQUEST_ONLY_METHODS.contains(&method) {
            MessageKind::Request { method }
        } else {
            MessageKind::Notification { method }
        },
    )
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
    /// The result claims completion and carries a continuation member in the same object.
    ///
    /// Either form counts. `InputRequiredResult` requires at least one of `inputRequests` or
    /// `requestState`, so both carry a call forward, and a completion claim beside either one is
    /// the same disagreement about whether the action finished. Naming only the first form would
    /// leave a transcript able to state the contradiction in a way this build cannot see.
    ///
    /// Kept apart from `Complete` because folding them would hand the more convenient answer to a
    /// result that contradicts itself. `CallToolResult` sets no `additionalProperties: false` and
    /// lists neither member, so both shapes are schema-legal: this is a fault only the semantic
    /// layer can state.
    CompleteWithContinuation,
    /// The result asks for input and states a continuation member in a shape that cannot carry
    /// one: a `requestState` that is not a string, an `inputRequests` that is not an object.
    ///
    /// Kept apart from `InputRequiredWithoutContinuation` because silence and a broken statement
    /// are different findings with different remediations, and kept apart from `InputRequired`
    /// because a member that cannot be read is not a way to continue. Value-free: neither the
    /// member's value nor which member it was leaves here.
    InputRequiredWithMalformedContinuation,
    /// The result asks for input and carries no way to supply it: neither `inputRequests` nor
    /// `requestState`.
    ///
    /// `InputRequiredResult` says in prose that at least one of the two MUST be present, and its
    /// JSON Schema does not encode that — `required` lists only `resultType`. So this shape passes
    /// the published definition while being unusable: a request for input that names no request and
    /// offers no continuation token cannot be answered, and reading it as an ordinary interim
    /// result would report a call as validly unfinished when nothing can finish it.
    InputRequiredWithoutContinuation,
    /// The field is present and is not a token at all: a number, an object, an array.
    /// Distinct from `Missing`, and the distinction is load-bearing on the legacy arm, where an
    /// absent field MUST be read as `"complete"` and an unreadable one must not inherit that.
    Malformed,
}

/// What one request said about its own capabilities.
///
/// Value-free by design, like `ResultObservation::Unrecognized`: the advertised names are
/// attacker-chosen and retaining them hands a channel into every log that ingests a finding. The
/// actionable fact is whether this build could decide the question, not which strings it was told.
///
/// Capabilities are per request and MUST NOT be inferred from a prior one, so this is bound to a
/// single call through the outstanding map rather than kept per transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CapabilityObservation {
    /// Advertised, with nothing beyond core. A present and empty set is a complete statement, not
    /// silence, which is why it can make an unreadable token *definitively* unrecognized.
    CoreOnly,
    /// Advertised, including at least one extension this build has no rule for. Being advertised
    /// and being understood are different questions.
    ExtensionNotUnderstood,
    /// The `_meta` container was readable and carried no capability member.
    ///
    /// Only that, and deliberately not "absent from a request that should have carried it": the
    /// observer reads a namespaced key without knowing which revision the request was written
    /// against, and an ordinary request from a revision that does not define the member is silent
    /// here for a reason that is not a gap. Whether this silence is a missing statement or simply
    /// nothing to state is decided later, against the resolved era.
    Absent,
    /// The capability member was present and could not be read.
    ///
    /// A fault only under a revision where this capability contract applies, and there on the same
    /// rule the era signal follows: more evidence does not make an unreadable value readable. The
    /// observer cannot make that judgement itself, because it reads a namespaced key without
    /// knowing which revision the request was written against — a value that is malformed by the
    /// later rules says nothing about a call from a revision that never defined the member.
    Malformed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IncompleteReason {
    EraUnknown(UnknownReason),
    /// The token is unreadable to this build, and nothing is outstanding that could still cover it.
    ///
    /// Two revisions reach this by different routes, and both are closed answers rather than gaps.
    /// Under a revision that predates the capability contract there is no set that could have
    /// advertised anything, so the question was never open. Under a revision that defines it, a
    /// stated set naming nothing this build cannot evaluate settles the question outright.
    UnrecognizedResultType,
    /// The token is unreadable and whether anything covers it cannot be decided.
    ///
    /// Reached under a revision that defines the capability contract, whenever the set that would
    /// decide the question did not: it was stated and carried something this build has no rule for,
    /// it was not stated at all, or no observation is available because no request was correlated
    /// to the record. Distinct from `UnrecognizedResultType`: that one is a closed answer, this one
    /// is an open question, and they have different remediations.
    RecognitionUndeterminable,
    /// The result claims completion while carrying a continuation member. Either form counts:
    /// `InputRequiredResult` requires at least one of `inputRequests` or `requestState`, so both
    /// carry a call forward and a completion claim beside either is the same disagreement.
    ///
    /// Incomplete rather than invalid: the bytes are well-formed and schema-legal, so what is
    /// missing is a statement about which of the two the server meant, not a readable value.
    /// Value-free, like every reason here — which member was seen, and what it held, do not
    /// travel.
    ContradictoryResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The capability set arrived and could not be read. Invalid on the same rule as the era
    /// signal, and deliberately not folded into `Absent`: silence and a broken statement are
    /// different findings.
    MalformedCapabilities,
    /// A modern request that states no capability set.
    ///
    /// `RequestMetaObject.required` names both `protocolVersion` and `clientCapabilities`, so from
    /// the revision that defines them a request carrying only the version is incomplete on the
    /// wire. Distinct from `MissingRequestMetadata`, which is the container being absent: here the
    /// container arrived and one required member did not.
    MissingCapabilities,
    /// A result asking for input whose continuation member arrived and could not be read.
    ///
    /// Invalid on the same rule the era signal and the capability set follow: more evidence does
    /// not make an unreadable value readable. Separate from `UncontinuableInputRequired`, which is
    /// about a member that was never stated.
    MalformedContinuation,
    /// A result asking for input that carries no continuation member.
    ///
    /// Invalid rather than incomplete, and the distinction is the whole point: incomplete means a
    /// verdict is still open and more evidence could settle it, while nothing that arrives later
    /// can make this exchange continuable. The bytes are self-defeating, not merely unfinished.
    /// Value-free, like every reason here.
    UncontinuableInputRequired,
}

/// What a request is worth on its own terms. A request has no result, so `NonTerminal` would be a
/// category error here: "no objection" and "valid but unfinished" are different answers and only
/// the second belongs to a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RequestAssessment {
    Valid,
    Incomplete(IncompleteReason),
    Invalid(InvalidReason),
}

/// What a response licenses under a resolved era. Deliberately not a boolean: unknown,
/// contradicted, and a missing required field are three different findings, and `input_required`
/// is valid while not being terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// The typed correlation key for this message, or `None` when this build declines to key on
    /// the id it carried.
    ///
    /// The public `McpEvent.jsonrpc_id` is a `String`, which renders JSON `1` and `"1"` identically
    /// and so cannot tell two different calls apart. The typed key is the one the parser actually
    /// correlates on, and it travels in the sidecar so consumers can reason about the same identity
    /// the parser used, with the public shape unchanged.
    pub(crate) correlation: Option<CorrelationId>,
    /// The capability set of *this call*, carried from the request that opened it.
    ///
    /// `None` means no capability-bearing metadata was readable here, which covers more than one
    /// route: no request was correlated to this record, the record is not a call at all, or the
    /// request carried no `params` or no `_meta` to look in. What they share is that nothing was
    /// observed, which is the only thing this field then reports.
    ///
    /// It never means "nothing was advertised". That is `Some(Absent)`, and it is reached only when
    /// the metadata *was* readable and the capability member was not in it. Conflating the two
    /// would let silence about the question read as an answer to it.
    pub(crate) capability_observation: Option<CapabilityObservation>,
}

/// One event and what was observed about its era, kept apart so the public event shape is
/// unchanged.
#[derive(Debug, Clone)]
pub(crate) struct ParsedMcpEvent {
    pub(crate) event: McpEvent,
    /// Read by the bounded conclusion layer before the public event projection drops it.
    pub(crate) context: McpEraContext,
    /// A valid JSON-RPC error response has no result whose terminality can be concluded.
    ///
    /// Kept as an observation on the internal sidecar so bounded ingest can distinguish it from
    /// silent absence and accept it without inventing a result conclusion or producer decision.
    pub(crate) is_error_response: bool,
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
pub(crate) fn is_version_shaped(v: &str) -> bool {
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
pub(crate) fn conclude(
    era: &EraResolution,
    observed: &ResultObservation,
    capability: Option<&CapabilityObservation>,
) -> ResultConclusion {
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
    // Whether the capability contract exists at all under the resolved revision.
    //
    // The observer reads a namespaced `_meta` key with no idea which revision the request was
    // written against, so everything it reports about that key is only a statement under a revision
    // that defines it. Applying the axis to a legacy call would invent a contract the client was
    // never held to: an ordinary 2025 request has no reason to carry the key, and reading its
    // absence as an open question turns conformance into a finding.
    let modern = requires_result_type(version);
    // An unreadable capability set is a fault on the same rule as the era signal, but only where
    // the contract applies. Deliberately after the era resolution: a value that is malformed by
    // 2026 rules says nothing about a 2025 call, and checking it first let a future field decide a
    // legacy verdict.
    if modern && matches!(capability, Some(CapabilityObservation::Malformed)) {
        return ResultConclusion::Invalid(InvalidReason::MalformedCapabilities);
    }
    match observed {
        ResultObservation::Complete => ResultConclusion::Terminal,
        ResultObservation::InputRequired => ResultConclusion::NonTerminal,
        // Terminal is the one answer this must never be: the result asks for input in the same
        // breath as it claims to be done, and picking the completion half is picking the reading
        // that closes the record.
        ResultObservation::CompleteWithContinuation => {
            ResultConclusion::Incomplete(IncompleteReason::ContradictoryResult)
        }
        // Not `NonTerminal`: that answer says "valid but unfinished", and this exchange cannot be
        // finished at all. Reporting it as an ordinary interim result would hand a dead call the
        // vocabulary of a live one.
        ResultObservation::InputRequiredWithoutContinuation => {
            ResultConclusion::Invalid(InvalidReason::UncontinuableInputRequired)
        }
        // Never `NonTerminal`. The runtime conclusion path does not validate against the vendored
        // schema, so nothing downstream would catch a `requestState` that is a number: if this
        // arm licensed a valid interim result, a call that cannot be continued would travel as one
        // that can.
        ResultObservation::InputRequiredWithMalformedContinuation => {
            ResultConclusion::Invalid(InvalidReason::MalformedContinuation)
        }
        // Recognition is capability-relative only where the capability contract exists. Under a
        // revision with no such member there was nothing that could have been advertised, so the
        // question was never open and the answer this build gave before the axis existed stands,
        // whatever the future key happens to contain.
        ResultObservation::Unrecognized if !modern => {
            ResultConclusion::Incomplete(IncompleteReason::UnrecognizedResultType)
        }
        // Where the contract does exist, the set decides. A present one naming nothing beyond core
        // settles the question: nothing covers this token. An absent one, or one advertising an
        // extension with no rule here, leaves it open — and no mapping from an extension name to a
        // result token may be invented to close it.
        ResultObservation::Unrecognized => match capability {
            // The only closed answer. A stated set that names nothing this build cannot evaluate
            // settles the question: nothing advertised covers this token.
            Some(CapabilityObservation::CoreOnly) => {
                ResultConclusion::Incomplete(IncompleteReason::UnrecognizedResultType)
            }
            // Stated and unevaluable, or required and not stated: either way the set that would
            // decide the question did not decide it.
            Some(CapabilityObservation::Absent | CapabilityObservation::ExtensionNotUnderstood) => {
                ResultConclusion::Incomplete(IncompleteReason::RecognitionUndeterminable)
            }
            // No observation at all: no request was correlated to this record. That cannot reach
            // the closed answer either — an orphan response gives no ground to say nothing
            // advertised covers the token, and claiming otherwise reads absence of evidence as
            // evidence.
            None => ResultConclusion::Incomplete(IncompleteReason::RecognitionUndeterminable),
            Some(CapabilityObservation::Malformed) => unreachable!("handled above"),
        },
        // Checked before the era, because an unreadable field is not an absent one and must not
        // reach the backward-compatibility rule that reads absence as completion.
        ResultObservation::Malformed => {
            ResultConclusion::Invalid(InvalidReason::MalformedResultType)
        }
        ResultObservation::Missing => {
            if modern {
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
pub(crate) fn conclude_request(
    era: &EraResolution,
    metadata: &RequestMetadata,
    capability: Option<&CapabilityObservation>,
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
        // The capability contract belongs to the revision that defines it. A legacy request could
        // not have carried the member, so judging it by that rule would manufacture a fault out of
        // conformance.
        EraResolution::Known(version) if !requires_result_type(version) => RequestAssessment::Valid,
        EraResolution::Known(_) => match metadata {
            // `RequestMetaObject.required` names `protocolVersion` and `clientCapabilities` both,
            // so a version alone does not make the metadata complete. This is read here rather than
            // left to the response side: a refused or abandoned call has no response, and that is
            // exactly the case where the request is the only record there will ever be.
            RequestMetadata::Present(_) => match capability {
                Some(CapabilityObservation::CoreOnly)
                | Some(CapabilityObservation::ExtensionNotUnderstood) => RequestAssessment::Valid,
                Some(CapabilityObservation::Malformed) => {
                    RequestAssessment::Invalid(InvalidReason::MalformedCapabilities)
                }
                // `None` reaches here only when the metadata container was readable enough to
                // yield a version, so there was somewhere to look and the member was not in it.
                Some(CapabilityObservation::Absent) | None => {
                    RequestAssessment::Invalid(InvalidReason::MissingCapabilities)
                }
            },
            RequestMetadata::Absent => {
                RequestAssessment::Invalid(InvalidReason::MissingRequestMetadata)
            }
            RequestMetadata::Malformed => unreachable!("handled above"),
        },
    }
}

/// The `_meta` key the protocol version travels under on a request.
pub(crate) const PROTOCOL_VERSION_META_KEY: &str = "io.modelcontextprotocol/protocolVersion";

/// The `_meta` key the client's capability set travels under on a request.
pub(crate) const CLIENT_CAPABILITIES_META_KEY: &str = "io.modelcontextprotocol/clientCapabilities";

/// The members `ClientCapabilities` defines. The definition does not set
/// `additionalProperties: false`, so this list is what *this build* has rules for, not what the
/// object may contain — which is why an unrecognised member is a legal statement this build cannot
/// evaluate rather than a fault.
const CAPABILITY_ELICITATION_KEY: &str = "elicitation";
const CAPABILITY_ROOTS_KEY: &str = "roots";
const CAPABILITY_SAMPLING_KEY: &str = "sampling";
/// The two open maps inside that set: non-standard capabilities and optional MCP extensions.
const CAPABILITY_EXPERIMENTAL_KEY: &str = "experimental";
const CAPABILITY_EXTENSIONS_KEY: &str = "extensions";

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

/// Read `params._meta`'s capability set as an observation, for the request that carries it.
///
/// Nothing about the advertised names is retained. The only questions asked are whether the set was
/// stated, whether it was readable, and whether it named anything outside core; the answer to the
/// last one is a boolean, so no attacker-supplied string survives the read.
///
/// `None` when there is no request-shaped metadata container to look in at all. That is the same
/// silence `RequestMetadata::Absent` reports and is not a capability answer.
pub(crate) fn observe_client_capabilities(
    raw: &serde_json::Value,
) -> Option<CapabilityObservation> {
    let params = raw.get("params")?;
    if !params.is_object() {
        return Some(CapabilityObservation::Malformed);
    }
    let meta = params.get("_meta")?;
    if !meta.is_object() {
        return Some(CapabilityObservation::Malformed);
    }
    Some(match meta.get(CLIENT_CAPABILITIES_META_KEY) {
        None => CapabilityObservation::Absent,
        Some(caps) => {
            let Some(caps) = caps.as_object() else {
                return Some(CapabilityObservation::Malformed);
            };
            let mut beyond_core = false;
            for (name, value) in caps {
                match name.as_str() {
                    // The core members this build has rules for. Their contents are not read: what
                    // matters is that the client named a capability this build understands.
                    CAPABILITY_ELICITATION_KEY | CAPABILITY_ROOTS_KEY | CAPABILITY_SAMPLING_KEY => {
                        // Known by name and unreadable by shape is a broken statement, not silence.
                        if !value.is_object() {
                            return Some(CapabilityObservation::Malformed);
                        }
                    }
                    // Two open maps, read the same way. Empty is a complete statement that none is
                    // offered; inhabited means at least one entry this build has no rule for. Only
                    // whether the map is inhabited is read, so no advertised name leaves here.
                    CAPABILITY_EXPERIMENTAL_KEY | CAPABILITY_EXTENSIONS_KEY => {
                        let Some(map) = value.as_object() else {
                            return Some(CapabilityObservation::Malformed);
                        };
                        beyond_core |= !map.is_empty();
                    }
                    // `ClientCapabilities` does not set `additionalProperties: false`, so an
                    // unrecognised member is legal rather than a fault — and it is exactly a
                    // capability this build cannot evaluate. Reading it as core would turn unknown
                    // vocabulary into a closed answer, which is the fold this whole axis exists to
                    // prevent. The name is not retained, only the fact that one was there.
                    _ => beyond_core = true,
                }
            }
            if beyond_core {
                CapabilityObservation::ExtensionNotUnderstood
            } else {
                CapabilityObservation::CoreOnly
            }
        }
    })
}

/// What a result's continuation members amount to.
///
/// `InputRequiredResult` requires at least one of `inputRequests` or `requestState`, so neither is
/// the continuation on its own: they are two forms of the same thing, and a rule reading one and
/// not the other leaves half the shape unobserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContinuationShape {
    /// Neither member is stated. Absent and explicitly null are the same silence.
    Absent,
    /// At least one member is stated in a shape that could carry a call forward.
    Present,
    /// A member is stated in a shape that cannot: a `requestState` that is not a string, an
    /// `inputRequests` that is not an object. Distinct from `Absent`, on the rule this module
    /// applies everywhere else — silence and a broken statement are different findings.
    Malformed,
}

/// Read both continuation members as one shape.
///
/// Only the top-level type of each member is checked, deliberately. Re-deriving `InputRequest`
/// here would duplicate a published definition into this file and go stale against it; what the
/// conclusion layer needs is whether the member could carry a call forward at all, and a number or
/// an array answers that without any deeper reading. Nothing about the values is retained.
///
/// Presence rather than content decides `Present`: an empty request map or an empty continuation
/// token is still a statement, and treating it as silence would let a contradiction be hidden by
/// emptying the value.
///
/// `Malformed` wins over `Present`. One member arriving broken is a fault whatever the other one
/// says, and letting a well-formed sibling cover for it would report a result as usable on the
/// strength of the half that happened to parse.
fn continuation_shape(result: &serde_json::Value) -> ContinuationShape {
    let mut present = false;
    // `requestState` is an opaque continuation token: `{"type": "string"}`.
    match result.get(CONTINUATION_REQUEST_STATE_KEY) {
        None => {}
        Some(v) if v.is_null() => {}
        Some(v) if v.is_string() => present = true,
        Some(_) => return ContinuationShape::Malformed,
    }
    // `inputRequests` is a map of server-initiated requests: `{"type": "object"}`.
    match result.get(CONTINUATION_INPUT_REQUESTS_KEY) {
        None => {}
        Some(v) if v.is_null() => {}
        Some(v) if v.is_object() => present = true,
        Some(_) => return ContinuationShape::Malformed,
    }
    if present {
        ContinuationShape::Present
    } else {
        ContinuationShape::Absent
    }
}

const CONTINUATION_INPUT_REQUESTS_KEY: &str = "inputRequests";
const CONTINUATION_REQUEST_STATE_KEY: &str = "requestState";

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
            // A claim of completion is read together with any continuation member beside it,
            // `inputRequests` or `requestState`, because a result carrying both a completion claim
            // and a way to continue is not the same observation as one carrying neither. Presence
            // is the test, not content: an empty continuation value alongside a completion claim is
            // still a result disagreeing with itself, and treating it as clean would let the
            // contradiction be hidden by emptying the value. An explicit null is silence rather
            // than a statement, so it does not count.
            // A broken continuation member folds into the contradiction here rather than getting
            // its own state, and that is fail-closed on purpose: whatever the member was meant to
            // say, a completion claim carrying it must not reach `Terminal`. The interim arm below
            // cannot make the same fold, because there the fold would land on *valid*.
            Some("complete") => match continuation_shape(result) {
                ContinuationShape::Absent => ResultObservation::Complete,
                ContinuationShape::Present | ContinuationShape::Malformed => {
                    ResultObservation::CompleteWithContinuation
                }
            },
            // The same continuation read as the completion arm above, and deliberately the same
            // helper: a claim of completion beside a continuation member and a request for input
            // without one are the two ways this pair can disagree, and they must not drift apart.
            Some("input_required") => match continuation_shape(result) {
                ContinuationShape::Present => ResultObservation::InputRequired,
                ContinuationShape::Absent => ResultObservation::InputRequiredWithoutContinuation,
                ContinuationShape::Malformed => {
                    ResultObservation::InputRequiredWithMalformedContinuation
                }
            },
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
