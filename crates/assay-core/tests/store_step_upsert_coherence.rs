//! Step upsert must refresh content provenance columns with content/meta.
//!
//! Bug pin: `insert_step` ON CONFLICT updated only `content` and `meta_json`,
//! leaving `content_sha256` and `truncations_json` stale relative to replacement content.
//!
//! Mutation notes (named assertions below must fail if either dependent assignment
//! is dropped from the ON CONFLICT SET list):
//! - Drop `content_sha256=excluded.content_sha256` → `replacement_content_sha256_matches_row` fails
//! - Drop `truncations_json=excluded.truncations_json` → `replacement_truncations_match_row`
//!   and/or `empty_truncations_clear_prior_loss_metadata` fails
//! - No-op control: first-insert path still persists both columns (`first_insert_persists_provenance`)

use assay_core::storage::store::Store;
use assay_core::trace::schema::{EpisodeStart, StepEntry, TraceEvent, TruncationMeta};
use rusqlite::params;

fn trunc(field: &str, original_len: usize, kept_len: usize, sha: &str) -> TruncationMeta {
    TruncationMeta {
        field: field.into(),
        original_len,
        kept_len,
        sha256: sha.into(),
        strategy: "head".into(),
    }
}

fn step(
    episode_id: &str,
    step_id: &str,
    content: &str,
    content_sha256: &str,
    truncations: Vec<TruncationMeta>,
    meta: serde_json::Value,
) -> TraceEvent {
    TraceEvent::Step(StepEntry {
        episode_id: episode_id.into(),
        step_id: step_id.into(),
        idx: 0,
        timestamp: 1001,
        kind: "model".into(),
        name: Some("agent".into()),
        content: Some(content.into()),
        content_sha256: Some(content_sha256.into()),
        truncations,
        meta,
    })
}

fn ensure_episode(store: &Store, episode_id: &str) -> anyhow::Result<()> {
    store.insert_event(
        &TraceEvent::EpisodeStart(EpisodeStart {
            episode_id: episode_id.into(),
            timestamp: 1000,
            input: serde_json::json!({"prompt": "hi"}),
            meta: serde_json::Value::Null,
        }),
        None,
        None,
    )?;
    Ok(())
}

struct StepProvenanceRow {
    content: Option<String>,
    content_sha256: Option<String>,
    truncations_json: Option<String>,
    meta_json: Option<String>,
}

fn read_step_provenance(store: &Store, step_id: &str) -> anyhow::Result<StepProvenanceRow> {
    let conn = store.conn.lock().unwrap();
    let row = conn.query_row(
        "SELECT content, content_sha256, truncations_json, meta_json FROM steps WHERE id = ?1",
        params![step_id],
        |r| {
            Ok(StepProvenanceRow {
                content: r.get(0)?,
                content_sha256: r.get(1)?,
                truncations_json: r.get(2)?,
                meta_json: r.get(3)?,
            })
        },
    )?;
    Ok(row)
}

#[test]
fn first_insert_persists_provenance() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;
    ensure_episode(&store, "ep-first")?;

    let truncations = vec![trunc("/content", 100, 10, "sha-first-trunc")];
    store.insert_event(
        &step(
            "ep-first",
            "step-first",
            "first-body",
            "sha-first-content",
            truncations.clone(),
            serde_json::json!({"k": "v1"}),
        ),
        None,
        None,
    )?;

    let row = read_step_provenance(&store, "step-first")?;
    assert_eq!(row.content.as_deref(), Some("first-body"));
    assert_eq!(
        row.content_sha256.as_deref(),
        Some("sha-first-content"),
        "first_insert_persists_content_sha256"
    );
    let expected_trunc = serde_json::to_string(&truncations)?;
    assert_eq!(
        row.truncations_json.as_deref(),
        Some(expected_trunc.as_str()),
        "first_insert_persists_truncations_json"
    );
    assert!(
        row.meta_json.as_deref().unwrap_or("").contains("v1"),
        "first_insert_persists_meta_json"
    );
    Ok(())
}

