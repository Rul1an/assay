use chrono::Utc;
use ed25519_dalek::VerifyingKey;

use crate::error::{RegistryError, RegistryResult};
use crate::trust::{KeyMetadata, TrustStoreInner};

pub(in crate::trust) fn get_key_inner(
    inner: &TrustStoreInner,
    key_id: &str,
) -> RegistryResult<VerifyingKey> {
    let key = inner
        .keys
        .get(key_id)
        .ok_or_else(|| RegistryError::KeyNotTrusted {
            key_id: key_id.to_string(),
        })?;

    if let Some(meta) = inner.metadata.get(key_id) {
        if meta.revoked {
            return Err(RegistryError::KeyNotTrusted {
                key_id: key_id.to_string(),
            });
        }

        if let Some(expires_at) = meta.expires_at {
            if expires_at < Utc::now() {
                return Err(RegistryError::KeyNotTrusted {
                    key_id: key_id.to_string(),
                });
            }
        }
    }

    Ok(*key)
}

pub(in crate::trust) fn list_keys(inner: &TrustStoreInner) -> Vec<String> {
    inner.keys.keys().cloned().collect()
}

pub(in crate::trust) fn get_metadata(inner: &TrustStoreInner, key_id: &str) -> Option<KeyMetadata> {
    inner.metadata.get(key_id).cloned()
}
