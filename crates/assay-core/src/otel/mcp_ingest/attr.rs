//! Bounded decoding of OTLP attribute lists and `AnyValue` trees.
//!
//! An attribute list is where the hostile corpus concentrates: oversized values, duplicate keys,
//! conflicting `AnyValue` members, and deep `kvlistValue`/`arrayValue` nesting all live here.
//! Every entry is charged against the per-list count ceiling before its content is decoded,
//! every key against the key-byte ceiling, and every value subtree against the per-value byte
//! budget — in addition to the document-global decoded-byte and depth ceilings from
//! [`super::decode`].

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};

use base64::Engine as _;

use super::decode::{
    reject_container_at, reject_non_null_scalars_at, reject_scalars_at, DecodedValue, KeySeed,
    Members, SkipSeed, SpanAttrs, St,
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

fn reject_value_scalar<E: de::Error, T>(st: St<'_>, budget: &mut u64, bytes: u64) -> Result<T, E> {
    st.borrow_mut().charge(bytes)?;
    charge_value(st, budget, bytes)?;
    Err(st
        .borrow_mut()
        .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
}

macro_rules! reject_value_non_null_scalars {
    () => {
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            reject_value_scalar(self.st, self.budget, 1)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            reject_value_scalar(self.st, self.budget, 8)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            reject_value_scalar(self.st, self.budget, 8)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            reject_value_scalar(self.st, self.budget, 8)
        }
        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            reject_value_scalar(self.st, self.budget, value.len() as u64)
        }
    };
}

macro_rules! reject_value_scalars {
    () => {
        reject_value_non_null_scalars!();
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            reject_value_scalar(self.st, self.budget, 1)
        }
    };
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

    reject_non_null_scalars_at!(ShapeSite::AttributeList);
    reject_container_at!(map, ShapeSite::AttributeList);

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.st.borrow_mut().charge(1)?;
        Ok(())
    }

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
        let mut state = self.st.borrow_mut();
        *self.count += 1;
        if *self.count > state.limits.max_attribute_count {
            let max = state.limits.max_attribute_count;
            return Err(state.limit(OtlpLimitDimension::AttributeCount, max));
        }
        drop(state);
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
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::AttributeEntry);
        let mut key: Option<String> = None;
        let mut value: Option<DecodedValue> = None;
        let mut budget: u64 = 0;
        while let Some(member) = map.next_key_seed(KeySeed { st: self.st })? {
            members.admit(self.st, &member)?;
            match member.as_str() {
                "key" => {
                    key = Some(map.next_value_seed(AttrKeySeed {
                        st: self.st,
                        depth: self.depth + 1,
                    })?)
                }
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
/// checked before copying it into the domain observation. Serde may hold parser scratch for the
/// token first; that allocation is bounded by the source ceiling. Any non-string shape is a typed
/// entry fault.
struct AttrKeySeed<'a> {
    st: St<'a>,
    depth: u64,
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
        self.st.borrow_mut().charge(1)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<String, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<String, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<String, E> {
        self.st.borrow_mut().charge(8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<String, E> {
        self.st.borrow_mut().charge(1)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<String, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeEntry)))
    }
}

