use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Signature object (DSSE-compatible, v1.0.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// Schema version. MUST be 1
    pub version: u32,

    /// Algorithm. MUST be "ed25519" for v1
    pub algorithm: String,

    /// Payload type for type confusion prevention
    pub payload_type: String,

    /// Content-addressed identifier = mandate_id
    pub content_id: String,

    /// SHA256 of signed payload bytes (DSSE standard)
    pub signed_payload_digest: String,

    /// SHA-256 of SPKI public key
    pub key_id: String,

    /// Base64-encoded Ed25519 signature (with padding)
    pub signature: String,

    /// Signing timestamp (metadata only)
    pub signed_at: DateTime<Utc>,
}
