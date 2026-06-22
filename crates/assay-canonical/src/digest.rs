//! Content-addressed IDs over canonical bytes.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{jcs, Error};

/// The content-addressed id of a value: `"sha256:" + hex(sha256(jcs_bytes))`.
///
/// The preimage is the value's RFC 8785 (JCS) canonical bytes, so the id is independent of how the
/// value was constructed (key order, whitespace) and is byte-for-byte the id `assay-evidence`
/// computes for the same value — both go through the same pinned `serde_jcs`.
///
/// This hashes the value *as given*; arrays are not reordered. For a semantic-set digest, normalize
/// the registered set-paths first with [`crate::set_paths::normalize_sets`], then call this.
///
/// ```
/// use serde_json::json;
/// let a = assay_canonical::content_id(&json!({"b": 2, "a": 1})).unwrap();
/// let b = assay_canonical::content_id(&json!({"a": 1, "b": 2})).unwrap();
/// assert_eq!(a, b); // key order does not change the id
/// assert!(a.starts_with("sha256:"));
/// ```
pub fn content_id<T: Serialize>(value: &T) -> Result<String, Error> {
    let bytes = jcs::to_vec(value)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn id_is_key_order_independent_but_array_order_sensitive() {
        assert_eq!(
            content_id(&json!({"x": 1, "y": 2})).unwrap(),
            content_id(&json!({"y": 2, "x": 1})).unwrap(),
        );
        assert_ne!(
            content_id(&json!({"x": ["a", "b"]})).unwrap(),
            content_id(&json!({"x": ["b", "a"]})).unwrap(),
        );
    }

    #[test]
    fn id_shape() {
        let id = content_id(&json!({})).unwrap();
        assert!(id.starts_with("sha256:"));
        assert_eq!(id.len(), "sha256:".len() + 64);
    }
}
