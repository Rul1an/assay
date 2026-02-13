use super::super::{AuthzError, AuthzReceipt, ConsumeParams};
use super::stats as stats_next;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

pub(crate) fn consume_mandate_inner_impl(
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

    let existing: Option<(String, i64, String)> = conn
        .query_row(
            "SELECT use_id, use_count, consumed_at FROM mandate_uses WHERE tool_call_id = ?",
            [tool_call_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    if let Some((use_id, use_count, consumed_at)) = existing {
        return Ok(AuthzReceipt {
            mandate_id: mandate_id.to_string(),
            use_id,
            use_count: use_count as u32,
            consumed_at: DateTime::parse_from_rfc3339(&consumed_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            tool_call_id: tool_call_id.to_string(),
            was_new: false,
        });
    }

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

    conn.execute(
        "UPDATE mandates SET use_count = ?1 WHERE mandate_id = ?2",
        params![new_count, mandate_id],
    )?;

    let use_id = stats_next::compute_use_id_impl(mandate_id, tool_call_id, new_count as u32);
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
        was_new: true,
    })
}
