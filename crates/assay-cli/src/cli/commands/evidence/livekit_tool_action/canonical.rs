use super::constants::MAX_REF_CHARS;
use super::validate::optional_bounded_string;
use anyhow::{bail, Result};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub(super) enum HashOrRef {
    Hash(String),
    Ref(String),
    Absent,
}

pub(super) fn hash_or_ref(
    record: &Map<String, Value>,
    raw_key: &str,
    ref_key: &str,
    field_context: &str,
    document_number: usize,
) -> Result<HashOrRef> {
    if record.contains_key(raw_key) && record.contains_key(ref_key) {
        bail!("document {document_number} {field_context}: {raw_key} and {ref_key} must not both be present");
    }
    if let Some(raw) = record.get(raw_key) {
        return Ok(HashOrRef::Hash(sha256_json_value(raw)?));
    }
    if let Some(reference) = optional_bounded_string(
        record.get(ref_key),
        &format!("{field_context}.{ref_key}"),
        MAX_REF_CHARS,
        document_number,
    )? {
        return Ok(HashOrRef::Ref(reference));
    }
    Ok(HashOrRef::Absent)
}

fn sha256_json_value(value: &Value) -> Result<String> {
    let canonical = canonical_json(value)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn canonical_json(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok("null".to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::String(value) => Ok(serde_json::to_string(value)?),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                return Ok(value.to_string());
            }
            if let Some(value) = number.as_u64() {
                return Ok(value.to_string());
            }
            let Some(value) = number.as_f64() else {
                bail!("unsupported JSON number in canonical JSON");
            };
            if !value.is_finite() {
                bail!("non-finite floats are not valid in canonical JSON");
            }
            if value.fract() != 0.0 {
                bail!("non-integer floats are not valid in LiveKit tool-action canonical JSON");
            }
            Ok(format!("{value:.0}"))
        }
        Value::Array(values) => {
            let items = values
                .iter()
                .map(canonical_json)
                .collect::<Result<Vec<_>>>()?;
            Ok(format!("[{}]", items.join(",")))
        }
        Value::Object(map) => {
            let mut parts = Vec::with_capacity(map.len());
            for key in map.keys().collect::<BTreeSet<_>>() {
                let key_json = serde_json::to_string(key)?;
                let value_json = canonical_json(map.get(key).unwrap())?;
                parts.push(format!("{key_json}:{value_json}"));
            }
            Ok(format!("{{{}}}", parts.join(",")))
        }
    }
}
