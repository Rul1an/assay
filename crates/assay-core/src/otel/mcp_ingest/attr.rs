//! Bounded decoding of OTLP attribute lists and `AnyValue` trees.
//!
//! An attribute list is where the hostile corpus concentrates: oversized values, duplicate keys,
//! conflicting `AnyValue` members, and deep `kvlistValue`/`arrayValue` nesting all live here.
//! Every entry is charged against the per-list count ceiling before its content is decoded,
//! every key against the key-byte ceiling, and every value subtree against the per-value byte
//! budget — in addition to the document-global decoded-byte and depth ceilings from
//! [`super::decode`].

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};

use super::decode::{
    reject_container_at, reject_scalars_at, DecodedValue, KeySeed, Members, SkipSeed, SpanAttrs, St,
};
use super::limits::{OtlpIngestError, OtlpLimitDimension, ShapeSite};

/// Charge decoded content bytes inside one attribute value against its per-value budget. The
/// charge model matches the document-global one (string length, 8 per number, 1 per bool/null),
/// applied to content only: structural member names like `stringValue` or `values` are schema
/// vocabulary, not attacker content, and are charged solely to the global decoded ceiling.
fn charge_value<E: de::Error>(st: St<'_>, budget: &mut u64, bytes: u64) -> Result<(), E> {
    *budget = budget.saturating_add(bytes);
    let mut state = st.borrow_mut();
    if *budget > state.limits.max_attribute_value_bytes {
        let max = state.limits.max_attribute_value_bytes;
        return Err(state.limit(OtlpLimitDimension::AttributeValueBytes, max));
    }
    Ok(())
}

/// An `attributes` list. In extract mode (span attributes) recognized MCP values are applied to
/// the span accumulator; otherwise (resource attributes) entries are validated and discarded.
pub(super) struct AttributeListSeed<'a, 'v> {
    pub(super) st: St<'a>,
    pub(super) depth: u64,
    pub(super) extract: Option<&'v mut SpanAttrs>,
}

impl<'de> DeserializeSeed<'de> for AttributeListSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for AttributeListSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an attribute list")
    }

    reject_scalars_at!(ShapeSite::AttributeList);
    reject_container_at!(map, ShapeSite::AttributeList);

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut extract = self.extract;
        let mut count: u64 = 0;
        let mut keys = std::collections::HashSet::new();
        while let Some((key, value)) = seq.next_element_seed(AttributeEntrySeed {
            st: self.st,
            depth: self.depth + 1,
            count: &mut count,
        })? {
            // The retained key set is transient and bounded by the count and key-byte ceilings
            // already charged; it never enters an error or the output.
            if !keys.insert(key.clone()) {
                return Err(self
                    .st
                    .borrow_mut()
                    .fail(OtlpIngestError::DuplicateAttributeKey));
            }
            if let Some(attrs) = extract.as_deref_mut() {
                attrs.apply(self.st, &key, value)?;
            }
        }
        Ok(())
    }
}

/// One `{"key": ..., "value": ...}` entry. The list-count ceiling is charged on entry, before
/// any content is decoded; members may arrive in either order.
struct AttributeEntrySeed<'a, 'c> {
    st: St<'a>,
    depth: u64,
    count: &'c mut u64,
}

impl<'de> DeserializeSeed<'de> for AttributeEntrySeed<'_, '_> {
    type Value = (String, DecodedValue);
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for AttributeEntrySeed<'_, '_> {
    type Value = (String, DecodedValue);

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an attribute entry object")
    }

    reject_scalars_at!(ShapeSite::AttributeEntry);
    reject_container_at!(seq, ShapeSite::AttributeEntry);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        {
            let mut state = self.st.borrow_mut();
            *self.count += 1;
            if *self.count > state.limits.max_attribute_count {
                let max = state.limits.max_attribute_count;
                return Err(state.limit(OtlpLimitDimension::AttributeCount, max));
            }
            state.enter(self.depth)?;
        }
        let mut members = Members::new(ShapeSite::AttributeEntry);
        let mut key: Option<String> = None;
        let mut value: Option<DecodedValue> = None;
        let mut budget: u64 = 0;
        while let Some(member) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &member)?;
            match member.as_str() {
                "key" => key = Some(map.next_value_seed(AttrKeySeed { st: self.st })?),
                "value" => {
                    value = Some(map.next_value_seed(AnyValueSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: &mut budget,
                    })?);
                }
                _ => map.next_value_seed(SkipSeed {
                    st: self.st,
                    depth: self.depth + 1,
                })?,
            }
        }
        match (key, value) {
            (Some(key), Some(value)) => Ok((key, value)),
            _ => Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry))),
        }
    }
}

