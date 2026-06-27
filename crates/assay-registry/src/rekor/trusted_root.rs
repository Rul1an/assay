use serde_json::Value;

use super::checkpoint::b64;

/// A pinned transparency-log: its raw Ed25519 verifier key, its full log id (the `logId.keyId`), and the
/// normalized host of its `baseUrl` (the operator-pinned origin), when present.
pub(super) struct PinnedTlog {
    pub(super) key: [u8; 32],
    pub(super) log_id: Vec<u8>,
    pub(super) origin: Option<String>,
}

/// Normalize a trusted-root `baseUrl` to a checkpoint origin host: drop the scheme and any trailing slash.
/// (The Rekor v2 checkpoint origin for the public/staging logs is the bare host.)
pub(super) fn normalize_origin(base_url: &str) -> String {
    base_url
        .strip_prefix("https://")
        .or_else(|| base_url.strip_prefix("http://"))
        .unwrap_or(base_url)
        .trim_end_matches('/')
        .to_string()
}

/// Extract pinned Ed25519 tlogs from a Sigstore trusted root. `publicKey.rawBytes` for `PKIX_ED25519` is
/// SPKI DER (44 bytes; raw key = trailing 32). `logId.keyId` is the log's identity. ECDSA tlogs (the old
/// v1 log) are ignored.
pub(super) fn pinned_tlogs(trusted_root: &Value) -> Vec<PinnedTlog> {
    let mut out = Vec::new();
    let Some(tlogs) = trusted_root.get("tlogs").and_then(Value::as_array) else {
        return out;
    };
    for t in tlogs {
        if t.pointer("/publicKey/keyDetails").and_then(Value::as_str) != Some("PKIX_ED25519") {
            continue;
        }
        let Some(raw) = t
            .pointer("/publicKey/rawBytes")
            .and_then(Value::as_str)
            .and_then(b64)
        else {
            continue;
        };
        let key: Option<[u8; 32]> = match raw.len() {
            44 => raw[12..44].try_into().ok(),
            32 => raw[..].try_into().ok(),
            _ => None,
        };
        let log_id = t
            .pointer("/logId/keyId")
            .and_then(Value::as_str)
            .and_then(b64);
        let origin = t
            .get("baseUrl")
            .and_then(Value::as_str)
            .map(normalize_origin);
        if let (Some(key), Some(log_id)) = (key, log_id) {
            out.push(PinnedTlog {
                key,
                log_id,
                origin,
            });
        }
    }
    out
}
