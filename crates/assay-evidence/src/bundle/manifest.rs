//! Bundle manifest types (first file in archive).
//!
//! Contract: schema_version 1, manifest.json + events.ndjson layout.

use crate::bundle::x_assay::XAssayExtension;
use crate::types::ProducerMeta;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Bundle manifest (first file in archive).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    /// Schema version (always 1 for v1 contract)
    pub schema_version: u32,
    /// Bundle ID (equals run_root for v1)
    pub bundle_id: String,
    /// Producer metadata
    pub producer: ProducerMeta,
    /// Run identifier
    pub run_id: String,
    /// Total event count
    pub event_count: usize,
    /// Integrity chain root
    pub run_root: String,
    /// Algorithm specifications
    pub algorithms: AlgorithmMeta,
    /// File metadata (hash + size)
    pub files: BTreeMap<String, FileMeta>,
    /// ADR-025 E2: producer provenance + extensions (optional, additive)
    #[serde(rename = "x-assay", skip_serializing_if = "Option::is_none")]
    pub x_assay: Option<XAssayExtension>,
}

/// Algorithm metadata for verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AlgorithmMeta {
    /// Canonicalization scheme
    pub canon: String,
    /// Hash algorithm
    pub hash: String,
    /// Run root computation
    pub root: String,
}

impl Default for AlgorithmMeta {
    fn default() -> Self {
        Self {
            canon: "jcs-rfc8785".into(),
            hash: "sha256".into(),
            root: "sha256(concat(content_hash + \"\\n\"))".into(),
        }
    }
}

/// File metadata within bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMeta {
    /// Relative path within archive
    pub path: String,
    /// SHA-256 hash with prefix
    pub sha256: String,
    /// Size in bytes
    pub bytes: u64,
}
