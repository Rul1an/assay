//! Schema and migration boundary for storage store.
//!
//! Intended ownership (Commit B):
//! - schema initialization and migrations
//! - column/additive migration helpers

use anyhow::Context;
use rusqlite::Connection;
use std::collections::HashSet;

pub(crate) fn migrate_v030_impl(conn: &Connection) -> anyhow::Result<()> {
    let cols = get_columns_impl(conn, "results")?;
    add_column_if_missing_impl(conn, &cols, "results", "fingerprint", "TEXT")?;
    add_column_if_missing_impl(conn, &cols, "results", "skip_reason", "TEXT")?;
    add_column_if_missing_impl(conn, &cols, "results", "attempts_json", "TEXT")?;
    Ok(())
}

pub(crate) fn get_columns_impl(conn: &Connection, table: &str) -> anyhow::Result<HashSet<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .context("prepare pragma table_info")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut out = HashSet::new();
    for r in rows {
        out.insert(r?);
    }
    Ok(out)
}

pub(crate) fn add_column_if_missing_impl(
    conn: &Connection,
    cols: &HashSet<String>,
    table: &str,
    col: &str,
    ty: &str,
) -> anyhow::Result<()> {
    if !cols.contains(col) {
        let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, col, ty);
        conn.execute(&sql, []).context("alter table add column")?;
    }
    Ok(())
}
