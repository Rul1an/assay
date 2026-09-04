use super::schema::TruncationMeta;
use crate::json_pointer::append_segment;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Stage-local ingest ceiling for individual string fields on the trace upgrade
/// path. This bound applies when V1/V2 events are read through `StreamUpgrader`
/// (and any direct callers of these helpers). It is not a global evidence-bundle
/// limit, a completeness claim, or a raiseable public contract.
const MAX_STRING_LEN: usize = 4096;
const TRUNCATED_MSG: &str = "...[TRUNCATED]";

/// Truncate `s` to a UTF-8 byte budget, appending [`TRUNCATED_MSG`].
///
/// The keep budget is measured in UTF-8 bytes (not Unicode scalar counts). The
/// cut lands on a char boundary at-or-before `MAX_STRING_LEN - TRUNCATED_MSG.len()`,
/// so the emitted string is always `<= MAX_STRING_LEN` bytes.
fn truncate_string_to_byte_budget(s: &str) -> String {
    let keep = MAX_STRING_LEN.saturating_sub(TRUNCATED_MSG.len());
    let mut end = keep.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = String::with_capacity(end + TRUNCATED_MSG.len());
    out.push_str(&s[..end]);
    out.push_str(TRUNCATED_MSG);
    debug_assert!(out.len() <= MAX_STRING_LEN);
    out
}

/// Truncates a value (recursively for JSON) and returns metadata for any changes.
///
/// `path` is an absolute RFC 6901 JSON Pointer to the value being walked
/// (e.g. `"/args"` or `"/meta"`), or a single unescaped root segment name such
/// as `"content"` which is normalized to `"/content"`.
#[must_use]
pub fn truncate_value_with_provenance(v: &mut Value, path: &str) -> Vec<TruncationMeta> {
    let path = normalize_root_pointer(path);
    truncate_value_at(v, &path)
}

fn normalize_root_pointer(path: &str) -> String {
    if path.is_empty() || path.starts_with('/') {
        path.to_string()
    } else {
        append_segment("", path)
    }
}

fn truncate_value_at(v: &mut Value, path: &str) -> Vec<TruncationMeta> {
    let mut metas = Vec::new();

    match v {
        Value::String(s) if s.len() > MAX_STRING_LEN => {
            let original_len = s.len();
            let hash = hex::encode(Sha256::digest(s.as_bytes()));
            let new_s = truncate_string_to_byte_budget(s);
            let kept_len = new_s.len();
            *s = new_s;

            metas.push(TruncationMeta {
                field: path.to_string(),
                original_len,
                kept_len,
                sha256: hash,
                strategy: "head".to_string(),
            });
        }
        Value::Array(arr) => {
            for (i, item) in arr.iter_mut().enumerate() {
                let sub_path = append_segment(path, &i.to_string());
                metas.extend(truncate_value_at(item, &sub_path));
            }
        }
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                let sub_path = append_segment(path, k);
                metas.extend(truncate_value_at(val, &sub_path));
            }
        }
        _ => {}
    }

    metas
}

/// Helper for pure string fields (e.g. `Step.content`).
///
/// `field_name` is normalized to an absolute RFC 6901 pointer the same way as
/// [`truncate_value_with_provenance`].
#[must_use]
pub fn truncate_string(s: &mut String, field_name: &str) -> Option<TruncationMeta> {
    if s.len() > MAX_STRING_LEN {
        let original_len = s.len();
        let hash = hex::encode(Sha256::digest(s.as_bytes()));
        let new_s = truncate_string_to_byte_budget(s);
        let kept_len = new_s.len();
        *s = new_s;

        Some(TruncationMeta {
            field: normalize_root_pointer(field_name),
            original_len,
            kept_len,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Four-byte code point used to expose char-count vs UTF-8 byte-budget mismatch.
    const MULTI: &str = "🔒"; // U+1F512, 4 UTF-8 bytes

    fn over_budget_multibyte() -> String {
        // 1500 × 4 = 6000 bytes > 4096; char-count keep (~4082) would emit ~16 KiB.
        MULTI.repeat(1500)
    }

    #[test]
    fn truncate_string_multibyte_stays_within_byte_budget() {
        let mut s = over_budget_multibyte();
        let meta = truncate_string(&mut s, "content").expect("must truncate");
        assert!(
            s.len() <= MAX_STRING_LEN,
            "emitted {} bytes exceeds stage-local {}-byte ingest ceiling",
            s.len(),
            MAX_STRING_LEN
        );
        assert_eq!(
            meta.kept_len,
            s.len(),
            "kept_len must equal emitted UTF-8 byte length"
        );
    }

    #[test]
    fn truncate_value_multibyte_stays_within_byte_budget_and_kept_len() {
        let mut v = json!(over_budget_multibyte());
        let metas = truncate_value_with_provenance(&mut v, "content");
        let emitted = v.as_str().expect("string value");
        assert!(
            emitted.len() <= MAX_STRING_LEN,
            "emitted {} bytes exceeds stage-local {}-byte ingest ceiling",
            emitted.len(),
            MAX_STRING_LEN
        );
        assert_eq!(metas.len(), 1);
        assert_eq!(
            metas[0].kept_len,
            emitted.len(),
            "kept_len must equal emitted UTF-8 byte length"
        );
    }

    #[test]
    fn literal_dotted_key_and_nested_path_emit_distinct_pointers() {
        let big = "x".repeat(MAX_STRING_LEN + 1);

        let mut literal = json!({ "gen_ai.prompt": big.clone() });
        let literal_metas = truncate_value_with_provenance(&mut literal, "meta");

        let mut nested = json!({ "gen_ai": { "prompt": big } });
        let nested_metas = truncate_value_with_provenance(&mut nested, "meta");

        assert_eq!(literal_metas.len(), 1);
        assert_eq!(nested_metas.len(), 1);
        assert_ne!(
            literal_metas[0].field, nested_metas[0].field,
            "literal dotted key must not alias nested path"
        );
        assert_eq!(literal_metas[0].field, "/meta/gen_ai.prompt");
        assert_eq!(nested_metas[0].field, "/meta/gen_ai/prompt");
    }

    #[test]
    fn rfc6901_escapes_tilde_and_slash_in_segment_order() {
        let big = "y".repeat(MAX_STRING_LEN + 1);
        let mut v = json!({
            "a~b": big.clone(),
            "c/d": big.clone(),
            "~/": big,
        });
        let metas = truncate_value_with_provenance(&mut v, "args");
        let fields: Vec<&str> = metas.iter().map(|m| m.field.as_str()).collect();
        assert!(
            fields.contains(&"/args/a~0b"),
            "tilde must escape as ~0; got {fields:?}"
        );
        assert!(
            fields.contains(&"/args/c~1d"),
            "slash must escape as ~1; got {fields:?}"
        );
        assert!(
            fields.contains(&"/args/~0~1"),
            "~ then / must escape as ~0~1; got {fields:?}"
        );
    }
}
