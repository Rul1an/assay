use super::super::{AuthzError, MandateStore};
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub(crate) fn open_impl(path: &Path) -> Result<MandateStore, AuthzError> {
    let conn = Connection::open(path)?;
    init_connection_impl(&conn)?;
    Ok(MandateStore {
        conn: Arc::new(Mutex::new(conn)),
    })
}

pub(crate) fn memory_impl() -> Result<MandateStore, AuthzError> {
    let conn = Connection::open_in_memory()?;
    init_connection_impl(&conn)?;
    Ok(MandateStore {
        conn: Arc::new(Mutex::new(conn)),
    })
}

pub(crate) fn from_connection_impl(conn: Connection) -> Result<MandateStore, AuthzError> {
    init_connection_impl(&conn)?;
    Ok(MandateStore {
        conn: Arc::new(Mutex::new(conn)),
    })
}

pub(crate) fn init_connection_impl(conn: &Connection) -> Result<(), AuthzError> {
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    let _ = conn.execute("PRAGMA journal_mode = WAL", []);
    let _ = conn.execute("PRAGMA busy_timeout = 5000", []);
    conn.execute_batch(crate::runtime::MANDATE_SCHEMA)?;
    Ok(())
}
