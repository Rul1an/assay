use super::super::{AuthzError, MandateMetadata, MandateStore};
use rusqlite::{params, OptionalExtension};

pub(crate) fn upsert_mandate_impl(
    store: &MandateStore,
    meta: &MandateMetadata,
) -> Result<(), AuthzError> {
    if meta.single_use {
        if let Some(max) = meta.max_uses {
            if max != 1 {
                return Err(AuthzError::InvalidConstraints { max_uses: max });
            }
        }
    }

    let conn = store.conn.lock().unwrap();

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
