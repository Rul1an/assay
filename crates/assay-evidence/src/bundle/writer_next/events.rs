use crate::crypto::jcs;
use crate::types::EvidenceEvent;
use anyhow::Result;
use sha2::{Digest, Sha256};

pub(crate) fn serialize_events_ndjson(events: &[EvidenceEvent]) -> Result<Vec<u8>> {
    let mut events_bytes = Vec::new();
    for event in events {
        events_bytes.extend_from_slice(&jcs::to_vec(event)?);
        events_bytes.push(b'\n');
    }
    Ok(events_bytes)
}

pub(crate) fn sha256_prefixed(data: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(data)))
}

pub(crate) fn normalize_hash(hash: &str) -> String {
    if hash.starts_with("sha256:") {
        hash.to_string()
    } else {
        format!("sha256:{}", hash)
    }
}
