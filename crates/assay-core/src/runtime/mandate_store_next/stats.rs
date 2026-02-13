use super::super::{AuthzError, MandateStore};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

pub(crate) fn get_use_count_impl(
    store: &MandateStore,
    mandate_id: &str,
) -> Result<Option<u32>, AuthzError> {
    let conn = store.conn.lock().unwrap();
    let count: Option<i64> = conn
        .query_row(
            "SELECT use_count FROM mandates WHERE mandate_id = ?",
            [mandate_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(count.map(|c| c as u32))
}

pub(crate) fn count_uses_impl(store: &MandateStore, mandate_id: &str) -> Result<u32, AuthzError> {
    let conn = store.conn.lock().unwrap();
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM mandate_uses WHERE mandate_id = ?",
        [mandate_id],
        |row| row.get(0),
    )?;
    Ok(count as u32)
}

pub(crate) fn nonce_exists_impl(
    store: &MandateStore,
    audience: &str,
    issuer: &str,
    nonce: &str,
) -> Result<bool, AuthzError> {
    let conn = store.conn.lock().unwrap();
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM nonces WHERE audience = ? AND issuer = ? AND nonce = ?",
        params![audience, issuer, nonce],
        |row| row.get(0),
    )?;
    Ok(exists > 0)
}

pub(crate) fn compute_use_id_impl(mandate_id: &str, tool_call_id: &str, use_count: u32) -> String {
    let input = format!("{}:{}:{}", mandate_id, tool_call_id, use_count);
    let hash = Sha256::digest(input.as_bytes());
    format!("sha256:{}", hex::encode(hash))
}
