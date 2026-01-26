use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Cryptographic identity of a tool based on its name, server, and schema.
/// This prevents "Tool Poisoning" where an attacker modifies a tool definition
/// to inject different instructions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ToolIdentity {
    pub server_id: String,
    pub tool_name: String,
    /// Hash of the full input schema (JSON)
    pub schema_hash: String,
    /// Hash of the description and metadata
    pub meta_hash: String,
}

impl ToolIdentity {
    pub fn new(
        server_id: &str,
        tool_name: &str,
        schema: &Option<serde_json::Value>,
        description: &Option<String>,
    ) -> Self {
        let schema_hash = compute_json_hash(schema);
        let meta_hash = compute_string_hash(description.as_deref().unwrap_or(""));

        Self {
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            schema_hash,
            meta_hash,
        }
    }

    /// Returns a short fingerprint for display/logging.
    pub fn fingerprint(&self) -> String {
        format!(
            "{}:{}:{}",
            self.server_id,
            self.tool_name,
            &self.schema_hash[0..8]
        )
    }
}

fn compute_json_hash(val: &Option<serde_json::Value>) -> String {
    let mut hasher = Sha256::new();
    if let Some(v) = val {
        // Deterministic serialization is key
        let s = serde_json::to_string(v).unwrap_or_default();
        hasher.update(s.as_bytes());
    } else {
        hasher.update(b"null");
    }
    format!("{:x}", hasher.finalize())
}

fn compute_string_hash(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}
