//! Bounded source reader and selective structured serde visitor for the pinned MCP-shaped
//! OTLP/JSON corpus.
//!
//! Untrusted bytes are decoded through `serde::de::DeserializeSeed` implementations rather than
//! materialized into `serde_json::Value`: only the recognized structure is traversed. The source
//! ceiling bounds serde's transient token scratch; traversal ceilings are charged before content
//! enters the domain observation, and unknown subtrees are skipped under the same depth and
//! decoded-byte ceilings without being kept.
//!
//! Typed errors cross the serde boundary by being stashed in shared [`DecodeState`] before a
//! value-free placeholder `serde` error is raised; the entry point recovers the stashed error, so
//! no attacker-controlled token from a `serde_json` message ever reaches a caller. Every seed
//! drives `deserialize_any`, so the visitor — not `serde_json`'s own invalid-type formatting —
//! decides how each actually-present token is classified.

use std::cell::RefCell;
use std::collections::HashSet;
use std::io::{BufReader, Read};

use assay_common::limits::{LimitExceeded, LimitKind, LimitReader};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};

use super::attr::AttributeListSeed;
use super::limits::{
    OtlpIngestError, OtlpIngestLimits, OtlpLimitDimension, RecognizedAttribute, ShapeSite,
    SpanField,
};
use super::observation::{
    ErrorTypeObservation, InstrumentationScopeObservation, McpResourceSpansObservation,
    McpSpanObservation, MethodObservation, OperationObservation, RequestIdObservation,
    RpcResponseStatusObservation, SpanKind, SpanProtocolVersion, StatusObservation, SEMCONV_PIN,
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
    // Buffer inside the source limiter: serde_json requests bytes one at a time, while the
    // BufReader batches underlying reads without prefetching past the configured source ceiling.
    let reader = TrackingReader {
        inner: BufReader::new(LimitReader::new(
            source,
            limits.max_source_bytes,
            LimitKind::SourceBytes,
        )),
        state: &state,
    };
    let mut de = serde_json::Deserializer::from_reader(reader);
    let outcome = (RootSeed {
        st: &state,
        depth: 1,
    })
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

/// Wraps the buffered source-byte [`LimitReader`] so read failures are stashed as typed errors
/// before `serde_json` swallows the `io::Error`, whose typed cause is otherwise unreachable
/// through the serde error chain. Genericity keeps buffering inside the limiter.
struct TrackingReader<'a, R> {
    inner: R,
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
macro_rules! reject_non_null_scalars_at {
    ($site:expr) => {
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            self.st.borrow_mut().charge(1)?;
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            self.st.borrow_mut().charge(8)?;
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            self.st.borrow_mut().charge(8)?;
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            self.st.borrow_mut().charge(8)?;
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            self.st.borrow_mut().charge(value.len() as u64)?;
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
    };
}

macro_rules! reject_scalars_at {
    ($site:expr) => {
        reject_non_null_scalars_at!($site);
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            self.st.borrow_mut().charge(1)?;
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
            self.st.borrow_mut().enter(self.depth)?;
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
    };
    (seq, $site:expr) => {
        fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
            self.st.borrow_mut().enter(self.depth)?;
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape($site)))
        }
    };
}

pub(super) use {reject_container_at, reject_non_null_scalars_at, reject_scalars_at};

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
        // Duplicate members fail closed in skipped containers too: skipping is not a license to
        // accept a document a traversed path would refuse. The member names are already charged
        // to the decoded-byte ceiling, so this transient set is bounded.
        let mut members = Members::new(ShapeSite::SkippedContainer);
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &key)?;
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
    depth: u64,
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
        self.st.borrow_mut().enter(self.depth)?;
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

    reject_non_null_scalars_at!(ShapeSite::ResourceSpans);
    reject_container_at!(map, ShapeSite::ResourceSpans);

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.st.borrow_mut().charge(1)?;
        Ok(())
    }

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

    reject_non_null_scalars_at!(ShapeSite::Resource);
    reject_container_at!(seq, ShapeSite::Resource);

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.st.borrow_mut().charge(1)?;
        Ok(())
    }

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

    reject_non_null_scalars_at!(ShapeSite::ScopeSpans);
    reject_container_at!(map, ShapeSite::ScopeSpans);

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.st.borrow_mut().charge(1)?;
        Ok(())
    }

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

/// One scope entry: `{"scope": {...}, "spans": [...]}`.
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
        let first_span = self.spans.len();
        let mut scope = None;
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &key)?;
            match key.as_str() {
                "scope" => {
                    scope = map.next_value_seed(InstrumentationScopeSeed {
                        st: self.st,
                        depth: self.depth + 1,
                    })?;
                }
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
        if let Some(scope) = scope {
            for span in &mut self.spans[first_span..] {
                span.instrumentation_scope = Some(scope.clone());
            }
        }
        Ok(())
    }
}

