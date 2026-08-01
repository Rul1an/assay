//! Bounded source reader and selective structured serde visitor for the pinned MCP-shaped
//! OTLP/JSON corpus.
//!
//! Untrusted bytes are decoded through `serde::de::DeserializeSeed` implementations rather than
//! materialized into `serde_json::Value`: only the recognized structure is traversed, every
//! ceiling in [`OtlpIngestLimits`] is charged before retention, and unknown subtrees are skipped
//! under the same depth and decoded-byte ceilings without being kept.
//!
//! Typed errors cross the serde boundary by being stashed in shared [`DecodeState`] before a
//! value-free placeholder `serde` error is raised; the entry point recovers the stashed error, so
//! no attacker-controlled token from a `serde_json` message ever reaches a caller. Every seed
//! drives `deserialize_any`, so the visitor — not `serde_json`'s own invalid-type formatting —
//! decides how each actually-present token is classified.

use std::cell::RefCell;
use std::collections::HashSet;
use std::io::Read;

use assay_common::limits::{LimitExceeded, LimitKind, LimitReader};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};

use super::attr::AttributeListSeed;
use super::limits::{
    OtlpIngestError, OtlpIngestLimits, OtlpLimitDimension, RecognizedAttribute, ShapeSite,
    SpanField,
};
use super::observation::{
    ErrorTypeObservation, McpResourceSpansObservation, McpSpanObservation, MethodObservation,
    OperationObservation, RequestIdObservation, SpanKind, SpanProtocolVersion, StatusObservation,
    SEMCONV_PIN,
};
use crate::mcp::era::{is_version_shaped, SUPPORTED_VERSIONS};

/// Decode one OTLP/JSON `resourceSpans` document from `source` under `limits`.
///
/// Source bytes are bounded by [`LimitReader`] (the one dimension with stream semantics); every
/// other ceiling is enforced inside the visitor. On rejection the returned error is typed and
/// value-free.
pub(crate) fn decode_mcp_resource_spans<R: Read>(
    source: R,
    limits: &OtlpIngestLimits,
) -> Result<McpResourceSpansObservation, OtlpIngestError> {
    let state = RefCell::new(DecodeState {
        limits: limits.clone(),
        decoded_bytes: 0,
        span_count: 0,
        typed: None,
    });
    let reader = TrackingReader {
        inner: LimitReader::new(source, limits.max_source_bytes, LimitKind::SourceBytes),
        state: &state,
    };
    let mut de = serde_json::Deserializer::from_reader(reader);
    let outcome = (RootSeed { st: &state })
        .deserialize(&mut de)
        .and_then(|value| {
            de.end()?;
            Ok(value)
        });
    match outcome {
        Ok(value) => Ok(value),
        Err(err) => Err(classify(state.into_inner().typed, &err)),
    }
}

/// Map a decode failure onto the typed vocabulary. A stashed typed error always wins; anything
/// else is classified by category alone so no `serde_json` message text (which can echo input
/// tokens) survives the boundary.
fn classify(typed: Option<OtlpIngestError>, err: &serde_json::Error) -> OtlpIngestError {
    if let Some(typed) = typed {
        return typed;
    }
    match err.classify() {
        serde_json::error::Category::Eof => OtlpIngestError::TruncatedSource,
        serde_json::error::Category::Io => OtlpIngestError::Io,
        serde_json::error::Category::Syntax | serde_json::error::Category::Data => {
            OtlpIngestError::MalformedJson
        }
    }
}

/// Shared traversal state: the ceilings, the two document-global counters, and the typed error
/// slot that carries the real rejection across the serde error type.
pub(super) struct DecodeState {
    pub(super) limits: OtlpIngestLimits,
    decoded_bytes: u64,
    span_count: u64,
    typed: Option<OtlpIngestError>,
}

impl DecodeState {
    /// Stash `err` (first fault wins) and produce the value-free placeholder serde error.
    pub(super) fn fail<E: de::Error>(&mut self, err: OtlpIngestError) -> E {
        if self.typed.is_none() {
            self.typed = Some(err);
        }
        E::custom("otlp ingest rejection")
    }

