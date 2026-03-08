use super::{write_bundle_tar_gz, BundleEntry};
use crate::replay::manifest::ReplayManifest;
use anyhow::Result;
use sha2::{Digest, Sha256};

/// Compute SHA256 of the entire archive (for provenance.bundle_digest).
/// Caller must pass the same manifest + set of entries as write_bundle_tar_gz; entry order is irrelevant.
pub fn bundle_digest(manifest: &ReplayManifest, entries: &[BundleEntry]) -> Result<String> {
    let mut buf = Vec::new();
    write_bundle_tar_gz(&mut buf, manifest, entries)?;
    let hash = Sha256::digest(&buf);
    Ok(hex::encode(hash))
}