/// A proto `AnyValue` object. Exactly one recognized oneof member may be present. Empty values and
/// unknown protobuf JSON fields are forward-compatible and decode as `Other`; a second recognized
/// member is a conflict, while duplicate unknown fields still fail closed.
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

    reject_container_at!(seq, ShapeSite::AttributeValue);

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(value.len() as u64)?;
        charge_value(self.st, self.budget, value.len() as u64)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)?;
        Ok(DecodedValue::Other)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut decoded: Option<DecodedValue> = None;
        let mut members = Members::new(ShapeSite::AttributeValue);
        while let Some(member) = map.next_key_seed(KeySeed { st: self.st })? {
            let recognized = matches!(
                member.as_str(),
                "stringValue"
                    | "intValue"
                    | "doubleValue"
                    | "boolValue"
                    | "bytesValue"
                    | "arrayValue"
                    | "kvlistValue"
            );
            if !recognized {
                // Unknown names are attacker content. Charge before duplicate detection so the
                // value ceiling remains the first boundary crossed by the next list member.
                charge_value(self.st, self.budget, member.len() as u64)?;
            }
            members.admit(self.st, &member)?;
            let value = match member.as_str() {
                "stringValue" => map
                    .next_value_seed(ValueStrSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                    })?
                    .map(DecodedValue::Str),
                "intValue" => map
                    .next_value_seed(ValueIntSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                    })?
                    .map(|_| DecodedValue::Other),
                "doubleValue" => map
                    .next_value_seed(ValueScalarSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                        kind: ScalarKind::Number,
                    })?
                    .map(|_| DecodedValue::Other),
                "boolValue" => map
                    .next_value_seed(ValueScalarSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                        kind: ScalarKind::Bool,
                    })?
                    .map(|_| DecodedValue::Other),
                "bytesValue" => map
                    .next_value_seed(ValueScalarSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                        kind: ScalarKind::Base64,
                    })?
                    .map(|_| DecodedValue::Other),
                "arrayValue" => map
                    .next_value_seed(ValueListSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                        kind: ListKind::Array,
                    })?
                    .map(|_| DecodedValue::Other),
                "kvlistValue" => map
                    .next_value_seed(ValueListSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                        kind: ListKind::Kv,
                    })?
                    .map(|_| DecodedValue::Other),
                _ => {
                    // OTLP protobuf JSON receivers ignore unknown fields. They still consume the
                    // same depth/global/value budgets and reject duplicate members.
                    map.next_value_seed(ValueSkipSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                    })?;
                    None
                }
            };
            if recognized {
                if let Some(value) = value {
                    if decoded.is_some() {
                        return Err(self
                            .st
                            .borrow_mut()
                            .fail(OtlpIngestError::ConflictingAttributeValue));
                    }
                    decoded = Some(value);
                }
            }
        }
        Ok(decoded.unwrap_or(DecodedValue::Other))
    }
}

/// Skip a forward-compatible value subtree without retention while charging both the document
/// and per-attribute budgets. Object member names are attacker-controlled here, so they count
/// toward the value budget as well as the document-global decoded budget.
struct ValueSkipSeed<'a, 'b> {
    st: St<'a>,
    depth: u64,
    budget: &'b mut u64,
}

impl<'de> DeserializeSeed<'de> for ValueSkipSeed<'_, '_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueSkipSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a forward-compatible attribute value")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<(), E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<(), E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
        let bytes = value.len() as u64;
        self.st.borrow_mut().charge(bytes)?;
        charge_value(self.st, self.budget, bytes)
    }
    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        while seq
            .next_element_seed(ValueSkipSeed {
                st: self.st,
                depth: self.depth + 1,
                budget: self.budget,
            })?
            .is_some()
        {}
        Ok(())
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::AttributeValue);
        while let Some(key) = map.next_key_seed(KeySeed { st: self.st })? {
            charge_value(self.st, self.budget, key.len() as u64)?;
            members.admit(self.st, &key)?;
            map.next_value_seed(ValueSkipSeed {
                st: self.st,
                depth: self.depth + 1,
                budget: self.budget,
            })?;
        }
        Ok(())
    }
}

/// A `stringValue`: charged to both the global and the per-value ceilings before retention.
struct ValueStrSeed<'a, 'b> {
    st: St<'a>,
    depth: u64,
    budget: &'b mut u64,
}

impl<'de> DeserializeSeed<'de> for ValueStrSeed<'_, '_> {
    type Value = Option<String>;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueStrSeed<'_, '_> {
    type Value = Option<String>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a string value")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(s.len() as u64)?;
        charge_value(self.st, self.budget, s.len() as u64)?;
        Ok(Some(s.to_owned()))
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        reject_value_scalar(self.st, self.budget, 1)
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        reject_value_scalar(self.st, self.budget, 8)
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        reject_value_scalar(self.st, self.budget, 8)
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        reject_value_scalar(self.st, self.budget, 8)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)?;
        Ok(None)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
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
    depth: u64,
    budget: &'b mut u64,
}