    pub(super) fn limit<E: de::Error>(&mut self, dimension: OtlpLimitDimension, limit: u64) -> E {
        self.fail(OtlpIngestError::LimitExceeded { dimension, limit })
    }

    /// Charge decoded scalar bytes against the document-global ceiling. Charge model: a string
    /// charges its UTF-8 byte length, a number charges 8, a boolean or null charges 1.
    pub(super) fn charge<E: de::Error>(&mut self, bytes: u64) -> Result<(), E> {
        self.decoded_bytes = self.decoded_bytes.saturating_add(bytes);
        if self.decoded_bytes > self.limits.max_decoded_bytes {
            let max = self.limits.max_decoded_bytes;
            return Err(self.limit(OtlpLimitDimension::DecodedBytes, max));
        }
        Ok(())
    }

    /// Check a container about to be entered at `depth` (root = 1) against the nesting ceiling.
    pub(super) fn enter<E: de::Error>(&mut self, depth: u64) -> Result<(), E> {
        if depth > self.limits.max_nesting_depth {
            let max = self.limits.max_nesting_depth;
            return Err(self.limit(OtlpLimitDimension::NestingDepth, max));
        }
        Ok(())
    }
}

pub(super) type St<'a> = &'a RefCell<DecodeState>;

/// Wraps the source-byte [`LimitReader`] so that read failures are stashed as typed errors
/// before `serde_json` swallows the `io::Error`, whose typed cause is otherwise unreachable
/// through the serde error chain.
struct TrackingReader<'a, R> {
    inner: LimitReader<R>,
    state: St<'a>,
}

impl<R: Read> Read for TrackingReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.inner.read(buf) {
            Ok(n) => Ok(n),
            Err(err) => {
                let typed = match LimitExceeded::from_io(&err) {
                    Some(exceeded) => OtlpIngestError::LimitExceeded {
                        dimension: OtlpLimitDimension::SourceBytes,
                        limit: exceeded.limit,
                    },
                    None => OtlpIngestError::Io,
                };
                let mut state = self.state.borrow_mut();
                if state.typed.is_none() {
                    state.typed = Some(typed);
                }
                Err(err)
            }
        }
    }
}

/// Reject every scalar shape with a typed site fault, so a wrong shape never falls through to a
/// `serde_json` message.
macro_rules! reject_scalars_at {
    ($site:expr) => {
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
    };
}

/// Reject the container shape the visitor does not expect.
macro_rules! reject_container_at {
    (map, $site:expr) => {
        fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
    };
    (seq, $site:expr) => {
        fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
    };
}

pub(super) use {reject_container_at, reject_scalars_at};

// --- Generic bounded skip -------------------------------------------------------------------

/// Skip one unknown value without retaining it, still charging decoded bytes and enforcing the
/// nesting ceiling. `depth` is the level this value's container would occupy if it is one.
pub(super) struct SkipSeed<'a> {
    pub(super) st: St<'a>,
    pub(super) depth: u64,
}

impl<'de> DeserializeSeed<'de> for SkipSeed<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for SkipSeed<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("any bounded value")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        self.st.borrow_mut().charge(1)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        self.st.borrow_mut().charge(8)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        self.st.borrow_mut().charge(8)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        self.st.borrow_mut().charge(8)
    }
    fn visit_str<E: de::Error>(self, s: &str) -> Result<(), E> {
        self.st.borrow_mut().charge(s.len() as u64)
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.st.borrow_mut().charge(1)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        while seq
            .next_element_seed(SkipSeed {
                st: self.st,
                depth: self.depth + 1,
            })?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        while map.next_key_seed(KeySeed { st: self.st })?.is_some() {
            map.next_value_seed(SkipSeed {
                st: self.st,
                depth: self.depth + 1,
            })?;
        }
        Ok(())
    }
}

/// Deserialize a map key (always a string in JSON), charging it to the decoded-byte ceiling.
pub(super) struct KeySeed<'a> {
    pub(super) st: St<'a>,
}