#[test]
fn step_upsert_refreshes_content_sha256_and_truncations() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;
    ensure_episode(&store, "ep-upsert")?;

    let initial_trunc = vec![trunc("/content", 200, 20, "sha-old-trunc")];
    store.insert_event(
        &step(
            "ep-upsert",
            "step-same",
            "old-content",
            "sha-old-content",
            initial_trunc,
            serde_json::json!({"gen": 1}),
        ),
        None,
        None,
    )?;

    let replacement_trunc = vec![
        trunc("/content", 300, 30, "sha-new-trunc-a"),
        trunc("/meta/note", 50, 5, "sha-new-trunc-b"),
    ];
    store.insert_event(
        &step(
            "ep-upsert",
            "step-same",
            "new-content-replacement",
            "sha-new-content",
            replacement_trunc.clone(),
            serde_json::json!({"gen": 2}),
        ),
        None,
        None,
    )?;

    let row = read_step_provenance(&store, "step-same")?;

    assert_eq!(
        row.content.as_deref(),
        Some("new-content-replacement"),
        "replacement_content_matches_row"
    );
    assert_eq!(
        row.content_sha256.as_deref(),
        Some("sha-new-content"),
        "replacement_content_sha256_matches_row"
    );
    let expected_trunc = serde_json::to_string(&replacement_trunc)?;
    assert_eq!(
        row.truncations_json.as_deref(),
        Some(expected_trunc.as_str()),
        "replacement_truncations_match_row"
    );
    assert!(
        row.meta_json.as_deref().unwrap_or("").contains("\"gen\":2")
            || row
                .meta_json
                .as_deref()
                .unwrap_or("")
                .contains("\"gen\": 2"),
        "replacement_meta_json_matches_row: {:?}",
        row.meta_json
    );

    // Identity: still one row for this step id
    assert_eq!(store.count_rows("steps")?, 1);

    Ok(())
}

#[test]
fn empty_truncations_clear_prior_loss_metadata() -> anyhow::Result<()> {
    let store = Store::memory()?;
    store.init_schema()?;
    ensure_episode(&store, "ep-clear")?;

    store.insert_event(
        &step(
            "ep-clear",
            "step-clear",
            "lossy",
            "sha-lossy",
            vec![trunc("/content", 999, 9, "sha-prior-loss")],
            serde_json::Value::Null,
        ),
        None,
        None,
    )?;

    // Nonempty → empty truncations control: replacement has no loss metadata.
    store.insert_event(
        &step(
            "ep-clear",
            "step-clear",
            "full-body",
            "sha-full",
            vec![],
            serde_json::Value::Null,
        ),
        None,
        None,
    )?;

    let row = read_step_provenance(&store, "step-clear")?;
    assert_eq!(row.content.as_deref(), Some("full-body"));
    assert_eq!(
        row.content_sha256.as_deref(),
        Some("sha-full"),
        "empty_truncations_control_refreshes_digest"
    );
    let expected_empty = serde_json::to_string(&Vec::<TruncationMeta>::new())?;
    assert_eq!(
        row.truncations_json.as_deref(),
        Some(expected_empty.as_str()),
        "empty_truncations_clear_prior_loss_metadata"
    );
    Ok(())
}

#[test]
fn step_upsert_sql_assigns_dependent_provenance_columns() {
    // Narrow SQL-shape guard: source must assign both dependent columns in the
    // existing steps ON CONFLICT update. Dropping either breaks this named assert.
    let src = include_str!("../src/storage/store_trace.rs");
    let conflict = src
        .split("INSERT INTO steps")
        .nth(1)
        .and_then(|rest| rest.split("ON CONFLICT(id) DO UPDATE SET").nth(1))
        .and_then(|rest| rest.split('"').next())
        .expect("steps ON CONFLICT clause present in store_trace.rs");

    assert!(
        conflict.contains("content_sha256=excluded.content_sha256"),
        "sql_shape_assigns_content_sha256: {conflict}"
    );
    assert!(
        conflict.contains("truncations_json=excluded.truncations_json"),
        "sql_shape_assigns_truncations_json: {conflict}"
    );
    assert!(
        conflict.contains("content=excluded.content"),
        "sql_shape_keeps_content_assignment: {conflict}"
    );
    assert!(
        conflict.contains("meta_json=excluded.meta_json"),
        "sql_shape_keeps_meta_json_assignment: {conflict}"
    );
}
