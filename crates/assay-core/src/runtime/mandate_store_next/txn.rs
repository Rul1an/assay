use super::super::{AuthzError, AuthzReceipt, ConsumeParams, MandateStore};

pub(crate) fn consume_mandate_in_txn_impl(
    store: &MandateStore,
    params: &ConsumeParams<'_>,
) -> Result<AuthzReceipt, AuthzError> {
    let conn = store.conn.lock().unwrap();

    conn.execute("BEGIN IMMEDIATE", [])?;
    let result = store.consume_mandate_inner(&conn, params);

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