impl<'de> DeserializeSeed<'de> for KeySeed<'_> {
    type Value = String;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<String, D::Error> {
        de.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for KeySeed<'_> {
    type Value = String;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an object member name")
    }
    fn visit_str<E: de::Error>(self, s: &str) -> Result<String, E> {
        self.st.borrow_mut().charge(s.len() as u64)?;
        Ok(s.to_owned())
    }
}

/// Track duplicate members of one traversed object; the retained names are transient and never
/// enter an error or the output.
pub(super) struct Members {
    seen: HashSet<String>,
    site: ShapeSite,
}

impl Members {
    pub(super) fn new(site: ShapeSite) -> Self {
        Self {
            seen: HashSet::new(),
            site,
        }
    }
    pub(super) fn admit<E: de::Error>(&mut self, st: St<'_>, key: &str) -> Result<(), E> {
        if !self.seen.insert(key.to_owned()) {
            return Err(st
                .borrow_mut()
                .fail(OtlpIngestError::DuplicateField(self.site)));
        }
        Ok(())
    }
}

// --- Document structure ---------------------------------------------------------------------

/// Root object: `{"resourceSpans": [...]}` at depth 1.
struct RootSeed<'a> {
    st: St<'a>,
}

impl<'de> DeserializeSeed<'de> for RootSeed<'_> {
    type Value = McpResourceSpansObservation;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for RootSeed<'_> {
    type Value = McpResourceSpansObservation;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an OTLP resourceSpans document")
    }

    reject_scalars_at!(ShapeSite::Root);
    reject_container_at!(seq, ShapeSite::Root);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(1)?;
        let mut members = Members::new(ShapeSite::Root);
        let mut spans = Vec::new();
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &key)?;
            match key.as_str() {
                "resourceSpans" => map.next_value_seed(ResourceSpansSeed {
                    st: self.st,
                    depth: 2,
                    spans: &mut spans,
                })?,
                _ => map.next_value_seed(SkipSeed {
                    st: self.st,
                    depth: 2,
                })?,
            }
        }
        Ok(McpResourceSpansObservation {
            semconv_pin: SEMCONV_PIN,
            spans,
        })
    }
}

/// `resourceSpans`: a sequence of resource entries.
struct ResourceSpansSeed<'a, 'v> {
    st: St<'a>,
    depth: u64,
    spans: &'v mut Vec<McpSpanObservation>,
}

impl<'de> DeserializeSeed<'de> for ResourceSpansSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ResourceSpansSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a resourceSpans array")
    }

    reject_scalars_at!(ShapeSite::ResourceSpans);
    reject_container_at!(map, ShapeSite::ResourceSpans);

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        while seq
            .next_element_seed(ResourceSpansEntrySeed {
                st: self.st,
                depth: self.depth + 1,
                spans: self.spans,
            })?
            .is_some()
        {}
        Ok(())
    }
}

/// One resource entry: `{"resource": {...}, "scopeSpans": [...]}`.
struct ResourceSpansEntrySeed<'a, 'v> {
    st: St<'a>,
    depth: u64,
    spans: &'v mut Vec<McpSpanObservation>,
}

impl<'de> DeserializeSeed<'de> for ResourceSpansEntrySeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ResourceSpansEntrySeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a resourceSpans entry object")
    }

    reject_scalars_at!(ShapeSite::ResourceSpansEntry);
    reject_container_at!(seq, ShapeSite::ResourceSpansEntry);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::ResourceSpansEntry);
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &key)?;
            match key.as_str() {
                "resource" => map.next_value_seed(ResourceSeed {
                    st: self.st,
                    depth: self.depth + 1,
                })?,
                "scopeSpans" => map.next_value_seed(ScopeSpansSeed {
                    st: self.st,
                    depth: self.depth + 1,
                    spans: self.spans,
                })?,
                _ => map.next_value_seed(SkipSeed {
                    st: self.st,
                    depth: self.depth + 1,
                })?,
            }
        }
        Ok(())
    }
}

/// `resource`: only its attribute list is structurally recognized; nothing is extracted.
struct ResourceSeed<'a> {
    st: St<'a>,
    depth: u64,
}

