//! Key trust store for signature verification.
//!
//! The trust store manages trusted signing keys for pack verification.
//! Keys can come from:
//! - Pinned roots (compiled into binary)
//! - Configuration file
//! - Remote keys manifest (fetched from registry)

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ed25519_dalek::VerifyingKey;
use tokio::sync::RwLock;

use crate::error::{RegistryError, RegistryResult};
use crate::types::{KeysManifest, TrustedKey};
#[cfg(test)]
use crate::verify::compute_key_id;
#[cfg(test)]
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

/// Default cache TTL for keys manifest (24 hours).
const DEFAULT_KEYS_TTL_SECS: i64 = 24 * 60 * 60;
const PRODUCTION_TRUST_ROOTS_JSON: &str = include_str!("../assets/production-trust-roots.json");

#[path = "trust_next/mod.rs"]
mod trust_next;

use trust_next::access;
use trust_next::cache;
use trust_next::manifest;
#[cfg(test)]
use trust_next::pinned::parse_pinned_roots_json_impl;
use trust_next::pinned::{insert_pinned_key, load_production_roots_impl};

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
            inner: Arc::new(RwLock::new(cache::empty_inner())),
        }
    }

    /// Create a trust store with pinned root keys.
    ///
    /// Pinned roots are always trusted and cannot be revoked remotely.
    pub fn from_pinned_roots(roots: Vec<TrustedKey>) -> RegistryResult<Self> {
        let mut inner = cache::empty_inner();
        for root in &roots {
            insert_pinned_key(&mut inner, root)?;
        }

        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    /// Create a trust store with pinned root keys.
    ///
    /// Pinned roots are always trusted and cannot be revoked remotely.
    pub async fn with_pinned_roots(roots: Vec<TrustedKey>) -> RegistryResult<Self> {
        Self::from_pinned_roots(roots)
    }

    /// Create a trust store with the default production roots.
    pub fn from_production_roots() -> RegistryResult<Self> {
        load_production_roots_impl(PRODUCTION_TRUST_ROOTS_JSON)
    }

    /// Create a trust store with the default production roots.
    pub async fn with_production_roots() -> RegistryResult<Self> {
        Self::from_production_roots()
    }

    /// Add a pinned root key.
    pub async fn add_pinned_key(&self, key: &TrustedKey) -> RegistryResult<()> {
        let mut inner = self.inner.write().await;
        insert_pinned_key(&mut inner, key)
    }

    /// Add keys from a manifest (fetched from registry).
    pub async fn add_from_manifest(&self, manifest: &KeysManifest) -> RegistryResult<()> {
        let mut inner = self.inner.write().await;
        manifest::add_from_manifest(&mut inner, manifest)
    }

    /// Get a key by ID.
    pub async fn get_key_async(&self, key_id: &str) -> RegistryResult<VerifyingKey> {
        let inner = self.inner.read().await;
        access::get_key_inner(&inner, key_id)
    }

    /// Get a key by ID (blocking version for sync contexts).
    pub fn get_key(&self, key_id: &str) -> RegistryResult<VerifyingKey> {
        // Use try_read to avoid blocking
        match self.inner.try_read() {
            Ok(inner) => access::get_key_inner(&inner, key_id),
            Err(_) => Err(RegistryError::KeyNotTrusted {
                key_id: key_id.to_string(),
            }),
        }
    }

    /// Check if the keys manifest needs refresh.
    pub async fn needs_refresh(&self) -> bool {
        let inner = self.inner.read().await;
        cache::needs_refresh(&inner)
    }

    /// Check if a key is trusted.
    pub async fn is_trusted(&self, key_id: &str) -> bool {
        self.get_key_async(key_id).await.is_ok()
    }

    /// Get all trusted key IDs.
    pub async fn list_keys(&self) -> Vec<String> {
        let inner = self.inner.read().await;
        access::list_keys(&inner)
    }

    /// Get metadata for a key.
    pub async fn get_metadata(&self, key_id: &str) -> Option<KeyMetadata> {
        let inner = self.inner.read().await;
        access::get_metadata(&inner, key_id)
    }

    /// Clear all non-pinned keys (for testing or force refresh).
    pub async fn clear_cached_keys(&self) {
        let mut inner = self.inner.write().await;
        cache::clear_cached_keys(&mut inner);
    }
}

impl Default for TrustStore {
    fn default() -> Self {
        Self::new()
    }
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
    async fn test_with_production_roots_loads_embedded_roots() -> RegistryResult<()> {
        let store = TrustStore::with_production_roots().await?;
        let keys = store.list_keys().await;
        assert_eq!(keys.len(), 1);
        assert_eq!(
            keys[0],
            "sha256:3a64307d5655ba86fa3c95118ed8fe9665ef6bd37c752ca93f3bbe8f16e83a7f"
        );

        let meta = store
            .get_metadata(&keys[0])
            .await
            .ok_or_else(|| RegistryError::Config {
                message: "embedded production root metadata missing".to_string(),
            })?;
        assert!(meta.is_pinned);
        assert!(!meta.revoked);
        Ok(())
    }

    #[test]
    fn test_parse_pinned_roots_json_rejects_empty_rootset() {
        assert!(matches!(
            parse_pinned_roots_json_impl("[]"),
            Err(RegistryError::Config { .. })
        ));
    }

    #[test]
    fn test_parse_pinned_roots_json_rejects_duplicate_key_ids() {
        let duplicate = r#"[
            {
                "key_id": "sha256:dup",
                "algorithm": "Ed25519",
                "public_key": "MCowBQYDK2VwAyEAykCN7Cf9EQAB4UPonG5AtKfTVny0H4xaKpPI6wIGBwE=",
                "revoked": false
            },
            {
                "key_id": "sha256:dup",
                "algorithm": "Ed25519",
                "public_key": "MCowBQYDK2VwAyEAykCN7Cf9EQAB4UPonG5AtKfTVny0H4xaKpPI6wIGBwE=",
                "revoked": false
            }
        ]"#;

