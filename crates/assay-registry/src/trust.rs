//! Key trust store for signature verification.
//!
//! The trust store manages trusted signing keys for pack verification.
//! Keys can come from:
//! - Pinned roots (compiled into binary)
//! - Configuration file
//! - Remote keys manifest (fetched from registry)

use std::collections::HashMap;
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::{DateTime, Utc};
use ed25519_dalek::VerifyingKey;
use tokio::sync::RwLock;

use crate::error::{RegistryError, RegistryResult};
use crate::types::{KeysManifest, TrustedKey};
use crate::verify::compute_key_id;

/// Default cache TTL for keys manifest (24 hours).
const DEFAULT_KEYS_TTL_SECS: i64 = 24 * 60 * 60;

/// Trust store for signing keys.
#[derive(Debug, Clone)]
pub struct TrustStore {
    inner: Arc<RwLock<TrustStoreInner>>,
}

#[derive(Debug)]
struct TrustStoreInner {
    /// Key ID -> VerifyingKey
    keys: HashMap<String, VerifyingKey>,

    /// Key metadata
    metadata: HashMap<String, KeyMetadata>,

    /// Pinned root key IDs (always trusted)
    pinned_roots: Vec<String>,

    /// When the keys manifest was last fetched
    manifest_fetched_at: Option<DateTime<Utc>>,

    /// When the cached manifest expires
    manifest_expires_at: Option<DateTime<Utc>>,
}

/// Metadata for a trusted key.
#[derive(Debug, Clone)]
pub struct KeyMetadata {
    /// Human-readable description.
    pub description: Option<String>,

    /// When the key was added.
    pub added_at: Option<DateTime<Utc>>,

    /// When the key expires.
    pub expires_at: Option<DateTime<Utc>>,

    /// Whether the key is revoked.
    pub revoked: bool,

    /// Whether this is a pinned root key.
    pub is_pinned: bool,
}

