use super::super::{AuthzError, MandateStore, RevocationRecord};
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

pub(crate) fn upsert_revocation_impl(
    store: &MandateStore,
    r: &RevocationRecord,
) -> Result<(), AuthzError> {
    let conn = store.conn.lock().unwrap();
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

pub(crate) fn get_revoked_at_impl(
    store: &MandateStore,
    mandate_id: &str,
) -> Result<Option<DateTime<Utc>>, AuthzError> {
    let conn = store.conn.lock().unwrap();
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
                .map_err(|e| AuthzError::Database(format!("Invalid revoked_at timestamp: {e}")))?
                .with_timezone(&Utc);
            Ok(Some(dt))
        }
        None => Ok(None),
    }
}

pub(crate) fn is_revoked_impl(store: &MandateStore, mandate_id: &str) -> Result<bool, AuthzError> {
    Ok(get_revoked_at_impl(store, mandate_id)?.is_some())
}
