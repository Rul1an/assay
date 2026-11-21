//! Lockfile type ownership.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

assay_common::closed! {
    /// A pack lockfile.
    #[derive(Serialize)]
    root pub struct Lockfile {
        /// Schema version.
        version: u8,

        /// When the lockfile was generated.
        generated_at: DateTime<Utc>,

        /// Tool that generated the lockfile.
        generated_by: String,

        /// Locked packs.
        #[default]
        packs: Vec<LockedPack>,
    }
}

assay_common::closed! {
    /// A locked pack entry.
    #[derive(Serialize, Eq)]
    pub struct LockedPack {
        /// Pack name.
        name: String,

        /// Pack version.
        version: String,

        /// Content digest (sha256:...).
        digest: String,

        /// Source type.
        source: LockSource,

        /// Registry URL (if source is registry).
        #[omit_none]
        registry_url: Option<String>,

        /// Registry namespace: absent, explicitly global (null), or named.
        #[present_option]
        namespace: Option<Option<String>>,

        /// Fetch timestamp as recorded by the registry client.
        #[omit_none]
        fetched_at: Option<String>,

        /// HTTP entity tag, including its original quoting.
        #[omit_none]
        etag: Option<String>,

        /// BYOS URL (if source is byos).
        #[omit_none]
        byos_url: Option<String>,

        /// Signature information (if signed).
        #[omit_none]
        signature: Option<LockSignature>,
    }
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

impl assay_common::doc::ClosedValue for LockSource {
    fn __shape() -> assay_common::doc::Shape {
        assay_common::doc::Shape::Leaf
    }
}

assay_common::closed! {
    /// Signature information.
    #[derive(Serialize, Eq)]
    pub struct LockSignature {
        /// Signature algorithm.
        algorithm: String,

        /// Key ID used for signing.
        key_id: String,
    }
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
