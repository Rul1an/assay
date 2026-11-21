//! The named fields read by tool signature verification.
pub use crate::ignored_keys::{Error, MAX_BYTES};
use std::path::PathBuf;
use std::prelude::v1::*;

tracked_struct! {
    root struct TrustPolicyFile {
        /// An unsigned tool fails when this is true and passes when it is false.
        #[default]
        require_signed: bool,
        /// The only keys a signature is verified against.
        #[default]
        trusted_keys: Vec<TrustedKeyEntry>,
    }
}

tracked_struct! {
    struct TrustedKeyEntry {
        /// Must equal the key id of the key at `public_key_path` and of the signature.
        key_id: String,
        /// The key a signature is verified against; an entry without one verifies nothing.
        public_key_path: Option<PathBuf>,
        /// Shown when this key verifies a tool.
        name: Option<String>,
    }
}

/// Field descriptions for comparing the file grammar with the verifier's public model.
pub fn field_shapes() -> (crate::doc::Shape, crate::doc::Shape) {
    use crate::doc::ClosedValue;
    (TrustPolicyFile::__shape(), TrustedKeyEntry::__shape())
}

/// Decode the concrete trust-policy grammar in one pass. The YAML decoder supplies
/// field values and reports malformed known fields before we report unread paths.
///
/// The deserializable root is private so a caller cannot select another loader for it.
/// Construct the verifier's keys only after the complete file has passed validation.
///
/// ```compile_fail
/// use assay_common::trust_document::TrustPolicyFile;
/// let _ = assay_common::doc::load::<TrustPolicyFile>("require_signed: true");
/// ```
pub fn read<K>(
    text: &str,
    mut make_key: impl FnMut(String, Option<PathBuf>, Option<String>) -> K,
) -> Result<(bool, Vec<K>), Error> {
    let TrustPolicyFile {
        require_signed,
        trusted_keys,
    } = crate::ignored_keys::read(text)?;
    let keys = trusted_keys
        .into_iter()
        .map(
            |TrustedKeyEntry {
                 key_id,
                 public_key_path,
                 name,
             }| make_key(key_id, public_key_path, name),
        )
        .collect();
    Ok((require_signed, keys))
}
