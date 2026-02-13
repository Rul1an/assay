use crate::model::LlmResponse;
use sha2::Digest;
use std::collections::HashMap;

pub(crate) fn compute_trace_fingerprint(traces: &HashMap<String, LlmResponse>) -> String {
    let mut keys: Vec<&String> = traces.keys().collect();
    keys.sort();
    let mut hasher = sha2::Sha256::new();
    for k in keys {
        hasher.update(k.as_bytes());
        if let Some(v) = traces.get(k) {
            hasher.update(v.text.as_bytes());
            hasher.update(v.model.as_bytes());
        }
    }
    hex::encode(hasher.finalize())
}
