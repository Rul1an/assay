use std::prelude::v1::*;

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
pub struct UniqueValue(pub serde_json::Value);

impl UniqueValue {
    /// Interpret the checked representation with the consumer's serde_json features,
    /// including the private number representation used by arbitrary_precision.
    pub fn into_native_value(self) -> Result<serde_json::Value, serde_json::Error> {
        // With raw_value enabled, Serde would parse this member's string as new JSON,
        // after duplicate detection. Keep that second input out of the native decoder.
        fn contains_raw_marker(value: &serde_json::Value) -> bool {
            match value {
                serde_json::Value::Object(fields) => {
                    fields.contains_key("$serde_json::private::RawValue")
                        || fields.values().any(contains_raw_marker)
                }
                serde_json::Value::Array(items) => items.iter().any(contains_raw_marker),
                _ => false,
            }
        }
        if contains_raw_marker(&self.0) {
            return Err(serde::de::Error::custom("reserved Serde raw-value member"));
        }
        serde::Deserialize::deserialize(self.0)
    }
}

// The YAML document reader checks the shape underneath a YAML tag, then its typed
// decoder reads the original tagged value. Strict JSON callers keep rejecting enums.
pub(crate) struct StringKeyValue<const YAML_SHAPE: bool, const ROOT_EXTENSIONS: bool = false>(
    pub serde_json::Value,
);

impl<'de> serde::Deserialize<'de> for UniqueValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        StringKeyValue::<false>::deserialize(deserializer).map(|value| Self(value.0))
    }
}

impl<'de, const YAML_SHAPE: bool, const ROOT_EXTENSIONS: bool> serde::Deserialize<'de>
    for StringKeyValue<YAML_SHAPE, ROOT_EXTENSIONS>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<const YAML_SHAPE: bool, const ROOT_EXTENSIONS: bool>;

        impl<'de, const YAML_SHAPE: bool, const ROOT_EXTENSIONS: bool> serde::de::Visitor<'de>
            for Visitor<YAML_SHAPE, ROOT_EXTENSIONS>
        {
            type Value = StringKeyValue<YAML_SHAPE, ROOT_EXTENSIONS>;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("any JSON value with unique object members")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(StringKeyValue(serde_json::Value::Null))
            }
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(StringKeyValue(serde_json::Value::Null))
            }
            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                d: D,
            ) -> Result<Self::Value, D::Error> {
                <Self::Value as serde::Deserialize>::deserialize(d)
            }
            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
                Ok(StringKeyValue(v.into()))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
                Ok(StringKeyValue(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
                Ok(StringKeyValue(v.into()))
            }
            fn visit_i128<E: serde::de::Error>(self, v: i128) -> Result<Self::Value, E> {
                if YAML_SHAPE {
                    // Only the shape is used; the typed reader consumes the original scalar.
                    return self.visit_unit();
                }
                serde::Deserialize::deserialize(serde::de::value::I128Deserializer::new(v))
                    .map(StringKeyValue)
            }
            fn visit_u128<E: serde::de::Error>(self, v: u128) -> Result<Self::Value, E> {
                if YAML_SHAPE {
                    // Only the shape is used; the typed reader consumes the original scalar.
                    return self.visit_unit();
                }
                serde::Deserialize::deserialize(serde::de::value::U128Deserializer::new(v))
                    .map(StringKeyValue)
            }
            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E> {
                Ok(StringKeyValue(
                    serde_json::Number::from_f64(v).map_or(serde_json::Value::Null, Into::into),
                ))
            }
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
                Ok(StringKeyValue(v.into()))
            }

            fn visit_enum<A: serde::de::EnumAccess<'de>>(
                self,
                data: A,
            ) -> Result<Self::Value, A::Error> {
                if !YAML_SHAPE {
                    return Err(serde::de::Error::invalid_type(
                        serde::de::Unexpected::Enum,
                        &self,
                    ));
                }
                let (_, value) = data.variant::<String>()?;
                serde::de::VariantAccess::newtype_variant(value)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(StringKeyValue(v)) =
                    seq.next_element::<StringKeyValue<YAML_SHAPE>>()?
                {
                    out.push(v);
                }
                Ok(StringKeyValue(serde_json::Value::Array(out)))
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    // Membership is checked on the key alone, before `next_value` materializes
                    // whatever the duplicate carries. Reading it first means a hostile input pays
                    // for a value that is only going to be refused, and the value may be an
                    // arbitrarily large subtree.
                    if out.contains_key(&key) {
                        return Err(serde::de::Error::custom(if YAML_SHAPE {
                            "duplicate mapping key"
                        } else {
                            "JSON object contains a duplicate member"
                        }));
                    }
                    // Root extension contents follow YAML's native key rules. Keep a
                    // placeholder so repeated extension names are still rejected.
                    #[cfg(feature = "closed-doc")]
                    if ROOT_EXTENSIONS && crate::doc::is_extension_key(&key) {
                        map.next_value::<serde_yaml::Value>()?;
                        out.insert(key, serde_json::Value::Null);
                        continue;
                    }
                    let StringKeyValue(value) = map.next_value::<StringKeyValue<YAML_SHAPE>>()?;
                    out.insert(key, value);
                }
                Ok(StringKeyValue(serde_json::Value::Object(out)))
            }
        }

        deserializer.deserialize_any(Visitor::<YAML_SHAPE, ROOT_EXTENSIONS>)
    }
}
