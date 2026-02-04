use moka::sync::Cache;
use sha2::{Digest, Sha256};

// We need types for what we cache.
// For args_schema, we cache the compiled JSON Validator?
// jsonschema::Validator is Clone (0.40), Send+Sync.
// moka::sync::Cache requires Value to optionally be Clone if we use get().
// Arc<Validator> is Clone.

// For now, let's define placeholder types or use what we have.
#[derive(Debug, Clone)]
pub enum SequencePolicy {
    Legacy(Vec<String>),
    Rules(Vec<assay_core::model::SequenceRule>),
    V1_1(Box<assay_core::model::Policy>),
}
pub type ParsedSequencePolicy = std::sync::Arc<SequencePolicy>;

pub type CompiledBlocklist = std::sync::Arc<Vec<String>>;

pub struct PolicyCaches {
    pub sequence: Cache<String, ParsedSequencePolicy>,
    pub blocklist: Cache<String, CompiledBlocklist>,
}

impl PolicyCaches {
    pub fn new(max_entries: u64) -> Self {
        Self {
            sequence: Cache::new(max_entries),
            blocklist: Cache::new(max_entries),
        }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn key(abs_path: &str, sha: &str) -> String {
    format!("{}:{}", abs_path, sha)
}
