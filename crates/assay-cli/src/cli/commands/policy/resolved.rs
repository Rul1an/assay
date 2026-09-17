//! Shared policy resolution and validation loader.
//!
//! Provides `load_resolved` and bounded reading for `validate`, `resolve`,
//! `activate`, and `status`.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use assay_common::limits::{LimitKind, LimitReader};
use assay_core::mcp::policy::McpPolicy;
use sha2::{Digest, Sha256};

pub const MAX_INPUT_BYTES: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct Resolved {
    pub policy: McpPolicy,
    pub input_sha256: String,
    pub policy_digest: String,
}

pub fn read_bounded(path: &Path) -> anyhow::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut reader = LimitReader::new(file, MAX_INPUT_BYTES, LimitKind::SourceBytes);
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

pub fn input_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

pub fn load_resolved(bytes: &[u8]) -> anyhow::Result<Resolved> {
    let policy = McpPolicy::from_slice(bytes)?;
    policy
        .try_compile_all_schemas()
        .map_err(|error| anyhow::anyhow!("policy schemas failed to compile: {error}"))?;
    let policy_digest = policy
        .policy_digest()
        .ok_or_else(|| anyhow::anyhow!("failed to canonicalize policy digest"))?;
    Ok(Resolved {
        policy,
        input_sha256: input_sha256(bytes),
        policy_digest,
    })
}