impl TrustStore {
    /// Create an empty trust store.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(TrustStoreInner {
                keys: HashMap::new(),
                metadata: HashMap::new(),
                pinned_roots: Vec::new(),
                manifest_fetched_at: None,
                manifest_expires_at: None,
            })),
        }
    }

    /// Create a trust store with pinned root keys.
    ///
    /// Pinned roots are always trusted and cannot be revoked remotely.
    pub async fn with_pinned_roots(roots: Vec<TrustedKey>) -> RegistryResult<Self> {
        let store = Self::new();
        for root in roots {
            store.add_pinned_key(&root).await?;
        }
        Ok(store)
    }

    /// Create a trust store with the default production roots.
    pub async fn with_production_roots() -> RegistryResult<Self> {
        // In production, these would be real keys compiled into the binary.
        // For now, return an empty store that will fetch keys from the registry.
        Ok(Self::new())
    }

    /// Add a pinned root key.
    pub async fn add_pinned_key(&self, key: &TrustedKey) -> RegistryResult<()> {
        let verifying_key = decode_verifying_key(&key.public_key)?;
        let computed_id = compute_key_id(&decode_public_key_bytes(&key.public_key)?);

        // Verify key_id matches
        if computed_id != key.key_id {
            return Err(RegistryError::SignatureInvalid {
                reason: format!(
                    "key_id mismatch: claimed {}, computed {}",
                    key.key_id, computed_id
                ),
            });
        }

        let mut inner = self.inner.write().await;
        inner.keys.insert(key.key_id.clone(), verifying_key);
        inner.metadata.insert(
            key.key_id.clone(),
            KeyMetadata {
                description: key.description.clone(),
                added_at: key.added_at,
                expires_at: key.expires_at,
                revoked: false, // Pinned keys cannot be revoked
                is_pinned: true,
            },
        );
        inner.pinned_roots.push(key.key_id.clone());

        Ok(())
    }

    /// Add keys from a manifest (fetched from registry).
    pub async fn add_from_manifest(&self, manifest: &KeysManifest) -> RegistryResult<()> {
        let now = Utc::now();

        let mut inner = self.inner.write().await;

        for key in &manifest.keys {
            // Skip revoked keys
            if key.revoked {
                // If the key exists and is not pinned, remove it
                if !inner.pinned_roots.contains(&key.key_id) {
                    inner.keys.remove(&key.key_id);
                    if let Some(meta) = inner.metadata.get_mut(&key.key_id) {
                        meta.revoked = true;
                    }
                }
                continue;
            }

            // Skip expired keys
            if let Some(expires_at) = key.expires_at {
                if expires_at < now {
                    continue;
                }
            }

            // Don't overwrite pinned roots
            if inner.pinned_roots.contains(&key.key_id) {
                continue;
            }

            // Decode and add key
            match decode_verifying_key(&key.public_key) {
                Ok(verifying_key) => {
                    // Verify key_id
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

        // Update cache metadata
        inner.manifest_fetched_at = Some(now);
        inner.manifest_expires_at = manifest
            .expires_at
            .or(Some(now + chrono::Duration::seconds(DEFAULT_KEYS_TTL_SECS)));

        Ok(())
    }

    /// Get a key by ID.
    pub async fn get_key_async(&self, key_id: &str) -> RegistryResult<VerifyingKey> {
        let inner = self.inner.read().await;
        self.get_key_inner(&inner, key_id)
    }

    /// Get a key by ID (blocking version for sync contexts).
    pub fn get_key(&self, key_id: &str) -> RegistryResult<VerifyingKey> {
        // Use try_read to avoid blocking
        match self.inner.try_read() {
            Ok(inner) => self.get_key_inner(&inner, key_id),
            Err(_) => Err(RegistryError::KeyNotTrusted {
                key_id: key_id.to_string(),
            }),
        }
    }

    fn get_key_inner(&self, inner: &TrustStoreInner, key_id: &str) -> RegistryResult<VerifyingKey> {
        // Check if key exists
        let key = inner
            .keys
            .get(key_id)
            .ok_or_else(|| RegistryError::KeyNotTrusted {
                key_id: key_id.to_string(),
            })?;

        // Check if revoked
        if let Some(meta) = inner.metadata.get(key_id) {
            if meta.revoked {
                return Err(RegistryError::KeyNotTrusted {
                    key_id: key_id.to_string(),
                });
            }

            // Check if expired
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

    /// Check if the keys manifest needs refresh.
    pub async fn needs_refresh(&self) -> bool {
        let inner = self.inner.read().await;

        match inner.manifest_expires_at {
            Some(expires_at) => Utc::now() >= expires_at,
            None => inner.manifest_fetched_at.is_none(),
        }
    }

    /// Check if a key is trusted.
    pub async fn is_trusted(&self, key_id: &str) -> bool {
        self.get_key_async(key_id).await.is_ok()
    }

    /// Get all trusted key IDs.
    pub async fn list_keys(&self) -> Vec<String> {
        let inner = self.inner.read().await;
        inner.keys.keys().cloned().collect()
    }

    /// Get metadata for a key.
    pub async fn get_metadata(&self, key_id: &str) -> Option<KeyMetadata> {
        let inner = self.inner.read().await;
        inner.metadata.get(key_id).cloned()
    }

    /// Clear all non-pinned keys (for testing or force refresh).
    pub async fn clear_cached_keys(&self) {
        let mut inner = self.inner.write().await;

        // Capture pinned roots to avoid borrow conflict
        let pinned_roots: std::collections::HashSet<_> =
            inner.pinned_roots.iter().cloned().collect();

        // Keep only pinned roots
        inner.keys.retain(|k, _| pinned_roots.contains(k));
        inner.metadata.retain(|k, _| pinned_roots.contains(k));
        inner.manifest_fetched_at = None;
        inner.manifest_expires_at = None;
    }
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode a Base64-encoded SPKI public key to VerifyingKey.
fn decode_verifying_key(b64: &str) -> RegistryResult<VerifyingKey> {
    use pkcs8::DecodePublicKey;

    let bytes = BASE64.decode(b64).map_err(|e| RegistryError::Config {
        message: format!("invalid base64 public key: {}", e),
    })?;

    VerifyingKey::from_public_key_der(&bytes).map_err(|e| RegistryError::Config {
        message: format!("invalid SPKI public key: {}", e),
    })
}

/// Decode Base64 public key bytes.
fn decode_public_key_bytes(b64: &str) -> RegistryResult<Vec<u8>> {
    BASE64.decode(b64).map_err(|e| RegistryError::Config {
        message: format!("invalid base64 public key: {}", e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use pkcs8::EncodePublicKey;

    fn generate_trusted_key() -> (SigningKey, TrustedKey) {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        let spki_der = verifying_key.to_public_key_der().unwrap();
        let public_key_b64 = BASE64.encode(spki_der.as_bytes());
        let key_id = compute_key_id(spki_der.as_bytes());

        let trusted = TrustedKey {
            key_id,
            algorithm: "Ed25519".to_string(),
            public_key: public_key_b64,
            description: Some("Test key".to_string()),
            added_at: Some(Utc::now()),
            expires_at: None,
            revoked: false,
        };

        (signing_key, trusted)
    }

    #[tokio::test]
    async fn test_empty_trust_store() {
        let store = TrustStore::new();
        let result = store.get_key_async("sha256:unknown").await;
        assert!(matches!(result, Err(RegistryError::KeyNotTrusted { .. })));
    }

    #[tokio::test]
    async fn test_add_pinned_key() {
        let store = TrustStore::new();
        let (_signing_key, trusted) = generate_trusted_key();

        store.add_pinned_key(&trusted).await.unwrap();

        let key = store.get_key_async(&trusted.key_id).await.unwrap();
        assert_eq!(key.as_bytes().len(), 32);

        let meta = store.get_metadata(&trusted.key_id).await.unwrap();
        assert!(meta.is_pinned);
        assert!(!meta.revoked);
    }

    #[tokio::test]
    async fn test_add_from_manifest() {
        let store = TrustStore::new();
        let (_, trusted1) = generate_trusted_key();
        let (_, trusted2) = generate_trusted_key();

        let manifest = KeysManifest {
            version: 1,
            keys: vec![trusted1.clone(), trusted2.clone()],
            expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
        };

        store.add_from_manifest(&manifest).await.unwrap();

        assert!(store.is_trusted(&trusted1.key_id).await);
        assert!(store.is_trusted(&trusted2.key_id).await);
    }

    #[tokio::test]
    async fn test_revoked_key_in_manifest() {
        let store = TrustStore::new();
        let (_, mut trusted) = generate_trusted_key();
        trusted.revoked = true;

        let manifest = KeysManifest {
            version: 1,
            keys: vec![trusted.clone()],
            expires_at: None,
        };

        store.add_from_manifest(&manifest).await.unwrap();

        // Revoked key should not be added
        assert!(!store.is_trusted(&trusted.key_id).await);
    }

    #[tokio::test]
    async fn test_expired_key_in_manifest() {
        let store = TrustStore::new();
        let (_, mut trusted) = generate_trusted_key();
        trusted.expires_at = Some(Utc::now() - chrono::Duration::hours(1));

        let manifest = KeysManifest {
            version: 1,
            keys: vec![trusted.clone()],
            expires_at: None,
        };

        store.add_from_manifest(&manifest).await.unwrap();

        // Expired key should not be added
        assert!(!store.is_trusted(&trusted.key_id).await);
    }

    #[tokio::test]
    async fn test_pinned_key_not_overwritten() {
        let store = TrustStore::new();
        let (_, trusted) = generate_trusted_key();

        // Add as pinned
        store.add_pinned_key(&trusted).await.unwrap();

        // Try to add revoked version via manifest
        let mut revoked = trusted.clone();
        revoked.revoked = true;

        let manifest = KeysManifest {
            version: 1,
            keys: vec![revoked],
            expires_at: None,
        };

        store.add_from_manifest(&manifest).await.unwrap();

        // Should still be trusted (pinned cannot be revoked)
        assert!(store.is_trusted(&trusted.key_id).await);
        let meta = store.get_metadata(&trusted.key_id).await.unwrap();
        assert!(meta.is_pinned);
        assert!(!meta.revoked);
    }

    #[tokio::test]
    async fn test_needs_refresh() {
        let store = TrustStore::new();

        // Empty store needs refresh
        assert!(store.needs_refresh().await);

        // Add manifest
        let manifest = KeysManifest {
            version: 1,
            keys: vec![],
            expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
        };
        store.add_from_manifest(&manifest).await.unwrap();

        // Should not need refresh
        assert!(!store.needs_refresh().await);
    }

    #[tokio::test]
    async fn test_clear_cached_keys() {
        let store = TrustStore::new();
        let (_, pinned) = generate_trusted_key();
        let (_, cached) = generate_trusted_key();

        // Add pinned key
        store.add_pinned_key(&pinned).await.unwrap();

        // Add cached key via manifest
        let manifest = KeysManifest {
            version: 1,
            keys: vec![cached.clone()],
            expires_at: None,
        };
        store.add_from_manifest(&manifest).await.unwrap();

        assert!(store.is_trusted(&pinned.key_id).await);
        assert!(store.is_trusted(&cached.key_id).await);

        // Clear cached
        store.clear_cached_keys().await;

        // Pinned should remain, cached should be gone
        assert!(store.is_trusted(&pinned.key_id).await);
        assert!(!store.is_trusted(&cached.key_id).await);
    }

    #[tokio::test]
    async fn test_list_keys() {
        let store = TrustStore::new();
        let (_, key1) = generate_trusted_key();
        let (_, key2) = generate_trusted_key();

        store.add_pinned_key(&key1).await.unwrap();
        store.add_pinned_key(&key2).await.unwrap();

        let keys = store.list_keys().await;
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&key1.key_id));
        assert!(keys.contains(&key2.key_id));
    }

    #[tokio::test]
    async fn test_key_id_mismatch_rejected() {
        let store = TrustStore::new();
        let (_, mut trusted) = generate_trusted_key();

        // Corrupt the key_id
        trusted.key_id =
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string();

        let result = store.add_pinned_key(&trusted).await;
        assert!(matches!(
            result,
            Err(RegistryError::SignatureInvalid { .. })
        ));
    }
}
