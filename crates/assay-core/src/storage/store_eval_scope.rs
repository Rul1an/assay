use super::*;
use crate::trace::observation::tool_call_target_key;
use anyhow::Context;
use rusqlite::params;

impl Store {
    pub fn clear_assertion_eval_scope(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM assertion_eval_session WHERE kind IN ('episode', 'used')",
            [],
        )
        .context("clear assertion eval scope")?;
        Ok(())
    }

    pub fn record_assertion_eval_episode(&self, episode_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO assertion_eval_session (kind, key) VALUES ('episode', ?1)",
            params![episode_id],
        )
        .context("record assertion eval episode")?;
        Ok(())
    }

    pub fn bind_assertion_eval_scope(&self, run_id: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let latest: i64 = conn.query_row(
            "SELECT COUNT(*) FROM assertion_eval_session
             WHERE kind = 'opt' AND key = 'latest_stored_episode'",
            [],
            |r| r.get(0),
        )?;
        if latest > 0 {
            return Ok(());
        }
        conn.execute(
            "UPDATE episodes SET run_id = ?1
             WHERE id IN (SELECT key FROM assertion_eval_session WHERE kind = 'episode')",
            params![run_id],
        )
        .context("bind assertion eval scope")?;
        Ok(())
    }

    pub fn set_latest_stored_episode_eval(&self, enabled: bool) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        if enabled {
            conn.execute(
                "INSERT OR REPLACE INTO assertion_eval_session (kind, key) VALUES ('opt', 'latest_stored_episode')",
                [],
            )
            .context("enable latest stored episode eval")?;
        } else {
            conn.execute(
                "DELETE FROM assertion_eval_session WHERE kind = 'opt' AND key = 'latest_stored_episode'",
                [],
            )
            .context("disable latest stored episode eval")?;
        }
        Ok(())
    }

    pub fn latest_stored_episode_eval(&self) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM assertion_eval_session
             WHERE kind = 'opt' AND key = 'latest_stored_episode'",
            [],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn mark_latest_stored_episode_used(&self, test_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO assertion_eval_session (kind, key) VALUES ('used', ?1)",
            params![test_id],
        )
        .context("mark latest stored episode used")?;
        Ok(())
    }

    pub fn take_latest_stored_episode_used(&self) -> anyhow::Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT key FROM assertion_eval_session WHERE kind = 'used' ORDER BY key")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        conn.execute("DELETE FROM assertion_eval_session WHERE kind = 'used'", [])?;
        Ok(ids)
    }

    pub fn purge_episode(&self, episode_id: &str) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let mut step_ids = Vec::new();
        {
            let mut stmt = tx.prepare("SELECT id FROM steps WHERE episode_id = ?1")?;
            let mut rows = stmt.query(params![episode_id])?;
            while let Some(row) = rows.next()? {
                step_ids.push(row.get::<_, String>(0)?);
            }
        }
        let mut tool_keys = Vec::new();
        {
            let mut stmt =
                tx.prepare("SELECT step_id, call_index FROM tool_calls WHERE episode_id = ?1")?;
            let mut rows = stmt.query(params![episode_id])?;
            while let Some(row) = rows.next()? {
                tool_keys.push((row.get::<_, String>(0)?, row.get::<_, i64>(1)?));
            }
        }

        tx.execute(
            "DELETE FROM trace_observations WHERE target_kind = 'episode_start' AND target_key = ?1",
            params![episode_id],
        )?;
        for sid in &step_ids {
            tx.execute(
                "DELETE FROM trace_observations WHERE target_kind = 'step' AND target_key = ?1",
                params![sid],
            )?;
        }
        for (step_id, call_index) in &tool_keys {
            let key = tool_call_target_key(step_id, *call_index as u32);
            tx.execute(
                "DELETE FROM trace_observations WHERE target_kind = 'tool_call' AND target_key = ?1",
                params![key],
            )?;
        }
        tx.execute(
            "DELETE FROM tool_calls WHERE episode_id = ?1",
            params![episode_id],
        )?;
        tx.execute(
            "DELETE FROM steps WHERE episode_id = ?1",
            params![episode_id],
        )?;
        tx.commit()?;
        Ok(())
    }
}
