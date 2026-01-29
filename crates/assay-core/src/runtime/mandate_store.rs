//! MandateStore: SQLite-backed mandate consumption tracking.
//!
//! Provides atomic, idempotent mandate consumption with:
//! - Single-use / max_uses constraint enforcement
//! - Nonce replay prevention
//! - tool_call_id idempotency

use super::schema::MANDATE_SCHEMA;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;

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
        let conn = Connection::open(path)?;
        Self::init_connection(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create an in-memory store (for testing).
    pub fn memory() -> Result<Self, AuthzError> {
        let conn = Connection::open_in_memory()?;
        Self::init_connection(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Create store from existing connection (for multi-connection tests).
    pub fn from_connection(conn: Connection) -> Result<Self, AuthzError> {
        Self::init_connection(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn init_connection(conn: &Connection) -> Result<(), AuthzError> {
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        // WAL mode for file-backed DBs (no-op for in-memory)
        let _ = conn.execute("PRAGMA journal_mode = WAL", []);
        conn.execute_batch(MANDATE_SCHEMA)?;
        Ok(())
    }

    /// Upsert mandate metadata. Idempotent for same content, errors on conflict.
    pub fn upsert_mandate(&self, meta: &MandateMetadata) -> Result<(), AuthzError> {
        // Validate constraints: single_use implies max_uses == 1
        if meta.single_use {
            if let Some(max) = meta.max_uses {
                if max != 1 {
                    return Err(AuthzError::InvalidConstraints { max_uses: max });
                }
            }
        }

        let conn = self.conn.lock().unwrap();

        // Insert with ON CONFLICT DO NOTHING
        conn.execute(
            r#"
            INSERT INTO mandates (
                mandate_id, mandate_kind, audience, issuer, expires_at,
                single_use, max_uses, use_count, canonical_digest, key_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?9)
            ON CONFLICT(mandate_id) DO NOTHING
            "#,
            params![
                meta.mandate_id,
                meta.mandate_kind,
                meta.audience,
                meta.issuer,
                meta.expires_at.map(|t| t.to_rfc3339()),
                meta.single_use as i32,
                meta.max_uses.map(|m| m as i64),
                meta.canonical_digest,
                meta.key_id,
            ],
        )?;

        // Verify consistency if already existed
        let stored: Option<(String, String, String, String, String)> = conn
            .query_row(
                r#"
                SELECT mandate_kind, audience, issuer, canonical_digest, key_id
                FROM mandates WHERE mandate_id = ?
                "#,
                [&meta.mandate_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;

        if let Some((kind, aud, iss, digest, key)) = stored {
            if kind != meta.mandate_kind {
                return Err(AuthzError::MandateConflict {
                    mandate_id: meta.mandate_id.clone(),
                    field: "mandate_kind".to_string(),
                });
            }
            if aud != meta.audience {
                return Err(AuthzError::MandateConflict {
                    mandate_id: meta.mandate_id.clone(),
                    field: "audience".to_string(),
                });
            }
            if iss != meta.issuer {
                return Err(AuthzError::MandateConflict {
                    mandate_id: meta.mandate_id.clone(),
                    field: "issuer".to_string(),
                });
            }
            if digest != meta.canonical_digest {
                return Err(AuthzError::MandateConflict {
                    mandate_id: meta.mandate_id.clone(),
                    field: "canonical_digest".to_string(),
                });
            }
            if key != meta.key_id {
                return Err(AuthzError::MandateConflict {
                    mandate_id: meta.mandate_id.clone(),
                    field: "key_id".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Consume mandate atomically. Idempotent on tool_call_id.
    pub fn consume_mandate(&self, params: &ConsumeParams<'_>) -> Result<AuthzReceipt, AuthzError> {
        let conn = self.conn.lock().unwrap();

        // BEGIN IMMEDIATE acquires write lock immediately
        conn.execute("BEGIN IMMEDIATE", [])?;

        let result = self.consume_mandate_inner(&conn, params);

        match &result {
            Ok(_) => {
                conn.execute("COMMIT", [])?;
            }
            Err(_) => {
                let _ = conn.execute("ROLLBACK", []);
            }
        }

        result
    }

    fn consume_mandate_inner(
        &self,
        conn: &Connection,
        params: &ConsumeParams<'_>,
    ) -> Result<AuthzReceipt, AuthzError> {
        let ConsumeParams {
            mandate_id,
            tool_call_id,
            nonce,
            audience,
            issuer,
            tool_name,
            operation_class,
            source_run_id,
        } = params;
        // Step 1: Idempotency check
        let existing: Option<(String, i64, String)> = conn
            .query_row(
                "SELECT use_id, use_count, consumed_at FROM mandate_uses WHERE tool_call_id = ?",
                [tool_call_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        if let Some((use_id, use_count, consumed_at)) = existing {
            // Return existing receipt (idempotent retry)
            return Ok(AuthzReceipt {
                mandate_id: mandate_id.to_string(),
                use_id,
                use_count: use_count as u32,
                consumed_at: DateTime::parse_from_rfc3339(&consumed_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                tool_call_id: tool_call_id.to_string(),
                was_new: false, // Idempotent retry
            });
        }

        // Step 2: Nonce replay check (atomic INSERT, not SELECT+INSERT)
        if let Some(n) = nonce {
            let insert_result = conn.execute(
                "INSERT INTO nonces (audience, issuer, nonce, mandate_id) VALUES (?1, ?2, ?3, ?4)",
                params![audience, issuer, n, mandate_id],
            );

            if let Err(e) = insert_result {
                if e.to_string().contains("UNIQUE constraint failed") {
                    return Err(AuthzError::NonceReplay {
                        nonce: n.to_string(),
                    });
                }
                return Err(e.into());
            }
        }

        // Step 3: Get mandate metadata + current use count
        let row: Option<(i64, i32, Option<i64>)> = conn
            .query_row(
                "SELECT use_count, single_use, max_uses FROM mandates WHERE mandate_id = ?",
                [mandate_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let (current_count, single_use, max_uses) = match row {
            Some(r) => r,
            None => {
                return Err(AuthzError::MandateNotFound {
                    mandate_id: mandate_id.to_string(),
                });
            }
        };

        let new_count = current_count + 1;

        // Step 4: Check constraints
        if single_use != 0 && current_count > 0 {
            return Err(AuthzError::AlreadyUsed);
        }

        if let Some(max) = max_uses {
            if new_count > max {
                return Err(AuthzError::MaxUsesExceeded {
                    max: max as u32,
                    current: new_count as u32,
                });
            }
        }

        // Step 5: Increment count + insert use record
        conn.execute(
            "UPDATE mandates SET use_count = ?1 WHERE mandate_id = ?2",
            params![new_count, mandate_id],
        )?;

        // use_id is content-addressed (deterministic) per SPEC-Mandate-v1.0.4 §7.4
        // use_id = sha256(mandate_id + ":" + tool_call_id + ":" + use_count)
        let use_id = compute_use_id(mandate_id, tool_call_id, new_count as u32);
        let consumed_at = Utc::now();

        conn.execute(
            r#"
            INSERT INTO mandate_uses (
                use_id, mandate_id, tool_call_id, use_count, consumed_at,
                tool_name, operation_class, nonce, source_run_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                use_id,
                mandate_id,
                tool_call_id,
                new_count,
                consumed_at.to_rfc3339(),
                tool_name,
                operation_class,
                nonce,
                source_run_id,
            ],
        )?;

        Ok(AuthzReceipt {
            mandate_id: mandate_id.to_string(),
            use_id,
            use_count: new_count as u32,
            consumed_at,
            tool_call_id: tool_call_id.to_string(),
            was_new: true, // First consumption
        })
    }

    /// Get current use count for a mandate (for testing/debugging).
    pub fn get_use_count(&self, mandate_id: &str) -> Result<Option<u32>, AuthzError> {
        let conn = self.conn.lock().unwrap();
        let count: Option<i64> = conn
            .query_row(
                "SELECT use_count FROM mandates WHERE mandate_id = ?",
                [mandate_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(count.map(|c| c as u32))
    }

    /// Count use records for a mandate (for testing).
    pub fn count_uses(&self, mandate_id: &str) -> Result<u32, AuthzError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mandate_uses WHERE mandate_id = ?",
            [mandate_id],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    /// Check if nonce exists (for testing).
    pub fn nonce_exists(
        &self,
        audience: &str,
        issuer: &str,
        nonce: &str,
    ) -> Result<bool, AuthzError> {
        let conn = self.conn.lock().unwrap();
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM nonces WHERE audience = ? AND issuer = ? AND nonce = ?",
            params![audience, issuer, nonce],
            |row| row.get(0),
        )?;
        Ok(exists > 0)
    }

    // =========================================================================
    // Revocation API (P0-A)
    // =========================================================================

    /// Insert or update a revocation record.
    ///
    /// Idempotent: re-inserting with same mandate_id updates the record.
    pub fn upsert_revocation(&self, r: &RevocationRecord) -> Result<(), AuthzError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO mandate_revocations (mandate_id, revoked_at, reason, revoked_by, source, event_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(mandate_id) DO UPDATE SET
                revoked_at = excluded.revoked_at,
                reason = excluded.reason,
                revoked_by = excluded.revoked_by,
                source = excluded.source,
                event_id = excluded.event_id
            "#,
            params![
                r.mandate_id,
                r.revoked_at.to_rfc3339(),
                r.reason,
                r.revoked_by,
                r.source,
                r.event_id,
            ],
        )?;
        Ok(())
    }

    /// Get revoked_at timestamp for a mandate (if revoked).
    pub fn get_revoked_at(&self, mandate_id: &str) -> Result<Option<DateTime<Utc>>, AuthzError> {
        let conn = self.conn.lock().unwrap();
        let s: Option<String> = conn
            .query_row(
                "SELECT revoked_at FROM mandate_revocations WHERE mandate_id = ?1",
                [mandate_id],
                |row| row.get(0),
            )
            .optional()?;

        match s {
            Some(ts) => {
                let dt = DateTime::parse_from_rfc3339(&ts)
                    .map_err(|e| {
                        AuthzError::Database(format!("Invalid revoked_at timestamp: {e}"))
                    })?
                    .with_timezone(&Utc);
                Ok(Some(dt))
            }
            None => Ok(None),
        }
    }

    /// Check if a mandate is revoked (convenience method).
    pub fn is_revoked(&self, mandate_id: &str) -> Result<bool, AuthzError> {
        Ok(self.get_revoked_at(mandate_id)?.is_some())
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
    let input = format!("{}:{}:{}", mandate_id, tool_call_id, use_count);
    let hash = Sha256::digest(input.as_bytes());
    format!("sha256:{}", hex::encode(hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata() -> MandateMetadata {
        MandateMetadata {
            mandate_id: "sha256:test123".to_string(),
            mandate_kind: "intent".to_string(),
            audience: "org/app".to_string(),
            issuer: "auth.org.com".to_string(),
            expires_at: None,
            single_use: false,
            max_uses: None,
            canonical_digest: "sha256:digest123".to_string(),
            key_id: "sha256:key123".to_string(),
        }
    }

    fn consume(
        store: &MandateStore,
        mandate_id: &str,
        tool_call_id: &str,
        nonce: Option<&str>,
        audience: &str,
        issuer: &str,
    ) -> Result<AuthzReceipt, AuthzError> {
        store.consume_mandate(&ConsumeParams {
            mandate_id,
            tool_call_id,
            nonce,
            audience,
            issuer,
            tool_name: "test_tool",
            operation_class: "read",
            source_run_id: None,
        })
    }

    // === A) Schema/migrations ===

    #[test]
    fn test_store_bootstraps_schema() {
        let store = MandateStore::memory().unwrap();
        let conn = store.conn.lock().unwrap();

        // Check tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"mandates".to_string()));
        assert!(tables.contains(&"mandate_uses".to_string()));
        assert!(tables.contains(&"nonces".to_string()));
    }

    #[test]
    fn test_store_sets_foreign_keys() {
        let store = MandateStore::memory().unwrap();
        let conn = store.conn.lock().unwrap();

        let fk: i32 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    // === B) Upsert invariants ===

    #[test]
    fn test_upsert_mandate_inserts_new() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();

        store.upsert_mandate(&meta).unwrap();

        let count = store.get_use_count(&meta.mandate_id).unwrap();
        assert_eq!(count, Some(0));
    }

    #[test]
    fn test_upsert_mandate_is_noop_on_same_content() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();

        store.upsert_mandate(&meta).unwrap();
        store.upsert_mandate(&meta).unwrap(); // Should not error

        let count = store.get_use_count(&meta.mandate_id).unwrap();
        assert_eq!(count, Some(0));
    }

    #[test]
    fn test_upsert_mandate_rejects_conflicting_metadata() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();
        store.upsert_mandate(&meta).unwrap();

        // Try to upsert with different audience
        let mut conflict = meta.clone();
        conflict.audience = "different/app".to_string();

        let result = store.upsert_mandate(&conflict);
        assert!(matches!(
            result,
            Err(AuthzError::MandateConflict { field, .. }) if field == "audience"
        ));
    }

    // === C) Consume - idempotency & counts ===

    #[test]
    fn test_consume_fails_if_mandate_missing() {
        let store = MandateStore::memory().unwrap();

        let result = consume(
            &store,
            "sha256:nonexistent",
            "tc_1",
            None,
            "org/app",
            "auth.org.com",
        );

        assert!(matches!(result, Err(AuthzError::MandateNotFound { .. })));
    }

    #[test]
    fn test_consume_first_time_returns_use_count_1() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();
        store.upsert_mandate(&meta).unwrap();

        let receipt = consume(
            &store,
            &meta.mandate_id,
            "tc_1",
            None,
            &meta.audience,
            &meta.issuer,
        )
        .unwrap();

        assert_eq!(receipt.use_count, 1);
        assert!(!receipt.use_id.is_empty());
        assert_eq!(receipt.tool_call_id, "tc_1");
        assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
        assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 1);
    }

    #[test]
    fn test_consume_is_idempotent_for_same_tool_call_id() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();
        store.upsert_mandate(&meta).unwrap();

        let receipt1 = consume(
            &store,
            &meta.mandate_id,
            "tc_1",
            None,
            &meta.audience,
            &meta.issuer,
        )
        .unwrap();
        let receipt2 = consume(
            &store,
            &meta.mandate_id,
            "tc_1",
            None,
            &meta.audience,
            &meta.issuer,
        )
        .unwrap();

        // Same receipt (idempotent)
        assert_eq!(receipt1.use_id, receipt2.use_id);
        assert_eq!(receipt1.use_count, receipt2.use_count);

        // was_new distinguishes first vs retry
        assert!(receipt1.was_new, "First consume should be was_new=true");
        assert!(!receipt2.was_new, "Retry should be was_new=false");

        // Count didn't increment
        assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
        assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 1);
    }

    #[test]
    fn test_consume_increments_for_different_tool_call_ids() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();
        store.upsert_mandate(&meta).unwrap();

        let r1 = consume(
            &store,
            &meta.mandate_id,
            "tc_1",
            None,
            &meta.audience,
            &meta.issuer,
        )
        .unwrap();
        let r2 = consume(
            &store,
            &meta.mandate_id,
            "tc_2",
            None,
            &meta.audience,
            &meta.issuer,
        )
        .unwrap();

        assert_eq!(r1.use_count, 1);
        assert_eq!(r2.use_count, 2);
        assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(2));
        assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 2);
    }

    // === D) Constraints - single_use/max_uses ===

    #[test]
    fn test_single_use_allows_first_then_rejects_second() {
        let store = MandateStore::memory().unwrap();
        let mut meta = test_metadata();
        meta.single_use = true;
        meta.max_uses = Some(1);
        store.upsert_mandate(&meta).unwrap();

        let r1 = consume(
            &store,
            &meta.mandate_id,
            "tc_1",
            None,
            &meta.audience,
            &meta.issuer,
        );
        assert!(r1.is_ok());

        let r2 = consume(
            &store,
            &meta.mandate_id,
            "tc_2",
            None,
            &meta.audience,
            &meta.issuer,
        );
        assert!(matches!(r2, Err(AuthzError::AlreadyUsed)));

        // use_count stayed at 1 (rollback worked)
        assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
    }

    #[test]
    fn test_max_uses_allows_up_to_n_then_rejects() {
        let store = MandateStore::memory().unwrap();
        let mut meta = test_metadata();
        meta.max_uses = Some(3);
        store.upsert_mandate(&meta).unwrap();

        for i in 1..=3 {
            let r = consume(
                &store,
                &meta.mandate_id,
                &format!("tc_{}", i),
                None,
                &meta.audience,
                &meta.issuer,
            );
            assert!(r.is_ok(), "Call {} should succeed", i);
        }

        let r4 = consume(
            &store,
            &meta.mandate_id,
            "tc_4",
            None,
            &meta.audience,
            &meta.issuer,
        );
        assert!(matches!(
            r4,
            Err(AuthzError::MaxUsesExceeded { max: 3, current: 4 })
        ));

        // use_count stayed at 3
        assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(3));
    }

    #[test]
    fn test_max_uses_null_is_unlimited() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata(); // max_uses = None
        store.upsert_mandate(&meta).unwrap();

        for i in 1..=20 {
            let r = consume(
                &store,
                &meta.mandate_id,
                &format!("tc_{}", i),
                None,
                &meta.audience,
                &meta.issuer,
            );
            assert!(r.is_ok(), "Call {} should succeed", i);
        }

        assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(20));
    }

    #[test]
    fn test_single_use_true_and_max_uses_gt_1_is_invalid() {
        let store = MandateStore::memory().unwrap();
        let mut meta = test_metadata();
        meta.single_use = true;
        meta.max_uses = Some(10); // Invalid: single_use with max > 1

        let result = store.upsert_mandate(&meta);
        assert!(matches!(
            result,
            Err(AuthzError::InvalidConstraints { max_uses: 10 })
        ));
    }

    // === E) Nonce replay prevention ===

    #[test]
    fn test_nonce_inserted_on_first_consume() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();
        store.upsert_mandate(&meta).unwrap();

        consume(
            &store,
            &meta.mandate_id,
            "tc_1",
            Some("nonce_123"),
            &meta.audience,
            &meta.issuer,
        )
        .unwrap();

        assert!(store
            .nonce_exists(&meta.audience, &meta.issuer, "nonce_123")
            .unwrap());
    }

    #[test]
    fn test_nonce_replay_rejected() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();
        store.upsert_mandate(&meta).unwrap();

        let r1 = consume(
            &store,
            &meta.mandate_id,
            "tc_1",
            Some("nonce_123"),
            &meta.audience,
            &meta.issuer,
        );
        assert!(r1.is_ok());

        // Different tool_call_id, same nonce
        let r2 = consume(
            &store,
            &meta.mandate_id,
            "tc_2",
            Some("nonce_123"),
            &meta.audience,
            &meta.issuer,
        );
        assert!(matches!(r2, Err(AuthzError::NonceReplay { nonce }) if nonce == "nonce_123"));

        // use_count stayed at 1 (rollback worked)
        assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
    }

    #[test]
    fn test_nonce_scope_is_audience_and_issuer() {
        let store = MandateStore::memory().unwrap();

        // Create two mandates with different audience
        let meta1 = test_metadata();
        let mut meta2 = test_metadata();
        meta2.mandate_id = "sha256:test456".to_string();
        meta2.audience = "different/app".to_string();
        meta2.canonical_digest = "sha256:digest456".to_string();

        store.upsert_mandate(&meta1).unwrap();
        store.upsert_mandate(&meta2).unwrap();

        // Same nonce, different audience should be allowed
        let r1 = consume(
            &store,
            &meta1.mandate_id,
            "tc_1",
            Some("shared_nonce"),
            &meta1.audience,
            &meta1.issuer,
        );
        assert!(r1.is_ok());

        // Same nonce but different audience
        let r2 = consume(
            &store,
            &meta2.mandate_id,
            "tc_2",
            Some("shared_nonce"),
            &meta2.audience,
            &meta2.issuer,
        );
        assert!(
            r2.is_ok(),
            "Same nonce with different audience should be allowed"
        );
    }

    // === F) Multi-call invariants (serialized via mutex) ===

    #[test]
    fn test_multicall_produces_monotonic_counts_no_gaps() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();
        store.upsert_mandate(&meta).unwrap();

        let mut counts = Vec::new();
        for i in 1..=50 {
            let r = consume(
                &store,
                &meta.mandate_id,
                &format!("tc_{}", i),
                None,
                &meta.audience,
                &meta.issuer,
            )
            .unwrap();
            counts.push(r.use_count);
        }

        // Verify monotonic: 1, 2, 3, ..., 50
        let expected: Vec<u32> = (1..=50).collect();
        assert_eq!(counts, expected);
        assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(50));
        assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 50);
    }

    #[test]
    fn test_multicall_idempotent_same_tool_call_id() {
        let store = MandateStore::memory().unwrap();
        let meta = test_metadata();
        store.upsert_mandate(&meta).unwrap();

        let mut receipts = Vec::new();
        for _ in 0..20 {
            let r = consume(
                &store,
                &meta.mandate_id,
                "tc_same",
                None,
                &meta.audience,
                &meta.issuer,
            )
            .unwrap();
            receipts.push(r);
        }

        // All receipts should be identical
        let first = &receipts[0];
        for r in &receipts {
            assert_eq!(r.use_id, first.use_id);
            assert_eq!(r.use_count, first.use_count);
        }

        // Only one actual use
        assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
        assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 1);
    }

    // === H) Revocation API ===

    #[test]
    fn test_revocation_roundtrip() {
        let store = MandateStore::memory().unwrap();

        let revoked_at = Utc::now();
        let record = RevocationRecord {
            mandate_id: "sha256:revoked123".to_string(),
            revoked_at,
            reason: Some("User requested".to_string()),
            revoked_by: Some("admin@example.com".to_string()),
            source: Some("assay://myorg/myapp".to_string()),
            event_id: Some("evt_revoke_001".to_string()),
        };

        store.upsert_revocation(&record).unwrap();

        let got = store.get_revoked_at(&record.mandate_id).unwrap();
        assert!(got.is_some());
        // Compare within 1 second tolerance (RFC3339 loses sub-second precision)
        let diff = (got.unwrap() - revoked_at).num_seconds().abs();
        assert!(diff <= 1, "revoked_at timestamps differ by {}s", diff);
    }

    #[test]
    fn test_revocation_is_revoked_helper() {
        let store = MandateStore::memory().unwrap();

        assert!(!store.is_revoked("sha256:not_revoked").unwrap());

        store
            .upsert_revocation(&RevocationRecord {
                mandate_id: "sha256:is_revoked".to_string(),
                revoked_at: Utc::now(),
                reason: None,
                revoked_by: None,
                source: None,
                event_id: None,
            })
            .unwrap();

        assert!(store.is_revoked("sha256:is_revoked").unwrap());
    }

    #[test]
    fn test_revocation_upsert_is_idempotent() {
        let store = MandateStore::memory().unwrap();

        let record = RevocationRecord {
            mandate_id: "sha256:idem".to_string(),
            revoked_at: Utc::now(),
            reason: Some("First".to_string()),
            revoked_by: None,
            source: None,
            event_id: None,
        };

        store.upsert_revocation(&record).unwrap();
        store.upsert_revocation(&record).unwrap(); // Should not fail
        store.upsert_revocation(&record).unwrap();

        assert!(store.is_revoked(&record.mandate_id).unwrap());
    }
}
