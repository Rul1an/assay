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

/// Default cache TTL for keys manifest (24 hours).
const DEFAULT_KEYS_TTL_SECS: i64 = 24 * 60 * 60;
const PRODUCTION_TRUST_ROOTS_JSON: &str = include_str!("../assets/production-trust-roots.json");

#[path = "trust_next/mod.rs"]
mod trust_next;

use trust_next::access;
use trust_next::cache;
use trust_next::manifest;
use trust_next::pinned::{
    insert_pinned_key, insert_prepared_pinned_key, load_production_roots_impl, prepare_pinned_key,
};

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
        let prepared = prepare_pinned_key(key)?;
        let mut inner = self.inner.write().await;
        insert_prepared_pinned_key(&mut inner, prepared);
        Ok(())
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
