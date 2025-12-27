use assay_core::storage::store::Store;
use assay_core::trace::schema::{EpisodeEnd, EpisodeStart, StepEntry, ToolCallEntry, TraceEvent};
use tempfile::tempdir;

#[test]
fn test_storage_smoke_lifecycle() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("trace.db");

    // 1. Open Store (init schema)
    let store = Store::open(&db_path)?;
    store.init_schema()?;

    // 2. Insert Events (Episode)
    let ep_id = "ep-1";
    let start = TraceEvent::EpisodeStart(EpisodeStart {
        episode_id: ep_id.into(),
        timestamp: 1000,
        input: serde_json::json!({"prompt": "Hello"}),
        meta: serde_json::Value::Null,
    });

    let step = TraceEvent::Step(StepEntry {
        episode_id: ep_id.into(),
        step_id: "step-1".into(),
        idx: 0,
        timestamp: 1001,
        kind: "invoke".into(),
        name: Some("agent".into()),
        content: Some("Thinking...".into()),
        meta: serde_json::Value::Null,
        content_sha256: Some("hash".into()),
        truncations: vec![],
    });

    let tool = TraceEvent::ToolCall(ToolCallEntry {
        episode_id: ep_id.into(),
        step_id: "step-1".into(),
        timestamp: 1002,
        call_index: Some(0),
        tool_name: "search".into(),
        args: serde_json::json!({"q": "rust"}),
        args_sha256: None,
        result: None,
        result_sha256: None,
        error: None,
        truncations: vec![],
    });

    let end = TraceEvent::EpisodeEnd(EpisodeEnd {
        episode_id: ep_id.into(),
        timestamp: 2000,
        outcome: Some("pass".into()),
        final_output: None,
    });

    // Batch insert
    store.insert_batch(&[start, step, tool, end], None, None)?;

    // 3. Verify via Raw SQL (since Store READ API is not fully exposed yet in plan, or I should treat Store as opaque?)
    // The plan said: "Store APIs: insert/query helpers". I implemented insert helpers.
    // I can open a new connection to check, or expose a simple query helper for testing?
    // Let's open a raw connection to verify content

    let conn = rusqlite::Connection::open(&db_path)?;

    // Check Episode
    let count: i64 = conn.query_row("SELECT count(*) FROM episodes", [], |r| r.get(0))?;
    assert_eq!(count, 1);

    // Check Step
    let step_count: i64 = conn.query_row("SELECT count(*) FROM steps", [], |r| r.get(0))?;
    assert_eq!(step_count, 1);

    // Check Tool
    let tool_count: i64 = conn.query_row("SELECT count(*) FROM tool_calls", [], |r| r.get(0))?;
    assert_eq!(tool_count, 1);

    Ok(())
}
