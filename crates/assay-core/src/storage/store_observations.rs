use super::*;
use crate::trace::observation::{
    bound_sha256, decode_stored_observation, episode_column_values, read_truncation,
    step_column_values, tool_call_column_values, tool_call_target_key, ObservedTraceEvent,
    TruncationObservation, TruncationReading,
};
use crate::trace::schema::{TraceEvent, TruncationMeta};
use anyhow::Context;

impl Store {
    pub fn insert_observed_event(
        &self,
        observed: &ObservedTraceEvent,
        run_id: Option<i64>,
        test_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        Self::insert_observed_in_tx(&tx, observed, run_id, test_id)?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_observed_batch(
        &self,
        events: &[ObservedTraceEvent],
        run_id: Option<i64>,
        test_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        for observed in events {
            Self::insert_observed_in_tx(&tx, observed, run_id, test_id)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn insert_observed_in_tx(
        tx: &rusqlite::Transaction<'_>,
        observed: &ObservedTraceEvent,
        run_id: Option<i64>,
        test_id: Option<&str>,
    ) -> anyhow::Result<()> {
        match observed.event() {
            TraceEvent::EpisodeStart(e) => {
                Self::insert_episode(tx, e, run_id, test_id)?;
                let (prompt, meta) = episode_column_values(e);
                let bound = bound_sha256(&[Some(prompt.as_str()), Some(meta.as_str())]);
                Self::write_observations(
                    tx,
                    "episode_start",
                    &e.episode_id,
                    observed.observations(),
                    &bound,
                )?;
            }
            TraceEvent::Step(e) => {
                Self::insert_step(tx, e)?;
                let (content, meta) = step_column_values(e);
                let bound = bound_sha256(&[content.as_deref(), Some(meta.as_str())]);
                Self::write_observations(tx, "step", &e.step_id, observed.observations(), &bound)?;
            }
            TraceEvent::ToolCall(e) => {
                let inserted = Self::insert_tool_call(tx, e)?;
                if inserted {
                    let (args, result) = tool_call_column_values(e);
                    let bound = bound_sha256(&[Some(args.as_str()), result.as_deref()]);
                    let key = tool_call_target_key(&e.step_id, e.call_index.unwrap_or(0));
                    Self::write_observations(
                        tx,
                        "tool_call",
                        &key,
                        observed.observations(),
                        &bound,
                    )?;
                }
            }
            TraceEvent::EpisodeEnd(e) => Self::update_episode_end(tx, e)?,
        }
        Ok(())
    }

    pub(crate) fn delete_observations(
        tx: &rusqlite::Transaction<'_>,
        kind: &str,
        key: &str,
    ) -> anyhow::Result<()> {
        tx.execute(
            "DELETE FROM trace_observations WHERE target_kind = ?1 AND target_key = ?2",
            rusqlite::params![kind, key],
        )
        .context("delete observations")?;
        Ok(())
    }

    fn write_observations(
        tx: &rusqlite::Transaction<'_>,
        kind: &str,
        key: &str,
        observations: &[TruncationObservation],
        bound: &str,
    ) -> anyhow::Result<()> {
        for (ordinal, obs) in observations.iter().enumerate() {
            let scope = serde_json::to_string(&obs.scope).unwrap_or_else(|_| "[]".to_string());
            let losses = serde_json::to_string(&obs.losses).unwrap_or_else(|_| "[]".to_string());
            tx.execute(
                "INSERT INTO trace_observations
                    (target_kind, target_key, ordinal, record_version, stage, ceiling, scope_json, losses_json, bound_sha256)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    kind,
                    key,
                    ordinal as i64,
                    obs.v as i64,
                    obs.stage,
                    obs.ceiling as i64,
                    scope,
                    losses,
                    bound,
                ],
            )
            .context("insert observation")?;
        }
        Ok(())
    }

    pub fn read_truncation(
        &self,
        kind: &str,
        key: &str,
        pointer: &str,
        trusted_stages: &[&str],
    ) -> anyhow::Result<TruncationReading> {
        let conn = self.conn.lock().unwrap();
        let (truncations, bound, require_parity) = match kind {
            "episode_start" => {
                let (prompt, meta): (Option<String>, Option<String>) = conn.query_row(
                    "SELECT prompt, meta_json FROM episodes WHERE id = ?1",
                    rusqlite::params![key],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let bound = bound_sha256(&[prompt.as_deref(), meta.as_deref()]);
                (Vec::new(), bound, false)
            }
            "step" => {
                let (content, meta, trunc_json): (Option<String>, Option<String>, Option<String>) =
                    conn.query_row(
                        "SELECT content, meta_json, truncations_json FROM steps WHERE id = ?1",
                        rusqlite::params![key],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )?;
                let bound = bound_sha256(&[content.as_deref(), meta.as_deref()]);
                (parse_truncations(trunc_json), bound, true)
            }
            "tool_call" => {
                let (step_id, call_index): (String, u32) = serde_json::from_str(key)
                    .context("tool_call target key must be JSON [step_id, call_index]")?;
                let (args, result, trunc_json): (Option<String>, Option<String>, Option<String>) =
                    conn.query_row(
                        "SELECT args, result, truncations_json FROM tool_calls
                         WHERE step_id = ?1 AND call_index = ?2",
                        rusqlite::params![step_id, call_index],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )?;
                let bound = bound_sha256(&[args.as_deref(), result.as_deref()]);
                (parse_truncations(trunc_json), bound, true)
            }
            other => anyhow::bail!("unknown observation target kind: {other}"),
        };

        let mut stmt = conn.prepare(
            "SELECT record_version, stage, ceiling, scope_json, losses_json, bound_sha256
             FROM trace_observations
             WHERE target_kind = ?1 AND target_key = ?2
             ORDER BY ordinal",
        )?;
        let rows = stmt.query_map(rusqlite::params![kind, key], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;

        let mut observations = Vec::new();
        for row in rows {
            let (version, stage, ceiling, scope_json, losses_json, stored_bound) = row?;
            if stored_bound != bound {
                continue;
            }
            if let Some(obs) =
                decode_stored_observation(version, stage, ceiling, &scope_json, &losses_json)
            {
                observations.push(obs);
            }
        }

        Ok(read_truncation(
            pointer,
            &truncations,
            &observations,
            trusted_stages,
            require_parity,
        ))
    }
}

fn parse_truncations(trunc_json: Option<String>) -> Vec<TruncationMeta> {
    trunc_json
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
