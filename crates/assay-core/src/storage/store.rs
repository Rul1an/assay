use super::now_rfc3339ish;
use crate::model::{AttemptRow, EvalConfig, LlmResponse, TestResultRow, TestStatus};
use crate::trace::schema::{EpisodeEnd, EpisodeStart, StepEntry, ToolCallEntry, TraceEvent};
use anyhow::Context;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[path = "store_internal/mod.rs"]
mod store_internal;
#[path = "store_trace.rs"]
mod store_trace;

#[derive(Clone)]
pub struct Store {
    pub conn: Arc<Mutex<Connection>>,
}

pub struct StoreStats {
    pub runs: Option<u64>,
    pub results: Option<u64>,
    pub last_run_id: Option<i64>,
    pub last_run_at: Option<String>,
    pub version: Option<String>,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path).context("failed to open sqlite db")?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
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

        let rows = stmt.query_map(rusqlite::params![suite, limit], row_to_test_result)?;

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

        let rows = stmt.query_map(rusqlite::params![suite, n], row_to_test_result)?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    pub fn get_latest_run_id(&self, suite: &str) -> anyhow::Result<Option<i64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id FROM runs WHERE suite = ?1 ORDER BY id DESC LIMIT 1")?;
        let mut rows = stmt.query(params![suite])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn fetch_results_for_run(
        &self,
        run_id: i64,
    ) -> anyhow::Result<Vec<crate::model::TestResultRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT
                r.test_id, r.outcome, r.duration_ms, r.score, r.attempts_json,
                r.fingerprint, r.skip_reason
             FROM results r
             WHERE r.run_id = ?1
             ORDER BY r.test_id ASC",
        )?;

        let rows = stmt.query_map(params![run_id], row_to_test_result)?;

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
            "SELECT r.test_id, r.score, r.duration_ms, r.output_json, r.skip_reason, run.id, run.started_at
             FROM results r
             JOIN runs run ON r.run_id = run.id
             WHERE r.fingerprint = ?1 AND r.outcome = 'pass'
             ORDER BY r.id DESC LIMIT 1"
        )?;

        let mut rows = stmt.query(params![fingerprint])?;
        if let Some(row) = rows.next()? {
            let status = TestStatus::Pass;

            let skip_reason: Option<String> = row.get(4)?;
            let run_id: i64 = row.get(5)?;
            let started_at: String = row.get(6)?;

            let details = serde_json::json!({
                "skip": {
                    "reason": skip_reason.clone().unwrap_or_else(|| "fingerprint_match".into()),
                    "fingerprint": fingerprint,
                    "previous_run_id": run_id,
                    "previous_at": started_at,
                    "origin_run_id": run_id,
                    "previous_score": row.get::<_, Option<f64>>(1)?
                }
            });

            Ok(Some(TestResultRow {
                test_id: row.get(0)?,
                status,
                message: skip_reason.unwrap_or_else(|| "fingerprint_match".to_string()),
                score: row.get(1)?,
                duration_ms: row.get(2)?,
                cached: true,
                details,
                fingerprint: Some(fingerprint.to_string()),
                skip_reason: None,
                attempts: None,
                error_policy_applied: None,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn insert_run(&self, suite: &str) -> anyhow::Result<i64> {
        let started_at = now_rfc3339ish();
        let conn = self.conn.lock().unwrap();
        insert_run_row(&conn, suite, &started_at, "running", None)
    }

    pub fn create_run(&self, cfg: &EvalConfig) -> anyhow::Result<i64> {
        let started_at = now_rfc3339ish();
        let config_json = serde_json::to_string(cfg)?;
        let conn = self.conn.lock().unwrap();
        insert_run_row(
            &conn,
            &cfg.suite,
            &started_at,
            "running",
            Some(config_json.as_str()),
        )
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
    pub fn stats_best_effort(&self) -> anyhow::Result<StoreStats> {
        let conn = self.conn.lock().unwrap();

        let runs: Option<u64> = conn
            .query_row("SELECT COUNT(*) FROM runs", [], |r| {
                r.get::<_, i64>(0).map(|x| x as u64)
            })
            .ok();
        let results: Option<u64> = conn
            .query_row("SELECT COUNT(*) FROM results", [], |r| {
                r.get::<_, i64>(0).map(|x| x as u64)
            })
            .ok();

        let last: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, started_at FROM runs ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        let (last_id, last_started) = if let Some((id, s)) = last {
            (Some(id), Some(s))
        } else {
            (None, None)
        };

        let v_str: Option<String> = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .ok()
            .map(|v: i64| v.to_string());

        Ok(StoreStats {
            runs,
            results,
            last_run_id: last_id,
            last_run_at: last_started,
            version: v_str,
        })
    }

    // --- Assertions Support ---

    pub fn get_episode_graph(
        &self,
        run_id: i64,
        test_id: &str,
    ) -> anyhow::Result<crate::agent_assertions::EpisodeGraph> {
        let conn = self.conn.lock().unwrap();

        // 1. Find Episode
        let mut stmt = conn.prepare("SELECT id FROM episodes WHERE run_id = ? AND test_id = ?")?;
        let mut rows = stmt.query(params![run_id, test_id])?;

        let mut episode_ids = Vec::new();
        while let Some(row) = rows.next()? {
            episode_ids.push(row.get::<_, String>(0)?);
        }

        if episode_ids.is_empty() {
            anyhow::bail!(
                "E_TRACE_EPISODE_MISSING: No episode found for run_id={} test_id={}",
                run_id,
                test_id
            );
        }
        if episode_ids.len() > 1 {
            anyhow::bail!(
                "E_TRACE_EPISODE_AMBIGUOUS: Multiple episodes ({}) found for run_id={} test_id={}",
                episode_ids.len(),
                run_id,
                test_id
            );
        }
        let episode_id = episode_ids[0].clone();

        load_episode_graph_for_episode_id(&conn, &episode_id)
    }
}

fn status_to_outcome(s: &TestStatus) -> &'static str {
    store_internal::results::status_to_outcome_impl(s)
}

fn migrate_v030(conn: &Connection) -> anyhow::Result<()> {
    store_internal::schema::migrate_v030_impl(conn)
}

fn row_to_test_result(row: &rusqlite::Row<'_>) -> rusqlite::Result<TestResultRow> {
    store_internal::results::row_to_test_result_impl(row)
}

fn insert_run_row(
    conn: &Connection,
    suite: &str,
    started_at: &str,
    status: &str,
    config_json: Option<&str>,
) -> anyhow::Result<i64> {
    store_internal::results::insert_run_row_impl(conn, suite, started_at, status, config_json)
}

fn load_episode_graph_for_episode_id(
    conn: &Connection,
    episode_id: &str,
) -> anyhow::Result<crate::agent_assertions::EpisodeGraph> {
    store_internal::episodes::load_episode_graph_for_episode_id_impl(conn, episode_id)
}
