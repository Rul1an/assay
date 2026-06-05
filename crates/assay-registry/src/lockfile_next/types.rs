//! Lockfile type ownership.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A pack lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    /// Schema version.
    pub version: u8,

    /// When the lockfile was generated.
    pub generated_at: DateTime<Utc>,

    /// Tool that generated the lockfile.
    pub generated_by: String,

    /// Locked packs.
    #[serde(default)]
    pub packs: Vec<LockedPack>,
}

/// A locked pack entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockedPack {
    /// Pack name.
    pub name: String,

    /// Pack version.
    pub version: String,

    /// Content digest (sha256:...).
    pub digest: String,

    /// Source type.
    pub source: LockSource,

    /// Registry URL (if source is registry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry_url: Option<String>,

    /// BYOS URL (if source is byos).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byos_url: Option<String>,

    /// Signature information (if signed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<LockSignature>,
}

/// Source type for locked packs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LockSource {
    /// Bundled pack.
    Bundled,

    /// Registry pack.
    Registry,

    /// BYOS pack.
    Byos,

    /// Local file (not recommended for lockfiles).
    Local,
}

/// Signature information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockSignature {
    /// Signature algorithm.
    pub algorithm: String,

    /// Key ID used for signing.
    pub key_id: String,
}

/// Lockfile verification result.
#[derive(Debug, Clone)]
pub struct VerifyLockResult {
    /// Whether all packs match.
    pub all_match: bool,

    /// Packs that matched.
    pub matched: Vec<String>,

    /// Packs with digest mismatches.
    pub mismatched: Vec<LockMismatch>,

    /// Packs in lockfile but not resolved.
    pub missing: Vec<String>,

    /// Packs resolved but not in lockfile.
    pub extra: Vec<String>,
}

/// A lockfile mismatch.
#[derive(Debug, Clone)]
pub struct LockMismatch {
    /// Pack name.
    pub name: String,

    /// Pack version.
    pub version: String,

    /// Expected digest from lockfile.
    pub expected: String,

    /// Actual digest from resolution.
    pub actual: String,
}
