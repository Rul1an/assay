//! MandateStore: SQLite-backed mandate consumption tracking.
//!
//! Provides atomic, idempotent mandate consumption with:
//! - Single-use / max_uses constraint enforcement
//! - Nonce replay prevention
//! - tool_call_id idempotency

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[path = "mandate_store_next/mod.rs"]
mod mandate_store_next;

/// Authorization receipt returned after successful consumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthzReceipt {
    pub mandate_id: String,
    pub use_id: String,
    pub use_count: u32,
    pub consumed_at: DateTime<Utc>,
    pub tool_call_id: String,
    /// True if this was a new consumption, false if idempotent retry.
    /// Used to avoid emitting duplicate lifecycle events on retries.
    pub was_new: bool,
}

/// Authorization errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuthzError {
    #[error("Mandate not found: {mandate_id}")]
    MandateNotFound { mandate_id: String },

    #[error("Mandate already used (single_use=true)")]
    AlreadyUsed,

    #[error("Max uses exceeded: {current} > {max}")]
    MaxUsesExceeded { max: u32, current: u32 },

    #[error("Nonce replay detected: {nonce}")]
    NonceReplay { nonce: String },

    #[error("Mandate metadata conflict for {mandate_id}: stored {field} differs")]
    MandateConflict { mandate_id: String, field: String },

    #[error("Invalid mandate constraints: single_use=true with max_uses={max_uses}")]
    InvalidConstraints { max_uses: u32 },

    #[error("Mandate revoked at {revoked_at}")]
    Revoked { revoked_at: DateTime<Utc> },

    #[error("Database error: {0}")]
    Database(String),
}

impl From<rusqlite::Error> for AuthzError {
    fn from(e: rusqlite::Error) -> Self {
        AuthzError::Database(e.to_string())
    }
}

/// Mandate metadata for upsert.
#[derive(Debug, Clone)]
pub struct MandateMetadata {
    pub mandate_id: String,
    pub mandate_kind: String,
    pub audience: String,
    pub issuer: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub single_use: bool,
    pub max_uses: Option<u32>,
    pub canonical_digest: String,
    pub key_id: String,
}

/// Parameters for consume_mandate.
#[derive(Debug, Clone)]
pub struct ConsumeParams<'a> {
    pub mandate_id: &'a str,
    pub tool_call_id: &'a str,
    pub nonce: Option<&'a str>,
    pub audience: &'a str,
    pub issuer: &'a str,
    pub tool_name: &'a str,
    pub operation_class: &'a str,
    pub source_run_id: Option<&'a str>,
}

/// SQLite-backed mandate store.
#[derive(Clone)]
pub struct MandateStore {
    conn: Arc<Mutex<Connection>>,
}

impl MandateStore {
    /// Open a file-backed store.
    pub fn open(path: &Path) -> Result<Self, AuthzError> {
        mandate_store_next::schema::open_impl(path)
    }

    /// Create an in-memory store (for testing).
    pub fn memory() -> Result<Self, AuthzError> {
        mandate_store_next::schema::memory_impl()
    }

    /// Create store from existing connection (for multi-connection tests).
    pub fn from_connection(conn: Connection) -> Result<Self, AuthzError> {
        mandate_store_next::schema::from_connection_impl(conn)
    }

    /// Upsert mandate metadata. Idempotent for same content, errors on conflict.
    pub fn upsert_mandate(&self, meta: &MandateMetadata) -> Result<(), AuthzError> {
        mandate_store_next::upsert::upsert_mandate_impl(self, meta)
    }

    /// Consume mandate atomically. Idempotent on tool_call_id.
    pub fn consume_mandate(&self, params: &ConsumeParams<'_>) -> Result<AuthzReceipt, AuthzError> {
        mandate_store_next::txn::consume_mandate_in_txn_impl(self, params)
    }

    fn consume_mandate_inner(
        &self,
        conn: &Connection,
        params: &ConsumeParams<'_>,
    ) -> Result<AuthzReceipt, AuthzError> {
        mandate_store_next::consume::consume_mandate_inner_impl(conn, params)
    }

    /// Get current use count for a mandate (for testing/debugging).
    pub fn get_use_count(&self, mandate_id: &str) -> Result<Option<u32>, AuthzError> {
        mandate_store_next::stats::get_use_count_impl(self, mandate_id)
    }

    /// Count use records for a mandate (for testing).
    pub fn count_uses(&self, mandate_id: &str) -> Result<u32, AuthzError> {
        mandate_store_next::stats::count_uses_impl(self, mandate_id)
    }

    /// Check if nonce exists (for testing).
    pub fn nonce_exists(
        &self,
        audience: &str,
        issuer: &str,
        nonce: &str,
    ) -> Result<bool, AuthzError> {
        mandate_store_next::stats::nonce_exists_impl(self, audience, issuer, nonce)
    }

    // =========================================================================
    // Revocation API (P0-A)
    // =========================================================================

    /// Insert or update a revocation record.
    ///
    /// Idempotent: re-inserting with same mandate_id updates the record.
    pub fn upsert_revocation(&self, r: &RevocationRecord) -> Result<(), AuthzError> {
        mandate_store_next::revocation::upsert_revocation_impl(self, r)
    }

    /// Get revoked_at timestamp for a mandate (if revoked).
    pub fn get_revoked_at(&self, mandate_id: &str) -> Result<Option<DateTime<Utc>>, AuthzError> {
        mandate_store_next::revocation::get_revoked_at_impl(self, mandate_id)
    }

    /// Check if a mandate is revoked (convenience method).
    pub fn is_revoked(&self, mandate_id: &str) -> Result<bool, AuthzError> {
        mandate_store_next::revocation::is_revoked_impl(self, mandate_id)
    }
}

/// Revocation record for upsert.
#[derive(Debug, Clone)]
pub struct RevocationRecord {
    pub mandate_id: String,
    pub revoked_at: DateTime<Utc>,
    pub reason: Option<String>,
    pub revoked_by: Option<String>,
    pub source: Option<String>,
    pub event_id: Option<String>,
}

/// Compute deterministic use_id per SPEC-Mandate-v1.0.4 §7.4.
///
/// ```text
/// use_id = "sha256:" + hex(SHA256(mandate_id + ":" + tool_call_id + ":" + use_count))
/// ```
pub fn compute_use_id(mandate_id: &str, tool_call_id: &str, use_count: u32) -> String {
    mandate_store_next::stats::compute_use_id_impl(mandate_id, tool_call_id, use_count)
}

#[cfg(test)]
#[path = "mandate_store_next/tests.rs"]
mod tests;
