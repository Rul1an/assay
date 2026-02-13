//! Digest computation for canonical pack content.

use sha2::{Digest, Sha256};

/// Compute SHA-256 hash of bytes and format as `sha256:{hex}`.
pub fn sha256_prefixed(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    format!("sha256:{:x}", hash)
}
