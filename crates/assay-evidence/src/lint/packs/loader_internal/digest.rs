//! Digest and canonicalization boundary for pack loading.
//!
//! Intended ownership (Commit B):
//! - deterministic digest computation helpers

use crate::lint::packs::loader::PackError;
use crate::lint::packs::schema::PackDefinition;
use sha2::{Digest, Sha256};

/// Compute pack digest: sha256(JCS(JSON(pack)))
pub(crate) fn compute_pack_digest_impl(definition: &PackDefinition) -> Result<String, PackError> {
    // Serialize to canonical JSON using RFC 8785 JCS
    let canonical = serde_jcs::to_string(definition).map_err(|e| PackError::YamlParseError {
        message: format!("Failed to canonicalize pack to JCS JSON: {}", e),
    })?;

    // Compute SHA-256
    let hash = Sha256::digest(canonical.as_bytes());
    Ok(format!("sha256:{}", hex::encode(hash)))
}