impl<'de> DeserializeSeed<'de> for ResourceSeed<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ResourceSeed<'_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a resource object")
    }

    reject_scalars_at!(ShapeSite::Resource);
    reject_container_at!(seq, ShapeSite::Resource);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::Resource);
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &key)?;
            match key.as_str() {
                "attributes" => {
                    map.next_value_seed(AttributeListSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        extract: None,
                    })?;
                }
                _ => map.next_value_seed(SkipSeed {
                    st: self.st,
                    depth: self.depth + 1,
                })?,
            }
        }
        Ok(())
    }
}

/// `scopeSpans`: a sequence of scope entries.
struct ScopeSpansSeed<'a, 'v> {
    st: St<'a>,
    depth: u64,
    spans: &'v mut Vec<McpSpanObservation>,
}

impl<'de> DeserializeSeed<'de> for ScopeSpansSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ScopeSpansSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a scopeSpans array")
    }

    reject_scalars_at!(ShapeSite::ScopeSpans);
    reject_container_at!(map, ShapeSite::ScopeSpans);

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        while seq
            .next_element_seed(ScopeSpansEntrySeed {
                st: self.st,
                depth: self.depth + 1,
                spans: self.spans,
            })?
            .is_some()
        {}
        Ok(())
    }
}

/// One scope entry: `{"scope": {...}, "spans": [...]}`; the scope itself is skipped.
struct ScopeSpansEntrySeed<'a, 'v> {
    st: St<'a>,
    depth: u64,
    spans: &'v mut Vec<McpSpanObservation>,
}

impl<'de> DeserializeSeed<'de> for ScopeSpansEntrySeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ScopeSpansEntrySeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a scopeSpans entry object")
    }

    reject_scalars_at!(ShapeSite::ScopeSpansEntry);
    reject_container_at!(seq, ShapeSite::ScopeSpansEntry);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::ScopeSpansEntry);
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &key)?;
            match key.as_str() {
                "spans" => map.next_value_seed(SpansSeed {
                    st: self.st,
                    depth: self.depth + 1,
                    spans: self.spans,
                })?,
                _ => map.next_value_seed(SkipSeed {
                    st: self.st,
                    depth: self.depth + 1,
                })?,
            }
        }
        Ok(())
    }
}

/// `spans`: a sequence of span objects, governed by the document-global span ceiling.
struct SpansSeed<'a, 'v> {
    st: St<'a>,
    depth: u64,
    spans: &'v mut Vec<McpSpanObservation>,
}

impl<'de> DeserializeSeed<'de> for SpansSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for SpansSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a spans array")
    }

    reject_scalars_at!(ShapeSite::Spans);
    reject_container_at!(map, ShapeSite::Spans);

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        while let Some(span) = seq.next_element_seed(SpanSeed {
            st: self.st,
            depth: self.depth + 1,
        })? {
            self.spans.push(span);
        }
        Ok(())
    }
}

// --- Span decoding --------------------------------------------------------------------------

/// Recognized MCP attribute values, accumulated while one span's attribute list is traversed.
#[derive(Default)]
pub(super) struct SpanAttrs {
    method: MethodObservation,
    operation: OperationObservation,
    tool_name: Option<String>,
    request_id: Option<RequestIdObservation>,
    protocol_version: SpanProtocolVersion,
    error_type: ErrorTypeObservation,
}

