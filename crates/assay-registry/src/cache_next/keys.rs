//! Key/path derivation boundary scaffold for cache split.
//!
//! Planned ownership (Step2+):
//! - cache key and path derivation helpers

use std::path::{Path, PathBuf};

pub(crate) fn pack_dir_impl(cache_dir: &Path, name: &str, version: &str) -> PathBuf {
    cache_dir.join(name).join(version)
}
