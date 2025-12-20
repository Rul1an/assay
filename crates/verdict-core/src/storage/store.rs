use crate::model::{AttemptRow, EvalConfig, LlmResponse, TestResultRow, TestStatus};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn init_schema(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(crate::storage::schema::DDL)?;
        Ok(())
    }

    pub fn create_run(&self, cfg: &EvalConfig) -> anyhow::Result<i64> {
        let started_at = now_rfc3339ish();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO runs(suite, started_at, status, config_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                cfg.suite,
                started_at,
                "running",
                serde_json::to_string(cfg)?
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn finalize_run(&self, run_id: i64, status: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE runs SET status=?1 WHERE id=?2",
            params![status, run_id],
        )?;
        Ok(())
    }

    pub fn insert_result_embedded(
        &self,
        run_id: i64,
        row: &TestResultRow,
        attempts: &[AttemptRow],
        output: &LlmResponse,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO results(run_id, test_id, outcome, score, duration_ms, attempts_json, output_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run_id,
                row.test_id,
                status_to_outcome(&row.status),
                row.score,
                row.duration_ms.map(|v| v as i64),
                serde_json::to_string(attempts)?,
                serde_json::to_string(output)?,
            ],
        )?;
        Ok(())
    }

    // quarantine
    pub fn quarantine_get_reason(
        &self,
        suite: &str,
        test_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT reason FROM quarantine WHERE suite=?1 AND test_id=?2")?;
        let mut rows = stmt.query(params![suite, test_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get::<_, Option<String>>(0)?.unwrap_or_default()))
        } else {
            Ok(None)
        }
    }

    pub fn quarantine_add(&self, suite: &str, test_id: &str, reason: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO quarantine(suite, test_id, reason, added_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(suite, test_id) DO UPDATE SET reason=excluded.reason, added_at=excluded.added_at",
            params![suite, test_id, reason, now_rfc3339ish()],
        )?;
        Ok(())
    }

    pub fn quarantine_remove(&self, suite: &str, test_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM quarantine WHERE suite=?1 AND test_id=?2",
            params![suite, test_id],
        )?;
        Ok(())
    }

    // cache
    pub fn cache_get(&self, key: &str) -> anyhow::Result<Option<LlmResponse>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT response_json FROM cache WHERE key=?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let s: String = row.get(0)?;
            let mut resp: LlmResponse = serde_json::from_str(&s)?;
            resp.cached = true;
            Ok(Some(resp))
        } else {
            Ok(None)
        }
    }

    pub fn cache_put(&self, key: &str, resp: &LlmResponse) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let created_at = now_rfc3339ish();
        let mut to_store = resp.clone();
        to_store.cached = false;
        conn.execute(
            "INSERT INTO cache(key, response_json, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET response_json=excluded.response_json, created_at=excluded.created_at",
            params![key, serde_json::to_string(&to_store)?, created_at],
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

fn status_to_outcome(s: &TestStatus) -> &'static str {
    match s {
        TestStatus::Pass => "pass",
        TestStatus::Fail => "fail",
        TestStatus::Flaky => "flaky",
        TestStatus::Warn => "warn",
        TestStatus::Error => "error",
    }
}
