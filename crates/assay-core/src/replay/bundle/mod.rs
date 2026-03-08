//! Replay bundle container writer (E9).
//!
//! Writes a hermetic .tar.gz with canonical layout: manifest.json, then
//! files under files/, outputs/, cassettes/ in deterministic order.
//! No user-facing CLI here (E9c); this is the core library for bundle creation.

mod io;
mod manifest;
pub mod paths;
mod verify;

use crate::replay::manifest::ReplayManifest;

/// Single file to add to the bundle: relative path (POSIX) and contents.
#[derive(Debug, Clone)]
pub struct BundleEntry {
    /// Relative path with POSIX forward slashes (e.g. "files/trace.jsonl").
    pub path: String,
    /// File contents.
    pub data: Vec<u8>,
}

/// Result of reading a bundle: manifest and all file entries (path -> contents).
/// Paths are POSIX, relative to bundle root; manifest.json is not in entries.
#[derive(Debug)]
pub struct ReadBundle {
    pub manifest: ReplayManifest,
    pub entries: Vec<(String, Vec<u8>)>,
}

pub use io::{read_bundle_tar_gz, write_bundle_tar_gz};
pub use manifest::build_file_manifest;
pub use verify::bundle_digest;

#[cfg(test)]
mod tests;