impl<'de> DeserializeSeed<'de> for ValueIntSeed<'_, '_> {
    type Value = Option<i64>;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueIntSeed<'_, '_> {
    type Value = Option<i64>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an int64 value")
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
        // A string-encoded int64 is a JSON string: it charges its observed UTF-8 length under
        // the shared charge model, never a fixed numeric width, and is charged before parsing.
        self.st.borrow_mut().charge(s.len() as u64)?;
        charge_value(self.st, self.budget, s.len() as u64)?;
        let parsed = parse_decimal_i64(s);
        parsed.map(Some).ok_or_else(|| {
            self.st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue))
        })
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)?;
        Ok(Some(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)?;
        i64::try_from(v).map(Some).map_err(|_| {
            self.st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue))
        })
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        reject_value_scalar(self.st, self.budget, 1)
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)?;
        integral_i64(value).map(Some).ok_or_else(|| {
            self.st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue))
        })
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)?;
        Ok(None)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
}

fn integral_i64(value: f64) -> Option<i64> {
    const I64_UPPER_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
    const I64_LOWER_INCLUSIVE: f64 = -9_223_372_036_854_775_808.0;
    (value.is_finite()
        && value.fract() == 0.0
        && (I64_LOWER_INCLUSIVE..I64_UPPER_EXCLUSIVE).contains(&value))
    .then_some(value as i64)
}

/// Parse ProtoJSON's quoted decimal/exponent form without routing through binary64. Floating-point
/// conversion can round a fractional or out-of-range decimal onto an integral boundary.
fn parse_decimal_i64(value: &str) -> Option<i64> {
    let (negative, unsigned) = match value.as_bytes().first() {
        Some(b'-') => (true, &value[1..]),
        Some(b'+') | None => return None,
        _ => (false, value),
    };
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(index) => {
            let exponent_text = &unsigned[index + 1..];
            if exponent_text.is_empty() || unsigned[index + 1..].contains(['e', 'E']) {
                return None;
            }
            let exponent = exponent_text.parse::<i64>().ok()?;
            (&unsigned[..index], exponent)
        }
        None => (unsigned, 0),
    };
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) if !integer.is_empty() && !fraction.is_empty() => {
            (integer, fraction)
        }
        Some(_) => return None,
        None if !mantissa.is_empty() => (mantissa, ""),
        None => return None,
    };
    if !integer.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }

    let mut digits = Vec::with_capacity(integer.len().saturating_add(fraction.len()));
    digits.extend(integer.bytes());
    digits.extend(fraction.bytes());
    let scale = exponent.checked_sub(i64::try_from(fraction.len()).ok()?)?;
    if scale < 0 {
        let remove = usize::try_from(scale.unsigned_abs()).ok()?;
        if remove > digits.len() {
            return digits.iter().all(|digit| *digit == b'0').then_some(0);
        }
        let split = digits.len() - remove;
        if !digits[split..].iter().all(|digit| *digit == b'0') {
            return None;
        }
        digits.truncate(split);
    }

    let first_nonzero = digits.iter().position(|digit| *digit != b'0');
    let Some(first_nonzero) = first_nonzero else {
        return Some(0);
    };
    let significant = &digits[first_nonzero..];
    let appended_zeros = usize::try_from(scale.max(0)).ok()?;
    let effective_len = significant.len().checked_add(appended_zeros)?;
    if effective_len > 19 {
        return None;
    }

    let mut magnitude = 0_u64;
    for digit in significant
        .iter()
        .copied()
        .chain(std::iter::repeat_n(b'0', appended_zeros))
    {
        magnitude = magnitude
            .checked_mul(10)?
            .checked_add(u64::from(digit - b'0'))?;
    }
    if negative {
        const MIN_MAGNITUDE: u64 = i64::MAX as u64 + 1;
        if magnitude == MIN_MAGNITUDE {
            Some(i64::MIN)
        } else if magnitude <= i64::MAX as u64 {
            Some(-(magnitude as i64))
        } else {
            None
        }
    } else {
        i64::try_from(magnitude).ok()
    }
}

