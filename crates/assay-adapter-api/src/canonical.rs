use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Serialize a JSON value into deterministic canonical bytes.
#[must_use]
pub fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write_canonical(value, &mut out);
    out
}

/// Serialize any serde value into canonical JSON bytes.
#[must_use]
pub fn canonical_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    let value = serde_json::to_value(value).expect("value must serialize to JSON");
    canonical_json_bytes(&value)
}

/// Compute a SHA-256 digest over canonical JSON bytes.
#[must_use]
pub fn digest_canonical_json<T: Serialize>(value: &T) -> String {
    let bytes = canonical_bytes(value);
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn write_canonical(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(boolean) => out.extend_from_slice(if *boolean { b"true" } else { b"false" }),
        Value::Number(number) => out.extend_from_slice(number.to_string().as_bytes()),
        Value::String(string) => out.extend_from_slice(
            serde_json::to_string(string)
                .expect("string escaping must succeed")
                .as_bytes(),
        ),
        Value::Array(array) => {
            out.push(b'[');
            for (index, item) in array.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write_canonical(item, out);
            }
            out.push(b']');
        }
        Value::Object(object) => {
            out.push(b'{');
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(
                    serde_json::to_string(key)
                        .expect("key escaping must succeed")
                        .as_bytes(),
                );
                out.push(b':');
                write_canonical(&object[*key], out);
            }
            out.push(b'}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_object_key_order_independent() {
        let first = json!({"b": 1, "a": 2});
        let second = json!({"a": 2, "b": 1});

        assert_eq!(canonical_json_bytes(&first), canonical_json_bytes(&second));
        assert_eq!(
            digest_canonical_json(&first),
            digest_canonical_json(&second)
        );
    }

    #[test]
    fn canonical_nested_object_key_order_independent() {
        let first = json!({
            "outer": {"z": true, "a": [3, {"b": 2, "a": 1}]},
            "name": "adapter"
        });
        let second = json!({
            "name": "adapter",
            "outer": {"a": [3, {"a": 1, "b": 2}], "z": true}
        });

        assert_eq!(canonical_json_bytes(&first), canonical_json_bytes(&second));
        assert_eq!(
            digest_canonical_json(&first),
            digest_canonical_json(&second)
        );
    }
}