/// The `key` member of an attribute entry: the one string with its own dedicated byte ceiling,
/// checked against the borrowed slice **before** the key is allocated, so an oversized key is
/// never retained even transiently. Any non-string shape is a typed entry fault.
struct AttrKeySeed<'a> {
    st: St<'a>,
}

impl<'de> DeserializeSeed<'de> for AttrKeySeed<'_> {
    type Value = String;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<String, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for AttrKeySeed<'_> {
    type Value = String;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an attribute key string")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<String, E> {
        let mut state = self.st.borrow_mut();
        state.charge(s.len() as u64)?;
        let max = state.limits.max_attribute_key_bytes;
        if s.len() as u64 > max {
            return Err(state.limit(OtlpLimitDimension::AttributeKeyBytes, max));
        }
        drop(state);
        Ok(s.to_owned())
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
}

/// A proto `AnyValue` object: exactly one recognized value member. A second member — duplicate
/// or different — is a conflict; an unrecognized member is a shape fault.
struct AnyValueSeed<'a, 'b> {
    st: St<'a>,
    depth: u64,
    budget: &'b mut u64,
}

impl<'de> DeserializeSeed<'de> for AnyValueSeed<'_, '_> {
    type Value = DecodedValue;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for AnyValueSeed<'_, '_> {
    type Value = DecodedValue;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an AnyValue object")
    }

    reject_scalars_at!(ShapeSite::AttributeValue);
    reject_container_at!(seq, ShapeSite::AttributeValue);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut decoded: Option<DecodedValue> = None;
        while let Some(member) = map.next_key_seed(KeySeed { st: self.st })? {
            if decoded.is_some() {
                return Err(self
                    .st
                    .borrow_mut()
                    .fail(OtlpIngestError::ConflictingAttributeValue));
            }
            decoded = Some(match member.as_str() {
                "stringValue" => {
                    let s = map.next_value_seed(ValueStrSeed {
                        st: self.st,
                        budget: self.budget,
                    })?;
                    DecodedValue::Str(s)
                }
                "intValue" => {
                    let i = map.next_value_seed(ValueIntSeed {
                        st: self.st,
                        budget: self.budget,
                    })?;
                    DecodedValue::Int(i)
                }
                "doubleValue" => {
                    map.next_value_seed(ValueScalarSeed {
                        st: self.st,
                        budget: self.budget,
                        kind: ScalarKind::Number,
                    })?;
                    DecodedValue::Other
                }
                "boolValue" => {
                    map.next_value_seed(ValueScalarSeed {
                        st: self.st,
                        budget: self.budget,
                        kind: ScalarKind::Bool,
                    })?;
                    DecodedValue::Other
                }
                "bytesValue" => {
                    map.next_value_seed(ValueScalarSeed {
                        st: self.st,
                        budget: self.budget,
                        kind: ScalarKind::Base64,
                    })?;
                    DecodedValue::Other
                }
                "arrayValue" => {
                    map.next_value_seed(ValueListSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                        kind: ListKind::Array,
                    })?;
                    DecodedValue::Other
                }
                "kvlistValue" => {
                    map.next_value_seed(ValueListSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                        kind: ListKind::Kv,
                    })?;
                    DecodedValue::Other
                }
                _ => {
                    return Err(self
                        .st
                        .borrow_mut()
                        .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
                }
            });
        }
        // An empty AnyValue object carries no value at all; failing closed beats inventing one.
        decoded.ok_or_else(|| {
            self.st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue))
        })
    }
}

/// A `stringValue`: charged to both the global and the per-value ceilings before retention.
struct ValueStrSeed<'a, 'b> {
    st: St<'a>,
    budget: &'b mut u64,
}

impl<'de> DeserializeSeed<'de> for ValueStrSeed<'_, '_> {
    type Value = String;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<String, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueStrSeed<'_, '_> {
    type Value = String;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a string value")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<String, E> {
        self.st.borrow_mut().charge(s.len() as u64)?;
        charge_value(self.st, self.budget, s.len() as u64)?;
        Ok(s.to_owned())
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<String, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
}

/// An `intValue`. Proto3 JSON encodes int64 as a decimal string; a raw JSON integer is also
/// accepted, matching lenient upstream decoders. Anything else is a shape fault.
struct ValueIntSeed<'a, 'b> {
    st: St<'a>,
    budget: &'b mut u64,
}

impl<'de> DeserializeSeed<'de> for ValueIntSeed<'_, '_> {
    type Value = i64;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<i64, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueIntSeed<'_, '_> {
    type Value = i64;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an int64 value")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<i64, E> {
        // A string-encoded int64 is a JSON string: it charges its observed UTF-8 length under
        // the shared charge model, never a fixed numeric width, and is charged before parsing.
        self.st.borrow_mut().charge(s.len() as u64)?;
        charge_value(self.st, self.budget, s.len() as u64)?;
        s.parse::<i64>().map_err(|_| {
            self.st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue))
        })
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)?;
        Ok(v)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)?;
        i64::try_from(v).map_err(|_| {
            self.st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue))
        })
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<i64, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<i64, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<i64, E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<i64, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<i64, A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
}