enum ScalarKind {
    /// `doubleValue`: a JSON number.
    Number,
    /// `boolValue`: a JSON boolean.
    Bool,
    /// `bytesValue`: base64 text, charged by encoded length and decoded transiently for validation.
    Base64,
}

/// A non-extracted scalar `AnyValue` member: validated for shape and charged, never retained.
struct ValueScalarSeed<'a, 'b> {
    st: St<'a>,
    depth: u64,
    budget: &'b mut u64,
    kind: ScalarKind,
}

impl<'de> DeserializeSeed<'de> for ValueScalarSeed<'_, '_> {
    type Value = Option<()>;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueScalarSeed<'_, '_> {
    type Value = Option<()>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a scalar AnyValue member")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)?;
        if matches!(self.kind, ScalarKind::Bool) {
            Ok(Some(()))
        } else {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
        }
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(8)?;
        charge_value(self.st, self.budget, 8)?;
        if matches!(self.kind, ScalarKind::Number) {
            Ok(Some(()))
        } else {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
        }
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        self.visit_f64(v as f64)
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        self.visit_f64(v as f64)
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(s.len() as u64)?;
        charge_value(self.st, self.budget, s.len() as u64)?;
        let valid = match self.kind {
            ScalarKind::Number => {
                matches!(s, "NaN" | "Infinity" | "-Infinity") || s.parse::<f64>().is_ok()
            }
            ScalarKind::Base64 => valid_base64(s),
            ScalarKind::Bool => false,
        };
        if valid {
            Ok(Some(()))
        } else {
            Err(self
                .st
                .borrow_mut()
                .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
        }
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)?;
        Ok(None)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
    fn visit_map<A: MapAccess<'de>>(self, _: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        Err(self
            .st
            .borrow_mut()
            .fail(OtlpIngestError::UnexpectedShape(ShapeSite::AttributeValue)))
    }
}

fn valid_base64(value: &str) -> bool {
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};

    let mut scratch = vec![0; base64::decoded_len_estimate(value.len())];
    [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD]
        .into_iter()
        .any(|engine| engine.decode_slice(value, &mut scratch).is_ok())
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
    type Value = Option<()>;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for ValueListSeed<'_, '_> {
    type Value = Option<()>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an AnyValue list wrapper")
    }

    reject_container_at!(seq, ShapeSite::AttributeValue);

    reject_value_non_null_scalars!();

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::AttributeValue);
        while let Some(member) = map.next_key_seed(KeySeed { st: self.st })? {
            match member.as_str() {
                "values" => {
                    members.admit(self.st, &member)?;
                    map.next_value_seed(ValueElementsSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                        kind: &self.kind,
                    })?;
                }
                _ => {
                    charge_value(self.st, self.budget, member.len() as u64)?;
                    members.admit(self.st, &member)?;
                    map.next_value_seed(ValueSkipSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                    })?;
                }
            }
        }
        Ok(Some(()))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)?;
        Ok(None)
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

    reject_value_non_null_scalars!();
    reject_container_at!(map, ShapeSite::AttributeValue);

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        self.st.borrow_mut().charge(1)?;
        charge_value(self.st, self.budget, 1)?;
        Ok(())
    }

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

    reject_value_scalars!();
    reject_container_at!(seq, ShapeSite::AttributeValue);

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        self.st.borrow_mut().enter(self.depth)?;
        let mut members = Members::new(ShapeSite::AttributeValue);
        while let Some(member) = map.next_key_seed(KeySeed { st: self.st })? {
            match member.as_str() {
                "key" => {
                    members.admit(self.st, &member)?;
                    let k = map.next_value_seed(KeySeed { st: self.st })?;
                    charge_value(self.st, self.budget, k.len() as u64)?;
                }
                "value" => {
                    members.admit(self.st, &member)?;
                    map.next_value_seed(AnyValueSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                    })?;
                }
                _ => {
                    charge_value(self.st, self.budget, member.len() as u64)?;
                    members.admit(self.st, &member)?;
                    map.next_value_seed(ValueSkipSeed {
                        st: self.st,
                        depth: self.depth + 1,
                        budget: self.budget,
                    })?;
                }
            }
        }
        Ok(())
    }
}
