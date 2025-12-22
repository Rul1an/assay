use super::schema::TruncationMeta;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_STRING_LEN: usize = 4096;
const TRUNCATED_MSG: &str = "...[TRUNCATED]";

/// Truncates a value (recursively for JSON) and returns metadata for any changes.
/// "path" argument tracks the field name (e.g. "args.query" or "content").
pub fn truncate_value_with_provenance(v: &mut Value, path: &str) -> Vec<TruncationMeta> {
    let mut metas = Vec::new();

    match v {
        Value::String(s) => {
            if s.len() > MAX_STRING_LEN {
                let original_len = s.len();
                let hash = hex::encode(Sha256::digest(s.as_bytes()));

                let keep = MAX_STRING_LEN.saturating_sub(TRUNCATED_MSG.len());
                let mut new_s = String::with_capacity(MAX_STRING_LEN);
                let chars: String = s.chars().take(keep).collect();
                new_s.push_str(&chars);
                new_s.push_str(TRUNCATED_MSG);
                *s = new_s;

                metas.push(TruncationMeta {
                    field: path.to_string(),
                    original_len,
                    kept_len: keep + TRUNCATED_MSG.len(),
                    sha256: hash,
                    strategy: "head".to_string(),
                });
            }
        }
        Value::Array(arr) => {
            for (i, item) in arr.iter_mut().enumerate() {
                let sub_path = format!("{}[{}]", path, i);
                metas.extend(truncate_value_with_provenance(item, &sub_path));
            }
        }
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                let sub_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", path, k)
                };
                metas.extend(truncate_value_with_provenance(val, &sub_path));
            }
        }
        _ => {}
    }

    metas
}

// Helper for pure string (Step.content)
pub fn truncate_string(s: &mut String, field_name: &str) -> Option<TruncationMeta> {
    if s.len() > MAX_STRING_LEN {
        let original_len = s.len();
        let hash = hex::encode(Sha256::digest(s.as_bytes()));

        let keep = MAX_STRING_LEN.saturating_sub(TRUNCATED_MSG.len());
        let mut new_s = String::with_capacity(MAX_STRING_LEN);
        let chars: String = s.chars().take(keep).collect();
        new_s.push_str(&chars);
        new_s.push_str(TRUNCATED_MSG);
        *s = new_s;

        Some(TruncationMeta {
            field: field_name.to_string(),
            original_len,
            kept_len: s.len(),
            sha256: hash,
            strategy: "head".to_string(),
        })
    } else {
        None
    }
}

pub fn compute_sha256(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.as_bytes().to_vec(),
        _ => serde_json::to_vec(v).unwrap_or_default(),
    };
    hex::encode(Sha256::digest(&s))
}

pub fn compute_sha256_str(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}
