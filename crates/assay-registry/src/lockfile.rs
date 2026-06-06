//! Lockfile support for reproducible builds.
//!
//! The lockfile (`assay.packs.lock`) records exact pack versions and digests
//! to ensure reproducible builds across machines and CI runs.
//!
//! # Lockfile Format (v2)
//!
//! ```yaml
//! version: 2
//! generated_at: "2026-01-29T10:00:00Z"
//! generated_by: "assay-cli/2.10.1"
//! packs:
//!   - name: eu-ai-act-pro
//!     version: "1.2.0"
//!     digest: sha256:abc123...
//!     source: registry
//!     registry_url: "https://registry.getassay.dev/v1"
//!     signature:
//!       algorithm: Ed25519
//!       key_id: sha256:def456...
//! ```

use std::path::Path;

use chrono::Utc;

use crate::error::RegistryResult;
use crate::resolver::PackResolver;

#[path = "lockfile_next/mod.rs"]
mod lockfile_next;
pub use lockfile_next::types::{
    LockMismatch, LockSignature, LockSource, LockedPack, Lockfile, VerifyLockResult,
};

/// Default lockfile name.
pub const LOCKFILE_NAME: &str = "assay.packs.lock";

/// Current lockfile schema version.
pub const LOCKFILE_VERSION: u8 = 2;

impl Lockfile {
    /// Create a new empty lockfile.
    pub fn new() -> Self {
        Self {
            version: LOCKFILE_VERSION,
            generated_at: Utc::now(),
            generated_by: format!("assay-cli/{}", env!("CARGO_PKG_VERSION")),
            packs: Vec::new(),
        }
    }

    /// Load a lockfile from a path.
    pub async fn load(path: impl AsRef<Path>) -> RegistryResult<Self> {
        lockfile_next::io::load_impl(path).await
    }

    /// Parse a lockfile from YAML content.
    pub fn parse(content: &str) -> RegistryResult<Self> {
        lockfile_next::parse::parse_lockfile_impl(content)
    }

    /// Save the lockfile to a path.
    pub async fn save(&self, path: impl AsRef<Path>) -> RegistryResult<()> {
        lockfile_next::io::save_impl(self, path).await
    }

    /// Convert to YAML string.
    pub fn to_yaml(&self) -> RegistryResult<String> {
        lockfile_next::format::to_yaml_impl(self)
    }

    /// Add or update a pack in the lockfile.
    pub fn add_pack(&mut self, pack: LockedPack) {
        lockfile_next::format::add_pack_impl(self, pack);
    }

    /// Remove a pack from the lockfile.
    pub fn remove_pack(&mut self, name: &str) -> bool {
        let len_before = self.packs.len();
        self.packs.retain(|p| p.name != name);
        self.packs.len() != len_before
    }

    /// Get a locked pack by name.
    pub fn get_pack(&self, name: &str) -> Option<&LockedPack> {
        self.packs.iter().find(|p| p.name == name)
    }

    /// Check if a pack is locked.
    pub fn contains(&self, name: &str) -> bool {
        self.packs.iter().any(|p| p.name == name)
    }

    /// Get all pack names.
    pub fn pack_names(&self) -> Vec<&str> {
        self.packs.iter().map(|p| p.name.as_str()).collect()
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a lockfile from pack references.
pub async fn generate_lockfile(
    references: &[String],
    resolver: &PackResolver,
) -> RegistryResult<Lockfile> {
    lockfile_next::generate_lockfile_impl(references, resolver).await
}

/// Verify packs against a lockfile.
pub async fn verify_lockfile(
    lockfile: &Lockfile,
    resolver: &PackResolver,
) -> RegistryResult<VerifyLockResult> {
    lockfile_next::digest::verify_lockfile_impl(lockfile, resolver).await
}

/// Check if lockfile is outdated (any pack has newer version available).
pub async fn check_lockfile(
    lockfile: &Lockfile,
    resolver: &PackResolver,
) -> RegistryResult<Vec<LockMismatch>> {
    lockfile_next::digest::check_lockfile_impl(lockfile, resolver).await
}

/// Update a lockfile with latest versions.
pub async fn update_lockfile(
    lockfile: &mut Lockfile,
    resolver: &PackResolver,
) -> RegistryResult<Vec<String>> {
    lockfile_next::digest::update_lockfile_impl(lockfile, resolver).await
}
