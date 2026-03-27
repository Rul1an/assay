use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::{RegistryError, RegistryResult};
use crate::trust::{KeyMetadata, TrustStore, TrustStoreInner};
use crate::types::TrustedKey;
use crate::verify::compute_key_id;

use super::cache;
use super::decode::{decode_public_key_bytes, decode_verifying_key};

pub(in crate::trust) struct PreparedPinnedKey {
    pub key_id: String,
    pub verifying_key: ed25519_dalek::VerifyingKey,
    pub metadata: KeyMetadata,
}

pub(in crate::trust) fn parse_pinned_roots_json_impl(raw: &str) -> RegistryResult<Vec<TrustedKey>> {
    let roots: Vec<TrustedKey> = serde_json::from_str(raw).map_err(|e| RegistryError::Config {
        message: format!("invalid production trust roots: {}", e),
    })?;

    if roots.is_empty() {
        return Err(RegistryError::Config {
            message: "production trust roots are empty".to_string(),
        });
    }

    let mut seen = std::collections::HashSet::new();
    for root in &roots {
        if root.algorithm != "Ed25519" {
            return Err(RegistryError::Config {
                message: format!(
                    "production trust root {} uses unsupported algorithm {}",
                    root.key_id, root.algorithm
                ),
            });
        }

        if root.revoked {
            return Err(RegistryError::Config {
                message: format!("production trust root {} is revoked", root.key_id),
            });
        }

        if !seen.insert(root.key_id.clone()) {
            return Err(RegistryError::Config {
                message: format!("duplicate production trust root {}", root.key_id),
            });
        }
    }

    Ok(roots)
}

pub(in crate::trust) fn load_production_roots_impl(raw: &str) -> RegistryResult<TrustStore> {
    let roots = parse_pinned_roots_json_impl(raw)?;
    let mut inner = cache::empty_inner();

    for root in &roots {
        insert_pinned_key(&mut inner, root).map_err(|err| RegistryError::Config {
            message: format!("invalid production trust root {}: {}", root.key_id, err),
        })?;
    }

    Ok(TrustStore {
        inner: Arc::new(RwLock::new(inner)),
    })
}

pub(in crate::trust) fn prepare_pinned_key(key: &TrustedKey) -> RegistryResult<PreparedPinnedKey> {
    let verifying_key = decode_verifying_key(&key.public_key)?;
    let computed_id = compute_key_id(&decode_public_key_bytes(&key.public_key)?);

    if computed_id != key.key_id {
        return Err(RegistryError::SignatureInvalid {
            reason: format!(
                "key_id mismatch: claimed {}, computed {}",
                key.key_id, computed_id
            ),
        });
    }

    Ok(PreparedPinnedKey {
        key_id: key.key_id.clone(),
        verifying_key,
        metadata: KeyMetadata {
            description: key.description.clone(),
            added_at: key.added_at,
            expires_at: key.expires_at,
            revoked: false,
            is_pinned: true,
        },
    })
}

pub(in crate::trust) fn insert_prepared_pinned_key(
    inner: &mut TrustStoreInner,
    prepared: PreparedPinnedKey,
) {
    let PreparedPinnedKey {
        key_id,
        verifying_key,
        metadata,
    } = prepared;

    inner.keys.insert(key_id.clone(), verifying_key);
    inner.metadata.insert(key_id.clone(), metadata);
    if !inner.pinned_roots.contains(&key_id) {
        inner.pinned_roots.push(key_id);
    }
}

pub(in crate::trust) fn insert_pinned_key(
    inner: &mut TrustStoreInner,
    key: &TrustedKey,
) -> RegistryResult<()> {
    let prepared = prepare_pinned_key(key)?;
    insert_prepared_pinned_key(inner, prepared);
    Ok(())
}