        assert!(matches!(
            parse_pinned_roots_json_impl(duplicate),
            Err(RegistryError::Config { .. })
        ));
    }

    #[test]
    fn test_load_production_roots_maps_key_mismatch_to_config() {
        let mismatched = r#"[
            {
                "key_id": "sha256:not-the-real-key-id",
                "algorithm": "Ed25519",
                "public_key": "MCowBQYDK2VwAyEAykCN7Cf9EQAB4UPonG5AtKfTVny0H4xaKpPI6wIGBwE=",
                "revoked": false
            }
        ]"#;

        let err = load_production_roots_impl(mismatched).unwrap_err();
        assert!(matches!(err, RegistryError::Config { .. }));
        assert!(err
            .to_string()
            .contains("invalid production trust root sha256:not-the-real-key-id"));
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

    // ==================== Trust Rotation Tests ====================

    #[tokio::test]
    async fn test_trust_rotation_new_key_via_manifest() {
        // Scenario: Root validates manifest A → manifest B adds new key → new key works
        let store = TrustStore::new();

        // Start with initial key (manifest A)
        let (_, key_a) = generate_trusted_key();
        let manifest_a = KeysManifest {
            version: 1,
            keys: vec![key_a.clone()],
            expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
        };
        store.add_from_manifest(&manifest_a).await.unwrap();
        assert!(store.is_trusted(&key_a.key_id).await);

        // Rotate: manifest B adds a new key
        let (_, key_b) = generate_trusted_key();
        let manifest_b = KeysManifest {
            version: 1,
            keys: vec![key_a.clone(), key_b.clone()], // Both keys
            expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
        };
        store.add_from_manifest(&manifest_b).await.unwrap();

        // Both keys should now be trusted
        assert!(store.is_trusted(&key_a.key_id).await);
        assert!(store.is_trusted(&key_b.key_id).await);
    }

    #[tokio::test]
    async fn test_trust_rotation_revoke_old_key() {
        // Scenario: Key A active → manifest revokes key A → key A no longer trusted
        let store = TrustStore::new();

        // Start with key A
        let (_, key_a) = generate_trusted_key();
        let manifest_v1 = KeysManifest {
            version: 1,
            keys: vec![key_a.clone()],
            expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
        };
        store.add_from_manifest(&manifest_v1).await.unwrap();
        assert!(store.is_trusted(&key_a.key_id).await);

        // Manifest v2: key A is now revoked
        let mut key_a_revoked = key_a.clone();
        key_a_revoked.revoked = true;

        let (_, key_b) = generate_trusted_key();
        let manifest_v2 = KeysManifest {
            version: 1,
            keys: vec![key_a_revoked, key_b.clone()],
            expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
        };
        store.add_from_manifest(&manifest_v2).await.unwrap();

        // Key A should no longer be trusted
        assert!(!store.is_trusted(&key_a.key_id).await);
        // Key B should be trusted
        assert!(store.is_trusted(&key_b.key_id).await);
    }

    #[tokio::test]
    async fn test_trust_rotation_pinned_root_survives_revocation() {
        // Scenario: Pinned root cannot be revoked by manifest
        let store = TrustStore::new();

        // Add pinned root
        let (_, pinned_root) = generate_trusted_key();
        store.add_pinned_key(&pinned_root).await.unwrap();
        assert!(store.is_trusted(&pinned_root.key_id).await);

        // Manifest tries to revoke the pinned root
        let mut revoked_root = pinned_root.clone();
        revoked_root.revoked = true;

        let manifest = KeysManifest {
            version: 1,
            keys: vec![revoked_root],
            expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
        };
        store.add_from_manifest(&manifest).await.unwrap();

        // Pinned root MUST still be trusted (cannot be revoked remotely)
        assert!(store.is_trusted(&pinned_root.key_id).await);
        let meta = store.get_metadata(&pinned_root.key_id).await.unwrap();
        assert!(meta.is_pinned);
        assert!(!meta.revoked);
    }

    #[tokio::test]
    async fn test_trust_rotation_expired_key_not_added() {
        // Scenario: Manifest contains already-expired key → should not be added
        let store = TrustStore::new();

        let (_, mut expired_key) = generate_trusted_key();
        expired_key.expires_at = Some(Utc::now() - chrono::Duration::hours(1)); // Expired 1 hour ago

        let manifest = KeysManifest {
            version: 1,
            keys: vec![expired_key.clone()],
            expires_at: Some(Utc::now() + chrono::Duration::hours(24)),
        };
        store.add_from_manifest(&manifest).await.unwrap();

        // Expired key should NOT be trusted
        assert!(!store.is_trusted(&expired_key.key_id).await);
    }

    #[tokio::test]
    async fn test_trust_rotation_key_expires_after_added() {
        // Scenario: Key added while valid, later becomes expired → should fail trust check
        let store = TrustStore::new();

        let (_, mut soon_to_expire) = generate_trusted_key();
        // Set to expire in the past (simulating time passing)
        soon_to_expire.expires_at = Some(Utc::now() - chrono::Duration::seconds(1));

        // First add without expiry check (simulating it was valid when added)
        // We need to manually add it to test the runtime check
        let manifest = KeysManifest {
            version: 1,
            keys: vec![soon_to_expire.clone()],
            expires_at: None,
        };
        // This won't add the key because it's already expired
        store.add_from_manifest(&manifest).await.unwrap();

        // The key should NOT be trusted because it was already expired when manifest was processed
        assert!(!store.is_trusted(&soon_to_expire.key_id).await);
    }
}
