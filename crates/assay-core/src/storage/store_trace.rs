use super::*;
use crate::trace::observation::{
    episode_column_values, step_column_values, tool_call_column_values, tool_call_target_key,
};

impl Store {
    // --- Trace V2 Storage ---

    pub fn insert_event(
        &self,
        event: &TraceEvent,
        run_id: Option<i64>,
        test_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        match event {
            TraceEvent::EpisodeStart(e) => Self::insert_episode(&tx, e, run_id, test_id)?,
            TraceEvent::Step(e) => Self::insert_step(&tx, e)?,
            TraceEvent::ToolCall(e) => {
                Self::insert_tool_call(&tx, e)?;
            }
            TraceEvent::EpisodeEnd(e) => Self::update_episode_end(&tx, e)?,
        }
        tx.commit()?;
        Ok(())
    }

    pub fn insert_batch(
        &self,
        events: &[TraceEvent],
        run_id: Option<i64>,
        test_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for event in events {
            match event {
                TraceEvent::EpisodeStart(e) => Self::insert_episode(&tx, e, run_id, test_id)?,
                TraceEvent::Step(e) => Self::insert_step(&tx, e)?,
                TraceEvent::ToolCall(e) => {
                    Self::insert_tool_call(&tx, e)?;
                }
                TraceEvent::EpisodeEnd(e) => Self::update_episode_end(&tx, e)?,
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn count_rows(&self, table: &str) -> anyhow::Result<i64> {
        let conn = self.conn.lock().unwrap();
        if !["episodes", "steps", "tool_calls", "runs", "results"].contains(&table) {
            anyhow::bail!("Invalid table name for count_rows: {}", table);
        }
        let sql = format!("SELECT COUNT(*) FROM {}", table);
        let n: i64 = conn.query_row(&sql, [], |r| r.get(0))?;
        Ok(n)
    }

    pub fn get_latest_episode_graph_by_test_id(
        &self,
        test_id: &str,
    ) -> anyhow::Result<crate::agent_assertions::EpisodeGraph> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id FROM episodes
             WHERE test_id = ?1
             ORDER BY timestamp DESC
             LIMIT 1",
        )?;

        let episode_id: String = stmt.query_row(params![test_id], |row| row.get(0)).map_err(
            |e| {
                anyhow::anyhow!(
                    "E_TRACE_EPISODE_MISSING: No episode found for test_id={} (fallback check) : {}",
                    test_id,
                    e
                )
            },
        )?;

        load_episode_graph_for_episode_id(&conn, &episode_id)
    }

    pub(crate) fn insert_episode(
        tx: &rusqlite::Transaction<'_>,
        e: &EpisodeStart,
        run_id: Option<i64>,
        test_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let (prompt_str, meta) = episode_column_values(e);

        let meta_test_id = e.meta.get("test_id").and_then(|v| v.as_str());
        let effective_test_id = test_id.or(meta_test_id).or(Some(&e.episode_id));

        tx.execute(
            "INSERT INTO episodes (id, run_id, test_id, timestamp, prompt, meta_json) VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                run_id=COALESCE(excluded.run_id, episodes.run_id),
                test_id=COALESCE(excluded.test_id, episodes.test_id),
                timestamp=excluded.timestamp,
                prompt=excluded.prompt,
                meta_json=excluded.meta_json",
            (
                &e.episode_id,
                run_id,
                effective_test_id,
                e.timestamp,
                prompt_str,
                meta,
            ),
        )
        .context("insert episode")?;
        Self::delete_observations(tx, "episode_start", &e.episode_id)?;
        Ok(())
    }

    pub(crate) fn insert_step(tx: &rusqlite::Transaction<'_>, e: &StepEntry) -> anyhow::Result<()> {
        let (_content, meta) = step_column_values(e);
        let trunc = serde_json::to_string(&e.truncations).unwrap_or_default();

        tx.execute(
            "INSERT INTO steps (id, episode_id, idx, kind, name, content, content_sha256, truncations_json, meta_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET content=excluded.content, content_sha256=excluded.content_sha256, truncations_json=excluded.truncations_json, meta_json=excluded.meta_json",
            (
                &e.step_id,
                &e.episode_id,
                e.idx,
                &e.kind,
                e.name.as_deref(),
                e.content.as_deref(),
                e.content_sha256.as_deref(),
                trunc,
                meta,
            ),
        )
        .context("insert step")?;
        Self::delete_observations(tx, "step", &e.step_id)?;
        Ok(())
    }

    pub(crate) fn insert_tool_call(
        tx: &rusqlite::Transaction<'_>,
        e: &ToolCallEntry,
    ) -> anyhow::Result<bool> {
        let (args, result) = tool_call_column_values(e);
        let trunc = serde_json::to_string(&e.truncations).unwrap_or_default();

        let call_idx = e.call_index.unwrap_or(0);

        let n = tx
            .execute(
            "INSERT INTO tool_calls (step_id, episode_id, tool_name, call_index, args, args_sha256, result, result_sha256, error, truncations_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(step_id, call_index) DO NOTHING",
            (
                &e.step_id,
                &e.episode_id,
                &e.tool_name,
                call_idx,
                args,
                e.args_sha256.as_deref(),
                result,
                e.result_sha256.as_deref(),
                e.error.as_deref(),
                trunc,
            ),
        )
            .context("insert tool call")?;
        if n > 0 {
            Self::delete_observations(
                tx,
                "tool_call",
                &tool_call_target_key(&e.step_id, call_idx),
            )?;
        }
        Ok(n > 0)
    }

    pub(crate) fn update_episode_end(
        tx: &rusqlite::Transaction<'_>,
        e: &EpisodeEnd,
    ) -> anyhow::Result<()> {
        tx.execute(
            "UPDATE episodes SET outcome = ? WHERE id = ?",
            (e.outcome.as_deref(), &e.episode_id),
        )
        .context("update episode outcome")?;
        Ok(())
    }
}
