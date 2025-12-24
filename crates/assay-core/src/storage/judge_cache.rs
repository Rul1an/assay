use crate::storage::Store;
use rusqlite::params;

#[derive(Clone)]
pub struct JudgeCache {
    store: Store,
}

impl JudgeCache {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub fn get(&self, key: &str) -> anyhow::Result<Option<serde_json::Value>> {
        let conn = self.store.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT payload_json FROM judge_cache WHERE key=?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let s: String = row.get(0)?;
            let val: serde_json::Value = serde_json::from_str(&s)?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }

    pub fn put(
        &self,
        key: &str,
        provider: &str,
        model: &str,
        rubric_id: &str,
        rubric_version: &str,
        payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let conn = self.store.conn.lock().unwrap();
        let payload_json = serde_json::to_string(payload)?;
        let created_at = now_rfc3339ish();

        conn.execute(
            "INSERT INTO judge_cache(
                key, provider, model, rubric_id, rubric_version, created_at, payload_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(key) DO UPDATE SET
                payload_json=excluded.payload_json,
                created_at=excluded.created_at",
            params![
                key,
                provider,
                model,
                rubric_id,
                rubric_version,
                created_at,
                payload_json
            ],
        )?;
        Ok(())
    }
}

fn now_rfc3339ish() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("unix:{}", secs)
}
