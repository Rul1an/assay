//! Strict JSON parsing for the canonical boundary.
//!
//! `serde_json::from_str::<Value>` silently collapses duplicate object keys (last value wins), which
//! would erase an ambiguity *before* canonicalization could reject it. RFC 8785 / I-JSON treat
//! duplicate keys as invalid. [`parse_strict`] fails closed on the first duplicate key, at any
//! nesting depth, so a caller never content-addresses a record whose keys were silently collapsed.

use std::fmt;

use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::Error;

/// A [`serde_json::Value`] parsed with duplicate object keys rejected at every depth.
struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictVisitor).map(StrictValue)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any valid JSON value with unique object keys")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Ok(Value::from(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(StrictVisitor)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut arr = Vec::new();
        while let Some(StrictValue(v)) = seq.next_element()? {
            arr.push(v);
        }
        Ok(Value::Array(arr))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        // serde_json yields every key in source order, including duplicates; the collapse only
        // happens in Value's own visitor. Checking the insert result here catches the duplicate.
        let mut out = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let StrictValue(val) = map.next_value()?;
            if out.insert(key.clone(), val).is_some() {
                return Err(de::Error::custom(format!("duplicate object key: {key}")));
            }
        }
        Ok(Value::Object(out))
    }
}

/// Parse `raw` JSON into a [`Value`], failing closed on any duplicate object key (at any depth).
///
/// Use this, not `serde_json::from_str::<Value>`, as the entry point for untrusted JSON that will be
/// content-addressed — otherwise duplicate keys are silently collapsed before they can be rejected.
///
/// ```
/// assert!(assay_canonical::parse_strict(r#"{"a":1,"a":2}"#).is_err());
/// assert!(assay_canonical::parse_strict(r#"{"a":1,"b":2}"#).is_ok());
/// ```
pub fn parse_strict(raw: &str) -> Result<Value, Error> {
    serde_json::from_str::<StrictValue>(raw)
        .map(|s| s.0)
        .map_err(|e| Error::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_top_level_duplicate_keys() {
        assert!(matches!(
            parse_strict(r#"{"a":1,"a":2}"#),
            Err(Error::Parse(_))
        ));
    }

    #[test]
    fn rejects_nested_duplicate_keys() {
        assert!(parse_strict(r#"{"outer":{"k":1,"k":2}}"#).is_err());
        assert!(parse_strict(r#"{"xs":[{"k":1,"k":2}]}"#).is_err());
    }

    #[test]
    fn accepts_unique_keys_and_preserves_structure() {
        let v = parse_strict(r#"{"b":1,"a":[1,2,3],"n":{"x":true}}"#).unwrap();
        assert_eq!(v, json!({"a":[1,2,3],"b":1,"n":{"x":true}}));
    }

    #[test]
    fn contrasts_with_the_silent_stdlib_collapse() {
        // Documents exactly why parse_strict exists: the stdlib path loses the duplicate.
        let collapsed: Value = serde_json::from_str(r#"{"a":1,"a":2}"#).unwrap();
        assert_eq!(collapsed, json!({"a":2}));
        assert!(parse_strict(r#"{"a":1,"a":2}"#).is_err());
    }
}