struct InstrumentationScopeSeed<'a> {
    st: St<'a>,
    depth: u64,
}

impl<'de> DeserializeSeed<'de> for InstrumentationScopeSeed<'_> {
    type Value = Option<InstrumentationScopeObservation>;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for InstrumentationScopeSeed<'_> {
    type Value = Option<InstrumentationScopeObservation>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an instrumentation scope object")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
    }
    reject_container_at!(seq, ShapeSite::InstrumentationScope);

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        Ok(None)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::InstrumentationScope);
        let mut name = None;
        let mut version = None;
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &key)?;
            match key.as_str() {
                "name" => {
                    name = map.next_value_seed(OptionalScopeStringSeed {
                        st: self.st,
                        depth: self.depth + 1,
                    })?;
                }
                "version" => {
                    version = map.next_value_seed(OptionalScopeStringSeed {
                        st: self.st,
                        depth: self.depth + 1,
                    })?;
                }
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
        if name.is_none() && version.is_none() {
            Ok(None)
        } else {
            Ok(Some(InstrumentationScopeObservation { name, version }))
        }
    }
}

struct OptionalScopeStringSeed<'a> {
    st: St<'a>,
    depth: u64,
}

impl<'de> DeserializeSeed<'de> for OptionalScopeStringSeed<'_> {
    type Value = Option<String>;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for OptionalScopeStringSeed<'_> {
    type Value = Option<String>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an instrumentation scope string")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(value.len() as u64)?;
        Ok((!value.is_empty()).then(|| value.to_owned()))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        Ok(None)
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
    }

    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self.st.borrow_mut().fail(OtlpIngestError::UnexpectedShape(
            ShapeSite::InstrumentationScope,
        )))
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

    reject_non_null_scalars_at!(ShapeSite::Spans);
    reject_container_at!(map, ShapeSite::Spans);

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.st.borrow_mut().charge(1)?;
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        while let Some(span) = seq.next_element_seed(SpanSeed {
            st: self.st,
            depth: self.depth + 1,
        })? {
            if let Some(span) = span {
                self.spans.push(span);
            }
        }
        Ok(())
    }
}

// --- Span decoding --------------------------------------------------------------------------

/// Bounded raw values for attributes that may participate in the MCP projection. Type validation
/// is deferred until the full list establishes whether the span is MCP-shaped; otherwise generic
/// OTLP attributes on unrelated spans could manufacture MCP projection failures.
#[derive(Default)]
pub(super) struct SpanAttrs {
    mcp_marker_seen: bool,
    method: Option<DecodedValue>,
    operation: Option<DecodedValue>,
    tool_name: Option<DecodedValue>,
    request_id: Option<DecodedValue>,
    protocol_version: Option<DecodedValue>,
    resource_uri: Option<DecodedValue>,
    session_id: Option<DecodedValue>,
    error_type: Option<DecodedValue>,
    rpc_response_status: Option<DecodedValue>,
}

impl SpanAttrs {
    /// Retain one bounded candidate value. Duplicate keys were already rejected by the caller, so
    /// each slot is set at most once.
    pub(super) fn apply<E: de::Error>(
        &mut self,
        _st: St<'_>,
        key: &str,
        value: DecodedValue,
    ) -> Result<(), E> {
        match key {
            "mcp.method.name" => {
                self.mcp_marker_seen = true;
                self.method = Some(value);
            }
            "mcp.protocol.version" => {
                self.mcp_marker_seen = true;
                self.protocol_version = Some(value);
            }
            "mcp.resource.uri" => {
                self.mcp_marker_seen = true;
                self.resource_uri = Some(value);
            }
            "mcp.session.id" => {
                self.mcp_marker_seen = true;
                self.session_id = Some(value);
            }
            "gen_ai.operation.name" => self.operation = Some(value),
            "gen_ai.tool.name" => self.tool_name = Some(value),
            "jsonrpc.request.id" => self.request_id = Some(value),
            "error.type" => self.error_type = Some(value),
            "rpc.response.status_code" => self.rpc_response_status = Some(value),
            _ => {}
        }
        Ok(())
    }

