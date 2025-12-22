use crate::model::{AttemptRow, EvalConfig, LlmResponse, TestResultRow, TestStatus};
use anyhow::Context;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Store {
    pub(crate) conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path).context("failed to open sqlite db")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn memory() -> anyhow::Result<Self> {
        // SQLite in-memory DB
        let conn = Connection::open_in_memory().context("failed to open in-memory sqlite db")?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn init_schema(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(crate::storage::schema::DDL)?;

        // v0.3.0 Migrations
        migrate_v030(&conn)?;

        // Ensure attempts table exists (covered by DDL if creating fresh, but good to be explicit if DDL didn't run on existing DB)
        // DDL handles IF NOT EXISTS for attempts.

        // Index on fingerprint for speed (CREATE INDEX IF NOT EXISTS is valid sqlite)
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_results_fingerprint ON results(fingerprint)",
            [],
        );

        Ok(())
    }

    pub fn fetch_recent_results(
        &self,
        suite: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<crate::model::TestResultRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT
                r.test_id, r.outcome, r.duration_ms, r.score, r.attempts_json,
                r.fingerprint, r.skip_reason
             FROM results r
             JOIN runs ON r.run_id = runs.id
             WHERE runs.suite = ?1
             ORDER BY r.id DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(rusqlite::params![suite, limit], |row| {
            let attempts_str: Option<String> = row.get(4)?;

            // Rehydrate "details" from last attempt (important for calibration)
            let attempts: Option<Vec<crate::model::AttemptRow>> = match attempts_str {
                Some(s) if !s.trim().is_empty() => serde_json::from_str(&s).ok(),
                _ => None,
            };

            let (message, details) = attempts
                .as_ref()
                .and_then(|v| v.last())
                .map(|a| (a.message.clone(), a.details.clone()))
                .unwrap_or_else(|| (String::new(), serde_json::json!({})));

            let cached = false;

            Ok(crate::model::TestResultRow {
                test_id: row.get(0)?,
                status: crate::model::TestStatus::parse(&row.get::<_, String>(1)?),
                message,
                duration_ms: row.get(2)?,
                details,
                score: row.get(3)?,
                cached,
                fingerprint: row.get(5)?,
                skip_reason: row.get(6)?,
                attempts,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn fetch_results_for_last_n_runs(
        &self,
        suite: &str,
        n: u32,
    ) -> anyhow::Result<Vec<crate::model::TestResultRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT
                r.test_id, r.outcome, r.duration_ms, r.score, r.attempts_json,
                r.fingerprint, r.skip_reason
             FROM results r
             JOIN runs ON r.run_id = runs.id
             WHERE runs.id IN (
                 SELECT id FROM runs WHERE suite = ?1 ORDER BY id DESC LIMIT ?2
             )
             ORDER BY r.id DESC",
        )?;

        let rows = stmt.query_map(rusqlite::params![suite, n], |row| {
            let attempts_str: Option<String> = row.get(4)?;

            let (message, details) =
                if let Some(s) = attempts_str.as_ref().filter(|s| !s.trim().is_empty()) {
                    if let Ok(attempts) = serde_json::from_str::<Vec<crate::model::AttemptRow>>(s) {
                        attempts
                            .last()
                            .map(|a| (a.message.clone(), a.details.clone()))
                            .unwrap_or_else(|| (String::new(), serde_json::json!({})))
                    } else {
                        (String::new(), serde_json::json!({}))
                    }
                } else {
                    (String::new(), serde_json::json!({}))
                };

            let attempts: Option<Vec<crate::model::AttemptRow>> =
                attempts_str.and_then(|s| serde_json::from_str(&s).ok());

            Ok(crate::model::TestResultRow {
                test_id: row.get(0)?,
                status: crate::model::TestStatus::parse(&row.get::<_, String>(1)?),
                message,
                duration_ms: row.get(2)?,
                details,
                score: row.get(3)?,
                cached: false,
                fingerprint: row.get(5)?,
                skip_reason: row.get(6)?,
                attempts,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn get_last_passing_by_fingerprint(
        &self,
        fingerprint: &str,
    ) -> anyhow::Result<Option<TestResultRow>> {
        let conn = self.conn.lock().unwrap();
        // We want the most recent passing result for this fingerprint.
        // run_id DESC ensures recency.
        let mut stmt = conn.prepare(
            "SELECT r.test_id, r.outcome, r.score, r.duration_ms, r.output_json, r.skip_reason, run.id, run.started_at
             FROM results r
             JOIN runs run ON r.run_id = run.id
             WHERE r.fingerprint = ?1 AND r.outcome = 'pass'
             ORDER BY r.id DESC LIMIT 1"
        )?;

        let mut rows = stmt.query(params![fingerprint])?;
        if let Some(row) = rows.next()? {
            let outcome: String = row.get(1)?;
            let status = match outcome.as_str() {
                "pass" => TestStatus::Pass,
                _ => TestStatus::Pass,
            };

            let skip_reason: Option<String> = row.get(5)?;
            let run_id: i64 = row.get(6)?;
            let started_at: String = row.get(7)?;

            let details = serde_json::json!({
                "skip": {
                    "reason": skip_reason.clone().unwrap_or_else(|| "fingerprint_match".into()),
                    "fingerprint": fingerprint,
                    "previous_run_id": run_id,
                    "previous_at": started_at,
                    "origin_run_id": run_id,
                    "previous_score": row.get::<_, Option<f64>>(2)?
                }
            });

            Ok(Some(TestResultRow {
                test_id: row.get(0)?,
                status,
                message: skip_reason.unwrap_or_else(|| "fingerprint_match".to_string()),
                score: row.get(2)?,
                duration_ms: row.get(3)?,
                cached: true,
                details,
                fingerprint: Some(fingerprint.to_string()),
                skip_reason: None,
                attempts: None,
            }))
        } else {
            Ok(None)
        }
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

        // 1. Insert into results
        conn.execute(
            "INSERT INTO results(run_id, test_id, outcome, score, duration_ms, attempts_json, output_json, fingerprint, skip_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                run_id,
                row.test_id,
                status_to_outcome(&row.status),
                row.score,
                row.duration_ms.map(|v| v as i64),
                serde_json::to_string(attempts)?,
                serde_json::to_string(output)?,
                row.fingerprint,
                row.skip_reason
            ],
        )?;

        let result_id = conn.last_insert_rowid();

        // 2. Insert individual attempts
        let mut stmt = conn.prepare(
            "INSERT INTO attempts(result_id, attempt_number, outcome, score, duration_ms, output_json, error_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        )?;

        for attempt in attempts {
            stmt.execute(params![
                result_id,
                attempt.attempt_no as i64,
                status_to_outcome(&attempt.status),
                0.0, // Score not tracked per attempt yet
                attempt.duration_ms.map(|v| v as i64),
                serde_json::to_string(&attempt.details)?,
                Option::<String>::None
            ])?;
        }

        Ok(())
    }

    // ... existing ...

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

    // embeddings
    pub fn get_embedding(&self, key: &str) -> anyhow::Result<Option<(String, Vec<f32>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT model, vec FROM embeddings WHERE key = ?1 LIMIT 1")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            let model: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            let vec = crate::embeddings::util::decode_vec_f32(&blob)?;
            Ok(Some((model, vec)))
        } else {
            Ok(None)
        }
    }

    pub fn put_embedding(&self, key: &str, model: &str, vec: &[f32]) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let blob = crate::embeddings::util::encode_vec_f32(vec);
        let dims = vec.len() as i64;
        let created_at = now_rfc3339ish();

        conn.execute(
            "INSERT OR REPLACE INTO embeddings (key, model, dims, vec, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![key, model, dims, blob, created_at],
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
        TestStatus::Skipped => "skipped",
        TestStatus::Unstable => "unstable",
    }
}

fn migrate_v030(conn: &Connection) -> anyhow::Result<()> {
    let cols = get_columns(conn, "results")?;
    add_column_if_missing(conn, &cols, "results", "fingerprint", "TEXT")?;
    add_column_if_missing(conn, &cols, "results", "skip_reason", "TEXT")?;
    add_column_if_missing(conn, &cols, "results", "attempts_json", "TEXT")?;
    Ok(())
}

fn get_columns(
    conn: &Connection,
    table: &str,
) -> anyhow::Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut out = std::collections::HashSet::new();
    for r in rows {
        out.insert(r?);
    }
    Ok(out)
}

fn add_column_if_missing(
    conn: &Connection,
    cols: &std::collections::HashSet<String>,
    table: &str,
    col: &str,
    ty: &str,
) -> anyhow::Result<()> {
    if !cols.contains(col) {
        let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, col, ty);
        conn.execute(&sql, [])?;
    }
    Ok(())
}
