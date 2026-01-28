//! SQLite schema for mandate runtime enforcement.
//!
//! Tables:
//! - `mandates`: Immutable mandate metadata
//! - `mandate_uses`: Append-only consumption log
//! - `nonces`: Replay prevention for transaction mandates

/// DDL for mandate runtime enforcement tables.
///
/// Schema version: 2
pub const MANDATE_SCHEMA: &str = r#"
-- Mandate metadata (immutable after insert)
CREATE TABLE IF NOT EXISTS mandates (
    mandate_id       TEXT PRIMARY KEY,
    mandate_kind     TEXT NOT NULL,
    audience         TEXT NOT NULL,
    issuer           TEXT NOT NULL,
    expires_at       TEXT,
    single_use       INTEGER NOT NULL DEFAULT 0,
    max_uses         INTEGER,
    use_count        INTEGER NOT NULL DEFAULT 0,
    canonical_digest TEXT NOT NULL,
    key_id           TEXT NOT NULL,
    inserted_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Use tracking (append-only, immutable)
CREATE TABLE IF NOT EXISTS mandate_uses (
    use_id           TEXT PRIMARY KEY,
    mandate_id       TEXT NOT NULL REFERENCES mandates(mandate_id),
    tool_call_id     TEXT NOT NULL UNIQUE,
    use_count        INTEGER NOT NULL,
    consumed_at      TEXT NOT NULL,
    tool_name        TEXT,
    operation_class  TEXT,
    nonce            TEXT,
    source_run_id    TEXT,
    UNIQUE(mandate_id, use_count)
);

-- Nonce replay prevention (transaction mandates)
CREATE TABLE IF NOT EXISTS nonces (
    audience         TEXT NOT NULL,
    issuer           TEXT NOT NULL,
    nonce            TEXT NOT NULL,
    mandate_id       TEXT NOT NULL,
    first_seen_at    TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (audience, issuer, nonce)
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_mandates_audience_issuer
    ON mandates(audience, issuer);
CREATE INDEX IF NOT EXISTS idx_mandate_uses_mandate_id
    ON mandate_uses(mandate_id);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_schema_is_valid_sql() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MANDATE_SCHEMA).unwrap();
    }

    #[test]
    fn test_schema_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(MANDATE_SCHEMA).unwrap();
        conn.execute_batch(MANDATE_SCHEMA).unwrap();
    }
}