    fn project<E: de::Error>(self, st: St<'_>) -> Result<Option<ProjectedSpanAttrs>, E> {
        let wrong = |which: RecognizedAttribute| -> E {
            st.borrow_mut()
                .fail(OtlpIngestError::RecognizedAttributeWrongType(which))
        };
        let Some(method_value) = self.method else {
            if self.mcp_marker_seen {
                return Err(st
                    .borrow_mut()
                    .fail(OtlpIngestError::MissingRequiredAttribute(
                        RecognizedAttribute::MethodName,
                    )));
            }
            return Ok(None);
        };
        let method = match method_value {
            DecodedValue::Str(s) if s == "tools/call" => MethodObservation::ToolsCall,
            DecodedValue::Str(_) => MethodObservation::OtherMethod,
            DecodedValue::Other => return Err(wrong(RecognizedAttribute::MethodName)),
        };
        for (value, field) in [
            (self.resource_uri, RecognizedAttribute::ResourceUri),
            (self.session_id, RecognizedAttribute::SessionId),
        ] {
            if matches!(value, Some(DecodedValue::Other)) {
                return Err(wrong(field));
            }
        }
        let operation = match self.operation {
            Some(DecodedValue::Str(s)) if s == "execute_tool" => OperationObservation::ExecuteTool,
            Some(DecodedValue::Str(_)) => OperationObservation::OtherOperation,
            Some(DecodedValue::Other) => return Err(wrong(RecognizedAttribute::OperationName)),
            None => OperationObservation::Absent,
        };
        let tool_name = match self.tool_name {
            Some(DecodedValue::Str(s)) => Some(s),
            Some(DecodedValue::Other) => return Err(wrong(RecognizedAttribute::ToolName)),
            None => None,
        };
        let request_id = match self.request_id {
            Some(DecodedValue::Str(s)) => Some(RequestIdObservation::String(s)),
            Some(DecodedValue::Other) => return Err(wrong(RecognizedAttribute::RequestId)),
            None => None,
        };
        // Malformed is an observation state, not a decode failure: this is producer-self-reported
        // telemetry and never substitutes for MCP transport evidence.
        let protocol_version = match self.protocol_version {
            Some(DecodedValue::Str(s)) if is_version_shaped(&s) => {
                if SUPPORTED_VERSIONS.contains(&s.as_str()) {
                    SpanProtocolVersion::PresentSupported(s)
                } else {
                    SpanProtocolVersion::PresentUnsupported(s)
                }
            }
            Some(_) => SpanProtocolVersion::Malformed,
            None => SpanProtocolVersion::Absent,
        };
        let error_type = match self.error_type {
            Some(DecodedValue::Str(s)) => ErrorTypeObservation::Present(s),
            Some(DecodedValue::Other) => return Err(wrong(RecognizedAttribute::ErrorType)),
            None => ErrorTypeObservation::Absent,
        };
        let rpc_response_status = match self.rpc_response_status {
            Some(DecodedValue::Str(s)) => RpcResponseStatusObservation::Present(s),
            Some(DecodedValue::Other) => {
                return Err(wrong(RecognizedAttribute::RpcResponseStatusCode))
            }
            None => RpcResponseStatusObservation::Absent,
        };
        Ok(Some(ProjectedSpanAttrs {
            method,
            operation,
            tool_name,
            request_id,
            protocol_version,
            error_type,
            rpc_response_status,
        }))
    }
}

struct ProjectedSpanAttrs {
    method: MethodObservation,
    operation: OperationObservation,
    tool_name: Option<String>,
    request_id: Option<RequestIdObservation>,
    protocol_version: SpanProtocolVersion,
    error_type: ErrorTypeObservation,
    rpc_response_status: RpcResponseStatusObservation,
}

/// The value of one attribute after bounded decoding. Only the shapes the recognized MCP
/// attributes can legally carry are distinguished; everything else collapses to `Other` without
/// retention.
pub(super) enum DecodedValue {
    Str(String),
    Other,
}

/// One span object. The span ceiling is charged on entry, before any content is decoded.
struct SpanSeed<'a> {
    st: St<'a>,
    depth: u64,
}

