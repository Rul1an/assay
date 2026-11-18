//! Episode graph boundary for storage store.
//!
//! Intended ownership (Commit B):
//! - trace event insert/batch and episode graph load paths

use rusqlite::{params, Connection};

pub(crate) fn load_episode_graph_for_episode_id_impl(
    conn: &Connection,
    episode_id: &str,
) -> anyhow::Result<crate::agent_assertions::EpisodeGraph> {
    // The converter writes at most two bounded strings, each JSON byte can
    // expand to six bytes (a Unicode escape), plus the three fixed keys.
    const MAX_LITERAL_META_BYTES: usize =
        2 * 6 * crate::trace::truncation::INGEST_STRING_CEILING + 128;

    let mut stmt_steps = conn.prepare(
        "SELECT id, episode_id, idx, kind, name, content,
                CASE WHEN content IS NULL AND kind = 'llm_completion' AND octet_length(content_sha256) = 64
                          AND octet_length(meta_json) <= ?2
                     THEN meta_json END,
                CASE WHEN octet_length(content_sha256) = 64 THEN content_sha256 END
         FROM steps
         WHERE episode_id = ?1
         ORDER BY idx ASC",
    )?;
    let step_rows = stmt_steps
        .query_map(params![episode_id, MAX_LITERAL_META_BYTES], |row| {
            let content = if let Some(encoded) = row.get_ref(6)?.as_str_or_null()? {
                let digest = row.get_ref(7)?.as_str()?;
                converted_literal_content(encoded, digest)
            } else {
                row.get(5)?
            };
            Ok(crate::storage::rows::StepRow {
                id: row.get(0)?,
                episode_id: row.get(1)?,
                idx: row.get(2)?,
                kind: row.get(3)?,
                name: row.get(4)?,
                content,
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

/// A contentless native step can also carry replay metadata. Only project a
/// converter's literal when the stored bytes still match the original digest.
fn converted_literal_content(encoded: &str, digest: &str) -> Option<String> {
    let meta: serde_json::Value = serde_json::from_str(encoded).ok()?;
    let literal = meta.get("gen_ai.completion")?.as_str()?;
    (meta[crate::trace::schema::CONTENT_IN_COMPLETION] == true
        && literal.len() <= crate::trace::truncation::INGEST_STRING_CEILING
        && crate::trace::truncation::compute_sha256_str(literal) == digest)
        .then(|| literal.to_owned())
}
