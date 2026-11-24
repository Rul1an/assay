use crate::model::LlmResponse;
use sha2::Digest;
use std::collections::HashMap;

/// The trace identity: one digest over every replayed episode, including its `meta`, which carries
/// the ordered tool calls a check reads. The replay cache key and the `--incremental` fingerprint
/// both include it, so a change to any call is a different trace to both.
///
/// Each field is length-prefixed, so two traces cannot share a digest by moving bytes from one
/// field into the next.
pub(crate) fn compute_trace_fingerprint(traces: &HashMap<String, LlmResponse>) -> String {
    fn field(hasher: &mut sha2::Sha256, bytes: &[u8]) {
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    let mut keys: Vec<&String> = traces.keys().collect();
    keys.sort();
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"assay.trace-identity.v2\0");
    for k in keys {
        field(&mut hasher, k.as_bytes());
        if let Some(v) = traces.get(k) {
            field(&mut hasher, v.text.as_bytes());
            field(&mut hasher, v.model.as_bytes());
            // A `Value` serializes the same way every time it is built from the same file.
            let meta = serde_json::to_vec(&v.meta).unwrap_or_default();
            field(&mut hasher, &meta);
        }
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The same bytes split differently over two adjacent fields are two different traces. Without
    // the length prefixes both splits hash the same byte stream.
    #[test]
    fn moving_bytes_between_adjacent_fields_changes_the_identity() {
        let trace = |text: &str, model: &str| {
            HashMap::from([(
                "p".to_string(),
                LlmResponse {
                    text: text.to_string(),
                    model: model.to_string(),
                    ..Default::default()
                },
            )])
        };
        assert_ne!(
            compute_trace_fingerprint(&trace("xy", "z")),
            compute_trace_fingerprint(&trace("x", "yz"))
        );
    }
}