impl SpanAttrs {
    /// Apply one recognized attribute. Duplicate keys were already rejected by the caller, so
    /// each field is set at most once.
    pub(super) fn apply<E: de::Error>(
        &mut self,
        st: St<'_>,
        key: &str,
        value: DecodedValue,
    ) -> Result<(), E> {
        let wrong = |st: St<'_>, which: RecognizedAttribute| -> E {
            st.borrow_mut()
                .fail(OtlpIngestError::RecognizedAttributeWrongType(which))
        };
        match key {
            "mcp.method.name" => {
                self.method = match value {
                    DecodedValue::Str(s) if s == "tools/call" => MethodObservation::ToolsCall,
                    DecodedValue::Str(_) => MethodObservation::OtherMethod,
                    _ => return Err(wrong(st, RecognizedAttribute::MethodName)),
                };
            }
            "gen_ai.operation.name" => {
                self.operation = match value {
                    DecodedValue::Str(s) if s == "execute_tool" => {
                        OperationObservation::ExecuteTool
                    }
                    DecodedValue::Str(_) => OperationObservation::OtherOperation,
                    _ => return Err(wrong(st, RecognizedAttribute::OperationName)),
                };
            }
            "gen_ai.tool.name" => {
                self.tool_name = match value {
                    DecodedValue::Str(s) => Some(s),
                    _ => return Err(wrong(st, RecognizedAttribute::ToolName)),
                };
            }
            "jsonrpc.request.id" => {
                self.request_id = match value {
                    DecodedValue::Str(s) => Some(RequestIdObservation::String(s)),
                    DecodedValue::Int(i) => Some(RequestIdObservation::Integer(i)),
                    _ => return Err(wrong(st, RecognizedAttribute::RequestId)),
                };
            }
            "mcp.protocol.version" => {
                // Malformed is a typed observation state, not a decode failure: the attribute is
                // producer-self-reported, and an unreadable version is a fact to record, never a
                // defect that suppresses the rest of the document.
                self.protocol_version = match value {
                    DecodedValue::Str(s) if is_version_shaped(&s) => {
                        if SUPPORTED_VERSIONS.contains(&s.as_str()) {
                            SpanProtocolVersion::PresentSupported(s)
                        } else {
                            SpanProtocolVersion::PresentUnsupported(s)
                        }
                    }
                    _ => SpanProtocolVersion::Malformed,
                };
            }
            "error.type" => {
                self.error_type = match value {
                    DecodedValue::Str(s) => ErrorTypeObservation::Present(s),
                    _ => return Err(wrong(st, RecognizedAttribute::ErrorType)),
                };
            }
            _ => {}
        }
        Ok(())
    }
}

/// The value of one attribute after bounded decoding. Only the shapes the recognized MCP
/// attributes can legally carry are distinguished; everything else collapses to `Other` without
/// retention.
pub(super) enum DecodedValue {
    Str(String),
    Int(i64),
    Other,
}

/// One span object. The span ceiling is charged on entry, before any content is decoded.
struct SpanSeed<'a> {
    st: St<'a>,
    depth: u64,
}

impl<'de> DeserializeSeed<'de> for SpanSeed<'_> {
    type Value = McpSpanObservation;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for SpanSeed<'_> {
    type Value = McpSpanObservation;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a span object")
    }

    reject_scalars_at!(ShapeSite::Span);
    reject_container_at!(seq, ShapeSite::Span);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        {
            let mut st = self.st.borrow_mut();
            st.span_count += 1;
            if st.span_count > st.limits.max_span_count {
                let max = st.limits.max_span_count;
                return Err(st.limit(OtlpLimitDimension::SpanCount, max));
            }
            st.enter(self.depth)?;
        }
        let mut members = Members::new(ShapeSite::Span);
        let mut trace_id: Option<String> = None;
        let mut span_id: Option<String> = None;
        let mut kind = SpanKind::Unspecified;
        let mut attrs = SpanAttrs::default();
        let mut status = StatusObservation::Absent;
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &key)?;
            match key.as_str() {
                "traceId" => {
                    trace_id = Some(map.next_value_seed(IdSeed {
                        st: self.st,
                        hex_len: 32,
                        field: SpanField::TraceId,
                    })?);
                }
                "spanId" => {
                    span_id = Some(map.next_value_seed(IdSeed {
                        st: self.st,
                        hex_len: 16,
                        field: SpanField::SpanId,
                    })?);
                }
                "kind" => {
                    kind = map.next_value_seed(EnumSeed {
                        st: self.st,
                        field: SpanField::Kind,
                        decode: decode_span_kind,
                    })?;
                }
                "attributes" => {
                    map.next_value_seed(AttributeListSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        extract: Some(&mut attrs),
                    })?;
                }
                "status" => {
                    status = map.next_value_seed(StatusSeed {
                        st: self.st,
                        depth: self.depth + 1,
                    })?;
                }
                _ => map.next_value_seed(SkipSeed {
                    st: self.st,
                    depth: self.depth + 1,
                })?,
            }
        }
        let trace_id = trace_id.ok_or_else(|| {
            self.st
                .borrow_mut()
                .fail(OtlpIngestError::MissingRequiredSpanField(
                    SpanField::TraceId,
                ))
        })?;
        let span_id = span_id.ok_or_else(|| {
            self.st
                .borrow_mut()
                .fail(OtlpIngestError::MissingRequiredSpanField(SpanField::SpanId))
        })?;
        Ok(McpSpanObservation {
            trace_id,
            span_id,
            kind,
            method: attrs.method,
            operation: attrs.operation,
            tool_name: attrs.tool_name,
            request_id: attrs.request_id,
            protocol_version: attrs.protocol_version,
            status,
            error_type: attrs.error_type,
        })
    }
}

