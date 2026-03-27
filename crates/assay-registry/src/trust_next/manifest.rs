use chrono::Utc;

use crate::error::RegistryResult;
use crate::trust::{KeyMetadata, TrustStoreInner, DEFAULT_KEYS_TTL_SECS};
use crate::types::KeysManifest;
use crate::verify::compute_key_id;

use super::decode::{decode_public_key_bytes, decode_verifying_key};

pub(in crate::trust) fn add_from_manifest(
    inner: &mut TrustStoreInner,
    manifest: &KeysManifest,
) -> RegistryResult<()> {
    let now = Utc::now();

    for key in &manifest.keys {
        if key.revoked {
            if !inner.pinned_roots.contains(&key.key_id) {
                inner.keys.remove(&key.key_id);
                if let Some(meta) = inner.metadata.get_mut(&key.key_id) {
                    meta.revoked = true;
                }
            }
            continue;
        }

        if let Some(expires_at) = key.expires_at {
            if expires_at < now {
                continue;
            }
        }

        if inner.pinned_roots.contains(&key.key_id) {
            continue;
        }

        match decode_verifying_key(&key.public_key) {
            Ok(verifying_key) => {
                let computed_id = match decode_public_key_bytes(&key.public_key) {
                    Ok(bytes) => compute_key_id(&bytes),
                    Err(_) => continue,
                };

                if computed_id != key.key_id {
                    tracing::warn!(
                        claimed = %key.key_id,
                        computed = %computed_id,
                        "key_id mismatch, skipping"
                    );
                    continue;
                }

                inner.keys.insert(key.key_id.clone(), verifying_key);
                inner.metadata.insert(
                    key.key_id.clone(),
                    KeyMetadata {
                        description: key.description.clone(),
                        added_at: key.added_at,
                        expires_at: key.expires_at,
                        revoked: false,
                        is_pinned: false,
                    },
                );
            }
            Err(e) => {
                tracing::warn!(key_id = %key.key_id, error = %e, "failed to decode key");
            }
        }
    }

    inner.manifest_fetched_at = Some(now);
    inner.manifest_expires_at = manifest
        .expires_at
        .or(Some(now + chrono::Duration::seconds(DEFAULT_KEYS_TTL_SECS)));

    Ok(())
}