impl<'de> DeserializeSeed<'de> for SpanSeed<'_> {
    type Value = Option<McpSpanObservation>;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        let mut st = self.st.borrow_mut();
        st.span_count += 1;
        if st.span_count > st.limits.max_span_count {
            let max = st.limits.max_span_count;
            return Err(st.limit(OtlpLimitDimension::SpanCount, max));
        }
        drop(st);
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for SpanSeed<'_> {
    type Value = Option<McpSpanObservation>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a span object")
    }

    reject_scalars_at!(ShapeSite::Span);
    reject_container_at!(seq, ShapeSite::Span);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::Span);
        let mut trace_id: Option<String> = None;
        let mut span_id: Option<String> = None;
        let mut parent_span_id: Option<String> = None;
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
                        depth: self.depth + 1,
                    })?);
                }
                "spanId" => {
                    span_id = Some(map.next_value_seed(IdSeed {
                        st: self.st,
                        hex_len: 16,
                        field: SpanField::SpanId,
                        depth: self.depth + 1,
                    })?);
                }
                "parentSpanId" => {
                    parent_span_id = map.next_value_seed(OptionalIdSeed {
                        st: self.st,
                        hex_len: 16,
                        field: SpanField::ParentSpanId,
                        depth: self.depth + 1,
                    })?;
                }
                "kind" => {
                    kind = map.next_value_seed(EnumSeed {
                        st: self.st,
                        field: SpanField::Kind,
                        decode: decode_span_kind,
                        depth: self.depth + 1,
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
        let Some(attrs) = attrs.project(self.st)? else {
            // Mixed exports contain unrelated spans. Generic GenAI/RPC/error attributes alone do
            // not establish MCP identity, so their projection rules do not apply here.
            return Ok(None);
        };
        Ok(Some(McpSpanObservation {
            trace_id,
            span_id,
            parent_span_id,
            instrumentation_scope: None,
            kind,
            method: attrs.method,
            operation: attrs.operation,
            tool_name: attrs.tool_name,
            request_id: attrs.request_id,
            protocol_version: attrs.protocol_version,
            status,
            error_type: attrs.error_type,
            rpc_response_status: attrs.rpc_response_status,
        }))
    }
}

struct OptionalIdSeed<'a> {
    st: St<'a>,
    hex_len: usize,
    field: SpanField,
    depth: u64,
}

impl<'de> DeserializeSeed<'de> for OptionalIdSeed<'_> {
    type Value = Option<String>;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for OptionalIdSeed<'_> {
    type Value = Option<String>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an optional hex identifier")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(value.len() as u64)?;
        if value.is_empty() {
            return Ok(None);
        }
        if value.len() != self.hex_len
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
            || value.bytes().all(|byte| byte == b'0')
        {
            return Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::MalformedSpanField(self.field)));
        }
        Ok(Some(value.to_ascii_lowercase()))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        Ok(None)
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
}

/// A fixed-length hex identifier. OTLP/JSON hex ids are case-insensitive, so both cases are
/// accepted and the retained id is normalized to lowercase — one span must never split into
/// two identities by case. Anything else is a typed malformed-field fault that never carries
/// the offending bytes.
struct IdSeed<'a> {
    st: St<'a>,
    hex_len: usize,
    field: SpanField,
    depth: u64,
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
        if s.len() != self.hex_len
            || !s.bytes().all(|b| b.is_ascii_hexdigit())
            || s.bytes().all(|b| b == b'0')
        {
            return Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::MalformedSpanField(self.field)));
        }
        Ok(s.to_ascii_lowercase())
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<String, E> {
        self.st.borrow_mut().charge(1)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<String, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<String, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<String, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<String, E> {
        self.st.borrow_mut().charge(1)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
}

fn decode_span_kind(v: i32) -> SpanKind {
    match v {
        0 => SpanKind::Unspecified,
        1 => SpanKind::Internal,
        2 => SpanKind::Server,
        3 => SpanKind::Client,
        4 => SpanKind::Producer,
        5 => SpanKind::Consumer,
        _ => SpanKind::Unknown,
    }
}

fn decode_status_code(v: i32) -> StatusObservation {
    match v {
        0 => StatusObservation::Unset,
        1 => StatusObservation::Ok,
        2 => StatusObservation::Error,
        _ => StatusObservation::Unknown,
    }
}

/// An open proto3 enum encoded as an `int32`. Future numbers, including negative values, map to
/// the decoder's value-free unknown state; out-of-range numbers and wrong JSON types are malformed.
struct EnumSeed<'a, T> {
    st: St<'a>,
    field: SpanField,
    decode: fn(i32) -> T,
    depth: u64,
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
        match i32::try_from(v) {
            Ok(v) => Ok((self.decode)(v)),
            Err(_) => Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::MalformedSpanField(self.field))),
        }
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<T, E> {
        self.st.borrow_mut().charge(8)?;
        match i32::try_from(v) {
            Ok(v) => Ok((self.decode)(v)),
            Err(_) => Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::MalformedSpanField(self.field))),
        }
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<T, E> {
        self.st.borrow_mut().charge(1)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<T, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<T, E> {
        self.st.borrow_mut().charge(value.len() as u64)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<T, E> {
        self.st.borrow_mut().charge(1)?;
        Ok((self.decode)(0))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<T, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::MalformedSpanField(self.field)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<T, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
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

    reject_non_null_scalars_at!(ShapeSite::Status);
    reject_container_at!(seq, ShapeSite::Status);

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        Ok(StatusObservation::Absent)
    }

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
                        depth: self.depth + 1,
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