/// A fixed-length hex identifier. The value is validated before retention; anything else is a
/// typed malformed-field fault that never carries the offending bytes.
struct IdSeed<'a> {
    st: St<'a>,
    hex_len: usize,
    field: SpanField,
}

impl<'de> DeserializeSeed<'de> for IdSeed<'_> {
    type Value = String;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<String, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for IdSeed<'_> {
    type Value = String;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a hex identifier")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<String, E> {
        self.st.borrow_mut().charge(s.len() as u64)?;
        if s.len() != self.hex_len || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::MalformedSpanField(self.field)));
        }
        Ok(s.to_owned())
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
}

fn decode_span_kind(v: u64) -> Option<SpanKind> {
    match v {
        0 => Some(SpanKind::Unspecified),
        1 => Some(SpanKind::Internal),
        2 => Some(SpanKind::Server),
        3 => Some(SpanKind::Client),
        4 => Some(SpanKind::Producer),
        5 => Some(SpanKind::Consumer),
        _ => None,
    }
}

fn decode_status_code(v: u64) -> Option<StatusObservation> {
    match v {
        0 => Some(StatusObservation::Unset),
        1 => Some(StatusObservation::Ok),
        2 => Some(StatusObservation::Error),
        _ => None,
    }
}

/// A closed proto enum encoded as a non-negative integer. Any other token, and any integer the
/// pinned proto does not define, is a typed malformed-field fault.
struct EnumSeed<'a, T> {
    st: St<'a>,
    field: SpanField,
    decode: fn(u64) -> Option<T>,
}

impl<'de, T> DeserializeSeed<'de> for EnumSeed<'_, T> {
    type Value = T;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<T, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de, T> Visitor<'de> for EnumSeed<'_, T> {
    type Value = T;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a proto enum integer")
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<T, E> {
        self.st.borrow_mut().charge(8)?;
        (self.decode)(v).ok_or_else(|| {
            self.st
                .borrow_mut()
                .fail(OtlpIngestError::MalformedSpanField(self.field))
        })
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<T, E> {
        match u64::try_from(v) {
            Ok(v) => self.visit_u64(v),
            Err(_) => Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::MalformedSpanField(self.field))),
        }
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<T, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<T, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<T, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<T, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<T, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<T, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
}

/// `status`: only `code` is read; the free-text `message` is skipped, never retained.
struct StatusSeed<'a> {
    st: St<'a>,
    depth: u64,
}

impl<'de> DeserializeSeed<'de> for StatusSeed<'_> {
    type Value = StatusObservation;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for StatusSeed<'_> {
    type Value = StatusObservation;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a span status object")
    }

    reject_scalars_at!(ShapeSite::Status);
    reject_container_at!(seq, ShapeSite::Status);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::Status);
        // A present status object with no `code` member carries the proto3 default: Unset.
        let mut observation = StatusObservation::Unset;
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &key)?;
            match key.as_str() {
                "code" => {
                    observation = map.next_value_seed(EnumSeed {
                        st: self.st,
                        field: SpanField::StatusCode,
                        decode: decode_status_code,
                    })?;
                }
                _ => map.next_value_seed(SkipSeed {
                    st: self.st,
                    depth: self.depth + 1,
                })?,
            }
        }
        Ok(observation)
    }
}
