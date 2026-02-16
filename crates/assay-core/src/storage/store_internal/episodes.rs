//! Episode graph boundary for storage store.
//!
//! Intended ownership (Commit B):
//! - trace event insert/batch and episode graph load paths

use rusqlite::{params, Connection};

pub(crate) fn load_episode_graph_for_episode_id_impl(
    conn: &Connection,
    episode_id: &str,
) -> anyhow::Result<crate::agent_assertions::EpisodeGraph> {
    let mut stmt_steps = conn.prepare(
        "SELECT id, episode_id, idx, kind, name, content
         FROM steps
         WHERE episode_id = ?1
         ORDER BY idx ASC",
    )?;
    let step_rows = stmt_steps
        .query_map(params![episode_id], |row| {
            Ok(crate::storage::rows::StepRow {
                id: row.get(0)?,
                episode_id: row.get(1)?,
                idx: row.get(2)?,
                kind: row.get(3)?,
                name: row.get(4)?,
                content: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut stmt_tools = conn.prepare(
        "SELECT tc.id, tc.step_id, tc.episode_id, tc.tool_name, tc.call_index, tc.args, tc.result
         FROM tool_calls tc
         JOIN steps s ON tc.step_id = s.id
         WHERE tc.episode_id = ?1
         ORDER BY s.idx ASC, tc.call_index ASC",
    )?;
    let tool_rows = stmt_tools
        .query_map(params![episode_id], |row| {
            Ok(crate::storage::rows::ToolCallRow {
                id: row.get(0)?,
                step_id: row.get(1)?,
                episode_id: row.get(2)?,
                tool_name: row.get(3)?,
                call_index: row.get(4)?,
                args: row.get(5)?,
                result: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(crate::agent_assertions::EpisodeGraph {
        episode_id: episode_id.to_string(),
        steps: step_rows,
        tool_calls: tool_rows,
    })
}