enum ScalarKind {
    /// `doubleValue`: a JSON number.
    Number,
    /// `boolValue`: a JSON boolean.
    Bool,
    /// `bytesValue`: base64 text, charged by its encoded length and never decoded.
    Base64,
}

/// A non-extracted scalar `AnyValue` member: validated for shape and charged, never retained.
struct ValueScalarSeed<'a, 'b> {
    st: St<'a>,
    budget: &'b mut u64,
    kind: ScalarKind,
}

impl<'de> DeserializeSeed<'de> for ValueScalarSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueScalarSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a scalar AnyValue member")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        if matches!(self.kind, ScalarKind::Bool) {
            self.st.borrow_mut().charge(1)?;
            charge_value(self.st, self.budget, 1)
        } else {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
        }
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        if matches!(self.kind, ScalarKind::Number) {
            self.st.borrow_mut().charge(8)?;
            charge_value(self.st, self.budget, 8)
        } else {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
        }
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<(), E> {
        self.visit_f64(v as f64)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<(), E> {
        self.visit_f64(v as f64)
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<(), E> {
        if matches!(self.kind, ScalarKind::Base64) {
            self.st.borrow_mut().charge(s.len() as u64)?;
            charge_value(self.st, self.budget, s.len() as u64)
        } else {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
        }
    }

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<(), A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<(), A::Error> {
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
}

enum ListKind {
    /// `arrayValue`: `{"values": [AnyValue...]}`.
    Array,
    /// `kvlistValue`: `{"values": [{"key": ..., "value": AnyValue}...]}`.
    Kv,
}

/// The wrapper object of an `arrayValue` or `kvlistValue`, sharing the per-value budget with
/// its parent so nested content aggregates against one ceiling.
struct ValueListSeed<'a, 'b> {
    st: St<'a>,
    depth: u64,
    budget: &'b mut u64,
    kind: ListKind,
}

impl<'de> DeserializeSeed<'de> for ValueListSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueListSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an AnyValue list wrapper")
    }

    reject_scalars_at!(ShapeSite::AttributeValue);
    reject_container_at!(seq, ShapeSite::AttributeValue);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::AttributeValue);
        while let Some(member) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &member)?;
            match member.as_str() {
                "values" => map.next_value_seed(ValueElementsSeed {
                    st: self.st,
                    depth: self.depth + 1,
                    budget: self.budget,
                    kind: &self.kind,
                })?,
                _ => {
                    return Err(self
                        .st
                        .borrow_mut()
                        .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
                }
            }
        }
        Ok(())
    }
}

/// The `values` array inside an `arrayValue` or `kvlistValue`.
struct ValueElementsSeed<'a, 'b> {
    st: St<'a>,
    depth: u64,
    budget: &'b mut u64,
    kind: &'b ListKind,
}

impl<'de> DeserializeSeed<'de> for ValueElementsSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueElementsSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an AnyValue values array")
    }

    reject_scalars_at!(ShapeSite::AttributeValue);
    reject_container_at!(map, ShapeSite::AttributeValue);

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        match self.kind {
            ListKind::Array => {
                while seq
                    .next_element_seed(AnyValueSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                    })?
                    .is_some()
                {}
            }
            ListKind::Kv => {
                while seq
                    .next_element_seed(KvEntrySeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                    })?
                    .is_some()
                {}
            }
        }
        Ok(())
    }
}

/// One `{"key": ..., "value": AnyValue}` entry inside a `kvlistValue`. Nested keys are content,
/// so they charge the per-value budget as well as the global ceiling.
struct KvEntrySeed<'a, 'b> {
    st: St<'a>,
    depth: u64,
    budget: &'b mut u64,
}

impl<'de> DeserializeSeed<'de> for KvEntrySeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for KvEntrySeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a kvlist entry object")
    }

    reject_scalars_at!(ShapeSite::AttributeValue);
    reject_container_at!(seq, ShapeSite::AttributeValue);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::AttributeValue);
        while let Some(member) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &member)?;
            match member.as_str() {
                "key" => {
                    let k = map.next_value_seed(KeySeed { st: self.st })?;
                    charge_value(self.st, self.budget, k.len() as u64)?;
                }
                "value" => {
                    map.next_value_seed(AnyValueSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                    })?;
                }
                _ => {
                    return Err(self
                        .st
                        .borrow_mut()
                        .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
                }
            }
        }
        Ok(())
    }
}
