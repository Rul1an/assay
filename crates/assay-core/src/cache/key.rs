use sha2::{Digest, Sha256};

pub fn cache_key(model: &str, prompt: &str, fingerprint: &str, trace_hash: Option<&str>) -> String {
    let mut h = Sha256::new();
    h.update(model.as_bytes());
    h.update(b"\n");
    h.update(prompt.as_bytes());
    h.update(b"\n");
    h.update(fingerprint.as_bytes());
    if let Some(th) = trace_hash {
        h.update(b"\n");
        h.update(th.as_bytes());
    }
    format!("{:x}", h.finalize())
}
